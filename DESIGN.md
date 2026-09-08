# Dshnext — 原生 Rust 重构设计文档

DshDesk（Tauri + React）的原生重写。**后端逻辑整体复用，前端换成纯 Rust GPU 渲染**，目标是把「低占用」和「高视觉」同时拿到。

- 状态：**阶段 0～5 全部完成**（可行性 / 视觉地基 / 后端接入 / 页面移植 / 收尾 / 切页动画与排版收口）。六个页面全部移植，模态、toast、Ctrl+1..6 都在；三轮视觉验收全 pass；`iced_test` 14 例全绿；性能逐项实测（切页动画后空闲仍归零）；单 exe + NSIS 安装包已产出，与上一代六页并排对比图在 `shots/compare-p5/`。唯一未在本机闭环的是「干净 Win10 VM 首帧复核」——需要一台无重复适配器的机器，见 §9「首帧真相」与 §13 待办。
- 上一代（Tauri + React）：已于 2026-09-05 从工作区删除，代码在 git 历史 `11a2401^` 之前（`git show 11a2401^:src-tauri` 等可翻出）；它出过 NSIS 安装包，功能完整
- 本仓库根即 Dshnext：`src/core/` 是从上一代直接复制的后端模块（1163 行），`phase0/` 是探针工程与实测报告，`docs/*.tauri-reference` 是对照用的旧文件

---

## 1. 目标与非目标

### 硬目标

下表的「实测」列是阶段 0 在同一台机器上量出来的真实数字，不是估计。

| 维度 | 上一代（Tauri/WebView2，实测） | Dshnext 目标 | 阶段 0 实测 |
|---|---|---|---|
| 空闲 CPU | 0.26%（全核）/ 3.6%（单核） | **≈ 0%**（不重绘就不出帧） | ✅ **0.0000%**，120s 内 0 帧 |
| 空闲 GPU | 1.72% | **≈ 0%** | ✅ **0.00%** |
| 常驻内存（私有工作集） | **199 MB**（7 个进程） | GPU 后端 **≤ 100 MB**<br>软件后端 **≤ 30 MB** | ✅ **88.5 MB**（阶段 4 产品，本机 DX12，稳定不爬）<br>**15.3 MB**（tiny-skia 探针） |
| 冷启动到**首帧** | 236 ms | **≤ 300 ms** | ⚠️ 本机 GUI+DX12 **~1.27 s**（驱动枚举 6 个重复适配器的机器特异性开销，见 §9「首帧真相」）；控制台子系统 136 ms、软件后端 212 ms |
| 分发体积 | 3 MB + 依赖系统 WebView2 | **单 exe ≤ 18 MB，零运行时依赖** | ✅ **17.31 MB**（含 4.34 MB 字体；`lto=false` + `+crt-static`） |
| 视觉质量 | 软阴影/圆角/过渡齐全 | **不退步**（软阴影、圆角、渐变、过渡动画全保留） | ✅ 软阴影/圆角观感一致 |
| 中文质量 | WebView2 灰度 AA | **不退步或更好** | ✅ 无豆腐块，等宽数字对齐 |

内存目标原写「≤ 40 MB」，阶段 0 证明这在有独显驱动的机器上不可能：79.8 MB 里约 75 MB 是 NVIDIA 用户态驱动 + wgpu 设备/队列的常驻开销，iced 自身加内嵌字体只占 5 MB 左右。**因此改为按后端分档**，判据仍是「必须显著低于上一代」——实测 199 → 79.8 MB，降幅 60%。

内存目标在阶段 1 放宽到「≤ 100 MB」，那时 demo 页实测 98.6 MB。**阶段 3 的真实六页反而更省：72.8 MB**——demo 页刻意把六类按钮 × 三档尺寸 × 禁用态、12 个 svg、三档阴影全堆在一屏，比任何真实页面都密。真实页面单页控件少、字形集中，稳态回落到 73 MB 左右，比阶段 0 探针的 79.8 还低（探针那次是 `Backends::all()` 之外的窗口更大）。目标保持 ≤ 100 MB 不变，留出插件市场上百行列表的余量。

「冷启动」也从模糊的「到首帧」改成明确区分**窗口出现**与**首帧画好**：Tauri 版空窗口 92 ms 就弹出来了，内容要等 WebView2 加载 bundle（236 ms）；iced 窗口慢 31 ms（初始化 wgpu 设备），首帧快 91 ms。用户感知的是后者，所以指标锚在首帧。

### 非目标

- 不跨平台。只做 Windows 10/11 x64（上一代也是）。
- 不自研渲染器。用现成 GPU UI 框架，不写 wgpu 管线（那是造框架不是写 app）。
- 不追求 3D/光追效果。「强美术」指的是 2D 界面质感，不是 Lumen 那类实时 GI。
- 不改变产品功能。页面、交互、数据目录结构与上一代一致，用户可无缝换用。

### 为什么值得做

上一代的结论是「WebView2 不是性能瓶颈，harness 本身才是」。这依然成立——所以**这次重构的正当理由不是性能数字，是三件具体的事**：

1. **零依赖分发**：不再要求目标机器有 WebView2 runtime。
2. **启动器该像工具而不是网页**：原生文本渲染、原生滚动惯性、瞬时冷启动。
3. **视觉可控性**：阴影/动画由自己的 shader 决定，不受 WebView 版本差异影响。

如果做完实测发现内存/CPU 没明显好转且视觉退步，**这个方向应当被放弃，回到 Tauri 版**。这是本文档预设的失败判据，见 §9。

---

## 2. 技术选型

### 结论：iced 0.14 + wgpu（DX12）

调研了 iced / Slint / Xilem+Vello / GPUI / egui / Freya 六条路线（2026 年状态），选 iced。

| 框架 | 版本 | 渲染 | 阴影 | CJK/IME | 空闲重绘 | 成熟度 | 许可 |
|---|---|---|---|---|---|---|---|
| **iced** | 0.14.0 | wgpu 27（DX12/Vulkan）+ tiny-skia 软回退 | ✅ 内置 `Shadow{color,offset,blur_radius}`，与圆角合并进单个 SDF quad shader | cosmic-text 0.15；0.14 首次做 IME（preedit 窗口、光标感知，修了 6 个中文输入 bug） | ✅ **否**（reactive rendering，仅状态变化时出帧） | 生产可用，但节奏慢 | MIT |
| Slint | 1.17.1 | Windows 默认 FemtoVG；Skia 需开 feature | ✅ 但 `spread`/`inner-shadow` 仅 Skia | 软件渲染器**仅支持西文** | 事件驱动+局部重绘 | 最成熟，承诺 1.x API 稳定 | GPLv3 / 免版税 / 商业 |
| Xilem+Masonry | 0.4.0 | Vello（GPU compute） | 未验证 | Parley/Fontique | retained | ⚠️ experimental，总下载 1 万 | Apache-2.0 |
| GPUI | crates.io 0.2.2（停更 10 月） | Windows=DirectX+DirectWrite | Zed 实测有 | DirectWrite，理论最佳 | 未验证 | ⚠️ 官方 README 写「be on macOS or Linux」，Windows 后端未单独发布 | Apache-2.0 |
| egui | 0.36.1 | wgpu/glow | 需手绘 | 需自装字体，无复杂 shaping / 无 fallback | ✅ idle 不耗 CPU，但 layout 每帧重算 | 极成熟 | MIT/Apache |
| Freya | 0.4.3 | Skia | ✅ `.shadow()` | 未验证 | 未验证 | 总下载 4 万 | MIT |

### 选 iced 的四条硬理由

1. **空闲真的不出帧**。PR #2662「Reactive Rendering」把运行时从「每个 event 无条件出帧」改成「只在 update 或 widget 请求时出帧」。这是「不吃 CPU/GPU」的直接保障，不用自己想办法。
2. **软阴影是一等公民，且几乎免费**。`Shadow{color, offset, blur_radius}` + `Border{radius}` 在 0.14 里合并成**单个 SDF quad shader** 一次绘制。这条至关重要——上一代那套「去框化软阴影浮起」的视觉，在 WPF 里是每帧 GPU 模糊（会糊字、吃填充率），在 iced 里是 SDF 解析式求值，成本可忽略。**视觉不用为性能让步。**
3. **IME 是 0.14 的核心新特性**。纯 Rust GUI 里对中文输入投入最明确的一家。启动器有 API Key、版本名、插件源三处输入框，中文输入法必须能用。
4. **控件覆盖度够**：侧边栏=`column`+`button`；卡片=`container`（Border+Shadow）；列表=`table`（0.14 新增）/`scrollable`；表单=`text_input`/`checkbox`/`toggler`/`pick_list`/`combo_box`；日志控制台=`text_editor` 或 `rich_text`+`span`，配 `scrollable` 的 **`auto_scroll`（0.14 新增，正是日志跟随尾部要的）**；逃生口有 `canvas` 和 `shader`（自定义 wgpu pipeline）。

### 落选原因（简）

- **Slint**：Windows 默认落到 FemtoVG（官方自述文本/路径质量「sometimes sub-optimal」），要好画质必须开 Skia，而 Skia 的 Windows 构建有一串脏活（路径含空格、260 字符上限、MSVC 链接、VC++ redist）。更致命的是软件渲染器**只支持西文**——无 GPU 机器上中文界面直接废掉。
- **GPUI**：视觉天花板最高（Zed 是活证据，Windows 走 DirectX+DirectWrite），但官方 crate 停更且声明只支持 mac/Linux，Windows 后端在 zed 仓库里未单独发布。外部要用只能依赖第三方快照转发 crate（下载量数百）。**把项目押在第三方 re-publish 上，风险不可接受。** 备选见 §9。
- **Xilem/Vello**：自述 experimental，Vello 甚至没有软件回退（无 compute shader 的 GPU 直接跑不了）。
- **egui**：immediate mode 每帧重算 layout，官方承认大 UI 会慢；默认字体无 CJK、无 shaping、无 fallback，视觉天花板明显不够。

### 依赖清单（拟）

iced 0.14.0（2025-12-07 发布，crates.io 当前 max_version）。**默认 features 是**：
```
default = ["wgpu", "tiny-skia", "crisp", "web-colors", "thread-pool", "linux-theme-detection", "x11", "wayland"]
```

```toml
[dependencies]
iced = { version = "0.14", default-features = false, features = [
    "wgpu",              # GPU 渲染（含全部后端；Windows 走 DX12/Vulkan）
    "tiny-skia",         # 软件回退：无可用 GPU 时仍能显示（Slint 那条路踩的坑，这里要避开）
    "crisp",             # 默认开，像素对齐，小字更锐
    "advanced-shaping",  # ★ 中文必须！见 §6
    "advanced",           # 访问底层 widget/renderer API（自造 modal 需要）
    "canvas",            # 图标兜底手绘、自定义绘制
    "svg",               # 描边图标
    "tokio",             # 复用后端的 tokio 运行时
    "thread-pool",
] }
tokio        = { version = "1", features = ["process", "io-util", "time", "sync", "macros"] }
reqwest      = { version = "0.12", default-features = false, features = ["rustls-tls", "json", "stream"] }
serde        = { version = "1", features = ["derive"] }
serde_json   = "1"
zip          = { version = "2", default-features = false, features = ["deflate"] }
dirs         = "5"
futures-util = "0.3"
open         = "5"       # 替代 tauri-plugin-opener

[dev-dependencies]
iced_test = "0.14"       # 端到端测试，见 §9
```

**刻意不开的 features**：
- `unconditional-rendering` —— 会退回「每个 runtime event 都出帧」，直接毁掉空闲零占用（§8）
- `web-colors` —— 默认开启，但 iced 自己的注释说这是为了匹配浏览器的 sRGB-linear 混色、并标注为 "broken"。**我们不是网页，应显式关掉**（用 `default-features = false` 已排除），让阴影/半透明按物理正确方式混合
- `basic-shaping` —— 与 `advanced-shaping` 互斥语义，开了会让默认 shaping 退回 Basic（中文豆腐块）
- `debug` / `time-travel` / `hot` / `sysinfo` —— 开发期可临时开 `debug`（带 devtools），release 不带


---

## 3. 后端复用与解耦

`src/core/` 里的 6 个模块（1163 行）是从上一代直接复制的，**业务逻辑一行不改**，只需切断 Tauri 耦合。

| 模块 | 行数 | 职责 | 耦合情况 |
|---|---|---|---|
| `store.rs` | 65 | 配置读写、数据目录定位 | ✅ 零耦合，直接用 |
| `profiles.rs` | 186 | profile 增删改查、目录模板 | ✅ 零耦合，直接用 |
| `envres.rs` | 249 | 环境探测、版本查询、PATH 拼装 | ✅ 零耦合，直接用 |
| `installs.rs` | 218 | Node 下载解压、dsh/pnpm 安装 | ⚠️ `AppHandle`/`Emitter` 发进度 |
| `procman.rs` | 199 | spawn dsh、日志转发、杀进程树 | ⚠️ `AppHandle`/`Emitter` 发日志、`tauri::async_runtime::spawn` |
| `plugins.rs` | 246 | 插件增删、npm 市场检索 | ⚠️ 同上 |

### 解耦手法：把 `Emitter` 换成 channel

三个模块用 Tauri 的事件系统往前端推流（进度、日志、退出码）。改造只动通道，不动逻辑：

```rust
// core/event.rs — 新增，唯一的新抽象
#[derive(Debug, Clone)]
pub enum CoreEvent {
    EnvProgress { task: String, line: String },
    Log { profile: String, stream: LogStream, line: String, ts: i64 },
    Url { profile: String, url: String },
    Exit { profile: String, code: i32 },
}

#[derive(Debug, Clone, Copy)]
pub enum LogStream { Stdout, Stderr, System, Plugin }

/// 后端只认这个：一个能发事件的东西
pub type EventSink = tokio::sync::mpsc::UnboundedSender<CoreEvent>;
```

替换规则（机械替换，可逐文件核对）：

| 原（Tauri） | 新（Dshnext） |
|---|---|
| `app: AppHandle` 参数 | `tx: &EventSink` 参数 |
| `app.emit("dsh-log", json!({...}))` | `tx.send(CoreEvent::Log{..})` |
| `app.emit("env-progress", ...)` | `tx.send(CoreEvent::EnvProgress{..})` |
| `tauri::async_runtime::spawn` | `tokio::spawn` |
| `app.get_webview_window("webui-x").close()` | 见 §5 WebUI 窗口方案 |

iced 侧用 `Subscription::run` 把 channel 接收端变成消息流，事件直接进 `update()`——比 Tauri 的 `listen` 更直接，且**类型安全**（枚举替代 JSON，编译期就能查出字段错误）。

**实现细节（阶段 2 踩到的）**：`Subscription::run` 的 builder 是裸函数指针 `fn() -> impl Stream`，**捕获不了任何状态**，所以 channel 不能建好再传进去。`src/bridge.rs` 的做法是把两端都放全局：`main` 调 `bridge::init()` 建 channel，发送端进 `OnceLock<EventSink>`（后端从 `bridge::sink()` 取），接收端进 `Mutex<Option<EventStream>>` 等订阅第一次启动时 `take()`。同理 `ProcMap` 要跨 `Task` 共享而 future 必须 `'static`，也放 `OnceLock<Arc<ProcMap>>`。

### 需要重写的部分

`commands.rs`（Tauri 的 22 个 `#[tauri::command]`）**整体废弃**，改为 iced 的 `Task::perform(async_fn, Message::Done)`。（对照用的 `docs/commands.rs.tauri-reference` 碎片已随清理删除，对照看 git 历史第一代 `src-tauri/src`。）

进程树清理（`taskkill /PID /T /F`）与 `CREATE_NO_WINDOW`（`0x0800_0000`）标志原样保留——这两个是 Windows 上的必需品，跟 UI 框架无关。

---

## 4. 架构与目录

iced 是 Elm 架构：`State → view() → Element → 用户操作 → Message → update() → 新 State`。天然契合「状态变化才重绘」。

**现状架构快照（组件清单、分层依赖方向、关键链路）维护在 [docs/architecture/current-state.md](docs/architecture/current-state.md)——架构改动后更新那份，本节只留目录总览。**

```
仓库根（原 Dshnext/，第一代删除后提升为根）
├── DESIGN.md                   # 设计决策与阶段日志（本文档）
├── AGENTS.md                   # 踩坑纪律（给接手的 agent）
├── docs/
│   └── architecture/current-state.md   # 现状架构快照（活的架构地图）
├── assets/
│   ├── icons/                  # app.ico/master.png/window-64.rgba：应用图标；*.svg：界面图标
│   └── fonts/                  # 内嵌字体，见 §6
├── phase0/                     # 探针工程（保留不删），性能数字唯一复现处
├── packaging/                  # NSIS 安装包
└── src/
    ├── main.rs                 # 入口：CLI、窗口设置、DX12 限定、字体注入
    ├── app.rs                  # 顶层 State / Message / helper（update/view 已拆出）
    ├── update.rs               # 全部 update / subscription（阶段 3 从 app.rs 拆出）
    ├── bridge.rs               # core↔iced 桥：channel 两端与 ProcMap 的全局持有
    ├── tests.rs                # iced_test 14 例
    ├── theme.rs                # 设计令牌（两套主题 + 字号/间距/圆角常量），见 §6
    ├── core/                   # ← 复用的后端（1255 行），仅解耦不改逻辑，零上层依赖
    │   ├── mod.rs
    │   ├── event.rs            # 新增：CoreEvent / EventSink
    │   ├── store.rs
    │   ├── profiles.rs
    │   ├── envres.rs
    │   ├── installs.rs
    │   ├── procman.rs
    │   └── plugins.rs
    ├── pages/                  # 六页 + 共享外壳（shell/侧边栏/环境光/帏幕都在 mod.rs）
    │   ├── mod.rs
    │   ├── home.rs
    │   ├── profiles.rs
    │   ├── plugins.rs
    │   ├── env.rs
    │   ├── console.rs
    │   └── settings.rs
    └── ui/                     # 自造组件 + 渲染管线，见 §7
        ├── mod.rs              # 文本纪律入口：txt/txt_bold/mono（唯一文本通道）
        ├── card.rs
        ├── button.rs
        ├── widgets.rs          # 通用控件收编处（tag/list_row/input/dropdown/segmented/toast/…）
        ├── icon.rs             # 描边图标，见 §6
        ├── anim.rs             # 过渡动画驱动，见 §7.1
        ├── modal.rs            # 自造（iced 无 modal），见 §7.2
        ├── reveal.rs           # 切页入场位移，见 §7.7
        ├── titlebar.rs         # 无边框窗口三件套（细条 40 + 品牌单元 64 + 缩放热区），见 §7.6
        ├── frosted.rs          # 假高斯模糊玻璃卡（布局跟随内容的自定义 widget）
        ├── glass_pipeline.rs   # 自定义 wgpu primitive 管线（背景场/玻璃卡/渐隐帏幕三模式）
        ├── glass.wgsl          # 上述管线的 fragment shader
        └── glow_mesh.rs        # 光球参数唯一来源 + tiny-skia 回退的 Mesh 扇形
```

顶层状态形状（对照上一代 `App.tsx` 的 shared props）：

```rust
struct Dshnext {
    page: Page,                        // 当前页
    config: Config,                    // 设置（core::store）
    env: Option<EnvStatus>,            // 环境探测结果
    profiles: Vec<ProfileInfo>,        // 版本列表
    procs: Vec<ProcStatus>,            // 运行中实例
    logs: VecDeque<LogLine>,           // 环形缓冲，上限 2000
    selected: String,                  // 当前选中版本
    urls: HashMap<String, String>,     // profile -> WebUI 地址
    busy: Option<String>,              // 全局忙提示
    toasts: Vec<Toast>,
    modal: Option<Modal>,
    anim: AnimState,                   // 见 §7
    tx: EventSink,                     // 交给 core 用
}
```

---

## 5. WebUI 窗口：唯一的真实取舍

上一代用 `WebviewWindowBuilder` 弹一个内置窗口显示 harness 的 Web 界面。**Dshnext 没有 WebView，这个能力消失了。**

三个方案：

| 方案 | 做法 | 代价 |
|---|---|---|
| **A. 交给系统浏览器**（选定） | `open::that("http://127.0.0.1:3080")` | 用户在浏览器里用 harness，不在启动器窗口内 |
| B. 可选挂载 WebView2 | 检测到 runtime 时用 `webview2-com` 开独立窗口 | 又把 WebView2 依赖引回来，违背零依赖目标 |
| C. 自己实现浏览器 | — | 不可能 |

**选 A。** 理由：
- 启动器的职责是「管理与启动」，不是「承载 Web 界面」。上一代内置窗口本质就是个套壳浏览器，没有增值。
- 浏览器里用户有完整的开发者工具、多标签、缩放、密码管理。
- 零依赖目标是本次重构最实在的收益，不能为一个套壳窗口牺牲。

实现变化：
- 设置里的「Web 界面打开方式：启动器内置窗口 / 系统浏览器」**降级为单一行为**，该项移除。
- `auto_open` 保留（启动成功后是否自动打开浏览器）。
- `procman.rs` 里进程退出时关闭对应 WebUI 窗口的逻辑删除（浏览器标签由用户自己管）。
- 首页「打开界面」按钮行为改为 `open::that(url)`。

这是**功能上唯一的退步**，需在 README 里明说。

### 5.1 桌面窗口模式（2026-09-07 首做 `--app`，2026-09-08 改 WebView2）

用户要「客户端打开」又反感套壳。先做了第四选项 `--app=<url>`（让已装 Chromium 浏览器开无地址栏独立窗口，启动器零依赖）——但实测两个硬伤：**任务栏/MyDockFinder 把它归到浏览器名下**（进程是 msedge.exe），且**那条标题栏是浏览器画的、换不掉**。PWA 能治（独立 AUMID + 图标 + WCO 无边框），但 token 每次启动都变、装不了固定起始地址，死路。

于是改用 **WebView2**（`core/webview.rs`）：这是**启动器自己的顶层窗口**（进程 dshnext.exe、挂 DSH.ico、标题「DshDesk — WebUI」），所以 dock 里独立成项、边框归自己。代价是多一棵 WebView2 进程树（~150-200MB，同内核，跑网页躲不掉——见上「轻量套壳不存在」）。WebView2 运行时 Win10/11 自带，**不打包内核**，仍单 exe；缺运行时直接报错、按用户要求不做静默回退。

实现要点（都踩过才写）：
- **不能用 tao/wry 的窗口层**：tao 是 winit 分支，与 iced 主线程的 winit 在同进程抢全局状态（窗口类/DPI），副线程建 tao 窗口会原生崩溃。所以窗口用 `windows` crate **手写原生 Win32**（RegisterClass/CreateWindowEx/GetMessage 泵），只借 wry 做 WebView2 那层（经 raw-window-handle 递 HWND）。
- WebView2 的 COM 对象 + 窗口必须建在**自带消息泵的独立线程**（同 tray.rs 约束），且线程要先 `CoInitializeEx`。主线程用 `PostMessageW(WM_APP_OPEN, Box<url>)` 跨线程投递。
- 打开链路仍收口 `Message::OpenWebUi`：`config.app_window` 开 → `webview::open`；关 → `open::that_detached`。启动页「打开方式」分段控件切换即生效即落盘（config+draft 同写）。
- 图标：`dsh.ico` 内嵌，运行时解析 ICO 目录取最大图 → `CreateIconFromResourceEx` → 类图标。

已知边界：单窗口复用（多 profile 共用一个，再开导航到新 token）；关窗=隐藏不销毁；与主启动器同进程，dock 里是否进一步拆独立项需每窗口 AUMID（`ITaskbarList3::SetAppUserModelID`，但 windows 0.62 未投影该方法，暂不做）。

---

## 6. 视觉系统：如何不退步

上一代的视觉语言（去框化、软阴影浮起、大留白、单强调色、meta 信息带）**全部保留**，逐项映射到 iced。

### 设计令牌（`theme.rs`）

两套主题，与上一代 CSS 变量一一对应：

```rust
pub struct Palette {
    pub bg_app: Color, pub bg_side: Color,
    pub surface_1: Color, pub surface_2: Color, pub surface_3: Color,
    pub hover: Color,
    pub border: Color, pub border_mid: Color, pub border_hi: Color,
    pub text: Color, pub text_2: Color, pub text_3: Color,
    pub accent: Color, pub accent_hi: Color, pub accent_soft: Color,
    pub teal: Color, pub ok: Color, pub warn: Color, pub bad: Color,
    pub shadow_card: Shadow, pub shadow_pop: Shadow, pub shadow_btn: Shadow,
    pub input_bg: Color,
}

pub const LIGHT: Palette = Palette { /* #f3f4fa / #ffffff / #5160ea / #0fb898 ... */ };
pub const DARK:  Palette = Palette { /* #060607 / #0e0e10 / #5b76ff / #2fd6b3 ... */ };
```

色值直接抄 `../src/styles.css` 的 `:root[data-theme]` 两组，保证观感一致。

### 逐项映射

| 视觉特性 | 上一代（CSS） | Dshnext（iced） | 成本 |
|---|---|---|---|
| 软阴影浮起卡片 | `box-shadow: 0 16px 40px -20px rgba(24,30,80,.14)` | `container::Style { shadow: Shadow{ blur_radius, offset, color } }` | ✅ 内置，SDF 一次绘制 |
| 圆角 | `border-radius: 16px` | `Border { radius: 16.0.into() }` | ✅ 内置，与阴影同 shader |
| 主按钮渐变 | `linear-gradient(180deg, hi, base)` | `Background::Gradient(Linear)` | ✅ 内置（阶段 1 已验证，角度与 CSS 同向） |
| hover 过渡 | `transition: background .14s` | 自造，见 §7 | ✅ 阶段 1 已落地，订阅生命周期已验证 |
| 主题切换 | `data-theme` 换变量组 | 换 `Palette` 常量，即时生效 | ✅ 免费（颜色插值动画要自己写） |
| 中文字体 | 系统 `Microsoft YaHei UI` | 内嵌字体 + cosmic-text | ⚠️ 见下 |
| 描边图标 | 内联 SVG | `iced::widget::svg` 或 `canvas` 画路径 | ✅ 内置（阶段 1 验证动态换色） |
| 等宽数字 | `font-variant-numeric: tabular-nums` | 选等宽字体或带 tnum 的字体 | ⚠️ 字体要选对 |

### 文本渲染（最容易踩的坑）

cosmic-text 是 Rust 的 CJK 事实标准：shaping 用 HarfRust，fallback 表直接抄 Chromium/Firefox 并按 locale 区分 Han（避免中日韩字形串味）。

**关键事实（已核对 iced_core 0.14 文档）**：`text::Shaping` 的默认值**由编译期 feature 决定**，不是固定的：

| feature 状态 | `Shaping` 默认值 | 中文表现 |
|---|---|---|
| 都不开 | `Auto` | ✅ 可用（纯 ASCII 走快路径，否则自动转 advanced） |
| 开 `basic-shaping` | `Basic` | ❌ **豆腐块 / 字形错乱**（无 shaping、无 fallback） |
| 开 `advanced-shaping` | `Advanced` | ✅ 最稳（始终完整 shaping + fallback） |

三个变体的官方描述：
- `Basic` —— "No shaping and no font fallback"，很便宜但「will not display complex scripts properly」
- `Advanced` —— "Advanced text shaping and font fallback"，文档警告「Advanced shaping is expensive! You should only enable it when necessary」
- `Auto` —— "Auto-detect the best shaping strategy from the text"，ASCII 走 basic，其余走 advanced

**决策**：开 `advanced-shaping`，让全局默认就是 `Advanced`。

理由：这是个中文界面，几乎每个字符串都含中文，`Auto` 的探测收益接近零而多一层判断；更重要的是**默认值安全**——万一某处漏了显式设置，`Advanced` 兜底不会出豆腐块。代价是纯 ASCII 文本（路径、版本号、日志）也走贵路径，但启动器的文本量是几百个字符级别，不是编辑器，可忽略。

补充纪律（双保险）：
- 在 `ui/mod.rs` 封装 `fn text(s) -> Text`，显式 `.shaping(Shaping::Advanced)`，**全项目禁止直接用 `iced::widget::text`**
- **内嵌字体**而不是依赖系统字体，用 `iced::Settings { fonts }` 加载。保证任何 Windows 上观感一致，也避免「用户系统没装雅黑」

### 字体方案（阶段 0 已落地，实测数字）

生成脚本 `phase0/tools/build_fonts.py`，产物在 `assets/fonts/`：

| 文件 | 大小 | 字形数 | 覆盖 |
|---|---|---|---|
| `NotoSansSC-Regular.subset.ttf` | 2.16 MB | 7750 | ASCII + Latin-1 + 标点 + 全角 + 假名 + **GB2312 全 6763 汉字** |
| `NotoSansSC-SemiBold.subset.ttf` | 2.16 MB | 7750 | 同上 |
| `CascadiaMono.subset.ttf` | 18 KB | 246 | 拉丁与符号（中文交给 sans 回退） |
| **合计** | **4.34 MB** | | |

原文写「常用 3500 字，约 1–2 MB」低估了。GB2312 全集是中文界面的地板（用户可能给 profile 起任何名字，dsh 的日志也原样打印），实测 4.34 MB。曾试过扩到 GBK（21886 汉字）→ 14.21 MB，单 exe 直接超标，**放弃**：子集外的字由 cosmic-text 的 Windows 回退表（`Script::Han` + locale `zh-CN` → `Microsoft YaHei UI`）接管，实测 `龘㸻鑫燚囍` 等生僻字正常显示，无豆腐块。

**三条踩过的坑，构建字体时不能省：**

1. **`tnum` 指望不上，等宽数字只能靠字体天然等宽。** iced 从不设置 cosmic-text 的 `font_features`（`iced_graphics` 全库搜不到该字段），所以 CSS 那个 `font-variant-numeric: tabular-nums` 在 iced 侧**没有对应开关**。所幸 Noto Sans SC 与 Cascadia Mono 的十个数字 advance 本来就完全相同。**纪律：换字体必须先验数字 advance 是否一致**，否则表格里的数字会跳。
2. **`varLib.instancer` 必须带 `--update-name-table`**，否则每个静态实例都继承变体字体的默认名（`Noto Sans SC Thin`）。
3. **必须显式写 name ID 16/17（typographic family/subfamily）。** `fontdb` 优先用 ID 16 做家族键，回退才用 ID 1；只改 ID 1 会让 SemiBold 注册成独立家族，于是 `Font::with_name("Noto Sans SC") + Weight::Semibold` **静默落到系统字体上**——阶段 0 第一轮就是这么错的，截图上看不出来，得查 name table 才发现。

### 暗色精修（阶段 1.5，借鉴 orevx glass-dark）

用户拿 [orevx.ai/llm-dashboard](https://orevx.ai/llm-dashboard) 当审美参照。扒了它的 CSS 变量体系（Tailwind v4 + oklch，`html.dark.glass-dark`），发现它「好看」的三个原因，逐条落到 `theme.rs` 的暗色令牌上（oklch 已转 sRGB 硬编码）：

| 改动 | 旧值 | 新值 | 依据 |
|---|---|---|---|
| surface 色阶 | #0e0e10 / #161619 / #1d1d21 | **#18181a / #232325 / #262728** | orevx 的卡片比背景亮 2.4 倍，我们原来只亮 1.5 倍，浮不起来 |
| 卡片描边 | 无（靠阴影） | **1px rgba(255,255,255,.08)** | 纯黑底上黑阴影几乎不可见（阶段 0 就撞见过），orevx 改用色阶差 + 白描边勾轮廓 |
| 卡片阴影 | blur30/alpha.8 | blur24/alpha.45 | 轮廓交给描边后，阴影只留一点深度 |
| 顶部环境光 | 无 | **linear-gradient(π, #195eb4@16% → #005e50@4% → 透明)，高 260px** | orevx 背景不是死黑，顶部有蓝→青氛围光——「高级感」最大来源 |
| active 导航 | surface_1 + shadow_card | surface_1 + 1px 描边，无阴影 | 同 orevx 的 `.nav active` |
| hero 大数字 | 无 | **48px / 600**（`HERO_NUM_SIZE`） | orevx 把数字当主角 |

**抄不了的两条，已验证等价或放弃：**
- `backdrop-filter: blur(36px)`：iced 的 `window::blur` 在 Windows 是 no-op（§12）。但 orevx 背景近纯色，模糊本来就糊不出东西——**直接画预糊好的渐变带，观感等价**，已实现。
- `letter-spacing: -1.2px`（hero 数字的负字距）：**iced 0.14 的 `Text` 没有 `letter_spacing` API**（`iced_core/src/widget/text.rs` 全部方法里没有，同 `tnum` 一类限制）。只能放弃负字距，48px/600 单独用已经比原来强很多。

**亮色完全不动**：orevx 没有亮色主题，亮色维持上一代「白卡 + 阴影浮起、无描边、无环境光」的设计（`card_border`/`ambient_*` 在 LIGHT 里都是 transparent，零成本）。对比图见 `shots/compare-orevx.png`（左 orevx、中旧暗色、右新暗色）。

### 排版与层级收口（阶段 5，按 `frontend-design` skill 复核）

上一代 CSS 以 14px 为根用 rem 写，直译到 iced 后散出 **9 / 9.5 / 10 / 11 / 11.6 / 12 / 12.5 / 13 / 13.5 / 13.7 十档**——相邻两档差 0.1~0.5px，肉眼分不出层级，只显得乱。这一轮把四件事收成令牌，页面里**只许引常量**：

| 令牌 | 值 | 用途 | 收口前的问题 |
|---|---|---|---|
| `FS_MICRO`…`FS_HERO` | 10 / 11.5 / 12.5 / 13 / 13.5 / 15 / 19 / 30 | 八档字号阶梯 | 十档里有五个是「邻近怪值」，改一处不知道动的是哪一层语义 |
| `GAP_CARD` / `GAP_SECTION` | 16 / 24 | 同级卡片之间 / 页头之后 | 六个页面一律 18，「页头→内容」和「卡片→卡片」看着是同级关系 |
| `R_CARD` / `R_HERO` | 16 / 20 | 普通卡片 / 英雄区 | 一套圆角用到底，hero 和它下面的列表卡分不出主次 |
| `shadow_card` / `shadow_hero` | blur 24 / 34（暗色） | 同上两档高度 | 同上。**不复用 `shadow_pop`**——那是浮层（模态/下拉）的量，用在常驻内容上会显得整块要脱离页面 |

节奏两档要靠 `widgets::page_stack(head, body)` 嵌一层 Column 才做得到：一个 `Column::spacing` 对所有间隙一视同仁。

按钮变体加了一档 **`Accent`**（surface 底 + accent 描边 + accent 字），解决「主 CTA 唯一」：渐变 + 彩色投影是最重的强调手段，此前版本管理每行的「启动」、环境页三行的「安装」、插件市场每行的「安装」全在用它，一屏七八个，谁都不显眼。现在 `Primary` 每屏至多一个（首页「启动 harness」、版本管理「+ 新建版本」、设置「保存」、模态确认），行内正向动作一律 `Accent`。


---

## 7. 要自己造的东西

网页栈白送、iced 没有的，共七件（§7.7 是阶段 5 追加的）。这是本次重构的**真实工作量所在**。

### 7.1 过渡动画（`ui/anim.rs`）

CSS 的 `transition: background .14s` 在 iced 里没有对应物。做法：

```rust
/// 一个值的补间。update 里推进，view 里读当前值。
pub struct Tween { from: f32, to: f32, start: Instant, dur: Duration, easing: Easing }

impl Tween {
    pub fn value(&self, now: Instant) -> f32 { /* 缓动插值 */ }
    pub fn done(&self, now: Instant) -> bool { /* 是否结束 */ }
}
```

驱动方式**决定了空闲占用能否为 0**：

```rust
fn subscription(&self) -> Subscription<Message> {
    if self.anim.is_animating() {
        // 只在动画期间订阅逐帧刷新
        window::frames().map(Message::Tick)
    } else {
        Subscription::none()   // 空闲：完全不出帧
    }
}
```

**这是本设计里最关键的一条纪律**：动画结束必须把 tween 从 `AnimState` 移除，否则 `frames()` 一直订阅，空闲 CPU 就回不到 0，整个重构的意义就没了。

范围控制：只给 hover 背景色、模态淡入、toast 滑入、主题切换（颜色插值）做过渡。列表滚动交给 iced 自己的 `scrollable`。

### 7.2 模态对话框（`ui/modal.rs`，阶段 3 已落地）

iced 无 modal。用 `stack!` + `opaque`（文档明确说它用来拦截鼠标、防事件穿透）+ `center` 自拼，需要：
- 半透明遮罩（`bg_app` 55% alpha）
- 淡入动画（复用 7.1）
- **ESC 关闭**：`keyboard::on_key_press` 订阅
- 点遮罩关闭、点内容不关闭
- 焦点陷阱（Tab 不跑到背景控件）——iced 的焦点模型待验证，可能需要在模态开启时禁用背景控件

对应上一代的 `PromptModal`（新建/重命名/复制/手动装插件）和 `ConfirmModal`（删除/卸载）。

### 7.3 日志控制台的窗口化（`pages/console.rs`，阶段 3 已落地）

`scrollable` + `column` 只有 primitive culling（视口外不绘制），**布局仍全量计算**。2000 行日志每帧算 layout 会卡。

两个选项：
- **首选**：用 `text_editor`（内部按 buffer 管理，天然支持大文本），只读模式，配 `auto_scroll`。
- 备选：自己做窗口化——按滚动偏移只把可见区间（约 40 行）构造成 `Element`，上下用 `Space` 撑出总高度。

`auto_scroll`（0.14 新增）正是「日志跟随尾部」要的，不用自己算。

### 7.7 切页入场动画（`ui/reveal.rs`，阶段 5 落地）

侧边栏点一下就整块内容瞬间换掉，看不出发生了什么。过渡只有一个量，由补间 `anim::PAGE` 驱动（**1 = 刚切过来、0 = 已落定**，方向刻意反着：`AnimState::value` 对无记录的 key 返回 0.0，正好是落定态，于是冷启动、`--page` 出图都不必预置初值）：

| 量 | 做法 | 为什么不用别的 |
|---|---|---|
| 内容整体下移 14px → 0 | `renderer.with_translation` 包住 `draw` | **不动 padding/height**：那会让每帧重新布局，设置页六张卡要整树重排；平移只影响 draw，布局结果逐帧复用 |

两个必须记住的细节：

1. **不做面纱淡入**（阶段 5 曾实现过，用户看后否掉，2026-09-05 删除）。面纱盖的是**整块区域**而不是内容本身，主区实际背景比 `bg_app` 亮，0.9α 盖上去背景肉眼可见地变暗（实测过渡中主区平均亮度 9.8 vs 落定 25.5），观感像闪屏。「淡入只能靠面纱」的技术结论仍然成立（见下），但代价是背景被染色——位移单独用就够了。模态遮罩用同一招没问题，因为「变暗」在那里正是目的。
2. **不用 `Transformation::scale`**。缩放会连文字一起缩，cosmic-text 在非整数缩放下要么每帧重新栅格化（白扔性能）要么拉伸图集（更糊）。字本来就没有 hinting，不能再糊。

`Reveal` 在 widget 树里是**透明的**——tag/state/children/layout/update 全部直通内容（抄 `iced_widget::themer`），所以补间值变化不引起任何树 diff；`t == 0` 时直接走原路，不推变换，落定后零开销。

侧边栏选中态同帧交叉淡入（`anim::NAV`，同一时长）：旧项 1→0、新项 0→1 一起插值，看着像一块底色滑过去。要点是**新旧两项必须共用一个补间值**，各自计时会看出「内容先到、指示条后到」；`app.prev_page` 就是为它存的。整数 0/1 阶跃会「跳」一下——上一代靠 CSS transition 掩掉，这里只能自己插。

取证方式：`--switch-to <页> --switch-at <毫秒>` 配 `--shot ... --after`，两者的差就是快门落在过渡的第几毫秒（单靠 `--after` 永远只截得到落定态）。删面纱后复测：过渡中间帧与落定帧的主区空白背景平均亮度**恒定 6.0**（不再变暗），两帧像素差只落在内容区（diff bbox y 87..588，即位移中的标题与卡片）。空闲零仍成立：`--drawlog` 显示切页那 5s 出 31 帧，之后每 5s `delta=0`。

### 7.4 虚拟滚动（版本/插件列表）

插件市场可能有上百条。`table`（0.14 新增）或 `scrollable` 的 culling 够不够要实测；若不够，同 7.3 的窗口化手法。

### 7.5 描边图标（`ui/icon.rs`）

上一代是内联 SVG（10 个图标：launch/versions/plugins/env/console/settings/node/harness/download/market/play/sleep/log）。iced 有 `svg` widget，把 SVG 存成 `assets/icons/*.svg` 用 `include_bytes!` 编进 exe。**动态着色已验证（阶段 1）**：`svg::Style{color: Some(c)}` 走的是渲染后像素级 RGB 替换（保留 alpha，见 `iced_wgpu/src/image/vector.rs`），对单色描边图标等价于 `currentColor`；多色图标不适用，但本套图标全是单色描边。阶段 4 补齐剩余图标即可，无需 canvas 兜底。

### 7.6 无边框窗口与自绘标题栏（`ui/titlebar.rs`，阶段 1.5 已落地）

用户要求去掉系统那条带缩放/最大化/关闭的原生外框。`window::Settings { decorations: false }` 一关，**系统标题栏和四周的缩放边框会一起消失**，两样都得自己补：

| 能力 | 做法 | 注意 |
|---|---|---|
| 拖动窗口 | 标题栏包 `mouse_area(...).on_press(DragWindow)` → `window::drag(id)` | **交给系统**（内部走 `WM_NCLBUTTONDOWN`+`HTCAPTION`），不要自己算 delta 再 `move_to`——那样跟手差且每帧都出帧 |
| 最小化 | `window::minimize(id, true)` | |
| 最大化/还原 | `window::toggle_maximize(id)` 再 `.chain(window::is_maximized(id))` 回读 | **必须回读**：本地布尔翻转会和现实脱节（系统可能拒绝），按钮字形就错了 |
| 关闭 | `window::close(id)` | `exit_on_close_request` 默认 true，关掉最后一个窗口即退出进程 |
| 八向缩放 | 窗口边缘铺 6px 透明热区，`on_press` → `window::drag_resize(id, Direction::*)` | 热区必须是 `stack!` 的**最后一个孩子**（`Stack::update` 逆序派发、先到的先捕获）；**中间那块必须是裸 `Space`，不能包 `mouse_area`**，否则整个内容区的点击全被它吃掉 |
| 缩放光标 | `mouse_area(...).interaction(Interaction::Resizing*)` | 只在 `content_interaction == None` 时生效，正好适合透明热区 |

两个 Windows 平台参数不能省（`window::settings::PlatformSpecific`）：
- `undecorated_shadow: true` —— 不开的话无边框窗口和桌面糊成一片，边界完全看不出来。代价是顶部多一条 1px 线（iced 文档明说）。
- `corner_preference: Round` —— 圆角**交给 DWM**（Win11 22000+）。试过自己在最外层容器上画 `border.radius`，结果和方形的窗口表面对不齐、四角露出直角色块；让系统裁、内容只管填满才干净。

标题栏三个按钮的字形直接用文本 `─ □ ❐ ✕`（U+2500 / U+25A1 / U+2750 / U+2715），都在子集字体覆盖范围内，省三个 svg 文件和一次解析。关闭按钮 hover 变红（Windows 惯例），其余变浅色叠加，复用 §7.1 的补间。

### 汇总工作量

| 项 | 难度 | 说明 |
|---|---|---|
| 后端解耦（Emitter → channel） | 低 | 机械替换，1163 行逻辑不动 |
| ~~六个页面 view()~~ | 中 | ✅ 阶段 3 完成 |
| ~~设计令牌 + 主题~~ | 低 | ✅ 阶段 1 完成 |
| ~~过渡动画~~ | **中高** | ✅ 阶段 1 完成，订阅生命周期已验证 |
| ~~模态~~ | 中 | ✅ 阶段 3 完成（opaque 必须包遮罩、ESC 走全局订阅） |
| ~~日志控制台~~ | 中 | ✅ 阶段 3 完成（只构造尾部 200 行，不用 text_editor） |
| ~~字体子集化 + shaping 纪律~~ | 中 | ✅ 阶段 0 完成 |
| ~~图标~~ | 低 | ✅ 阶段 1 验证动态换色，阶段 4 补齐全套 |

---

## 8. 性能纪律

「不吃 CPU/GPU/内存」不是选了 iced 就自动获得的，取决于这几条是否守住：

1. **空闲零订阅**。`subscription()` 在无动画、无运行中实例时必须返回 `Subscription::none()`。已知的常驻订阅有两个，都要按需开关：
   - 动画帧（7.1）——动画结束即取消
   - 进程状态轮询——上一代是 `setInterval(2000)` 无条件轮询；**Dshnext 改为仅在有运行中实例时才起 `time::every(2s)`**，空闲时不轮询

   阶段 0 已验证这条能兑现：不订阅任何周期性来源时，开窗画 2 帧后 **120 秒内一帧都没有**，CPU 与 GPU 都是 0.0000%。
2. **不开 `unconditional-rendering` feature**（iced 用它退回旧的每事件出帧行为）。
3. **`main` 开头限定 GPU 后端为 DX12**（阶段 0 新增，白省 38 MB）：

   ```rust
   // iced 没有暴露 wgpu::Backends 的设置入口，默认是 Backends::all()：
   // Vulkan、DX12、GL 三套加载器全初始化再挑一个，光这份浪费就有 38 MB 常驻。
   // wgpu 在 compositor 创建时才读这个变量，晚于 main，所以进程内设置有效。
   // SAFETY: 单线程启动阶段，尚未创建窗口或后台线程。
   unsafe { std::env::set_var("WGPU_BACKEND", "dx12") };
   ```

   实测私有工作集：`Backends::all()` 117.8 MB → DX12 79.8 MB。三个后端的渲染输出**逐像素完全一致**，限定后端不影响视觉。
4. **日志环形缓冲**用 `VecDeque` 上限 2000，超出从头弹出，避免无限增长。
5. **`view()` 里不做重计算**。过滤/排序/格式化的结果缓存进 State，`view()` 只读不算。
6. **图片/SVG 缓存**。iced 有内部缓存，但确认图标不是每帧重新解析。
7. **release 构建**：`lto`、`codegen-units = 1`、`strip = true`、`panic = "abort"`。阶段 0 实测 `panic = "abort"` 一项就省 2.75 MB（17.97 → 15.22 MB），是唯一还有明显空间的开关。
8. **窗口最小化时不出帧**（winit 默认行为，需确认 iced 未覆盖）。

### 内存构成（阶段 0 归因实测，私有工作集）

知道钱花在哪，才不会在错的地方省：

| 项 | 占用 | 可控性 |
|---|---|---|
| 多后端加载器（`Backends::all()`） | **38 MB** | ✅ 限定 DX12 即可省掉，见上面第 3 条 |
| NVIDIA 用户态驱动 + wgpu 设备/队列/管线 | ~75 MB | ❌ 不在我们控制范围内 |
| 内嵌字体（4.34 MB 文件） | 4.8 MB | 合理，与文件大小相符 |
| MSAAx4 抗锯齿 | 0.5 MB | 可忽略，不必为省它牺牲画质 |
| **软件渲染（tiny-skia）全程总计** | **15.3 MB** | 无 GPU 的机器上反而最省 |


---

## 9. 验收判据与失败退出

### 必须达标（否则本方向失败）

阈值已按阶段 0 实测校准，「测法」列指向可复现的脚本而不是「任务管理器观察」。

| 指标 | 阈值 | 测法 | 阶段 0 结果 |
|---|---|---|---|
| 空闲 CPU | < 0.1%（全核） | `phase0/tools/measure-idle.ps1`，60s | ✅ 0.0000% |
| 空闲 GPU | ~0% | 同上（读 `\GPU Engine(*)\Utilization Percentage`，按 pid 归属） | ✅ 0.00% |
| 空闲出帧 | 开窗后增量为 0 | 探针在 `draw()` 里计数 | ✅ 120s 内 0 帧 |
| 常驻内存 | GPU ≤ 90 MB / 软件 ≤ 30 MB | **私有工作集**（`\Process(*)\Working Set - Private`），六个页面都开过之后 | ⚠️ 阶段 4 产品实测 **88.5 MB**（本机 DX12，稳定不爬；与阶段 3 的 72.8 差异是驱动常驻分配的机器波动，逼近 90 阈值） |
| 首帧画好 | ≤ 300 ms | `phase0/tools/first-paint.ps1`（按窗口表面颜色数判定，不是窗口出现） | ⚠️ **见下方「首帧真相」——阶段 0 的 145 ms 是控制台子系统探针，产品是 GUI 子系统，本机 DX12 实测 ~1.27 s** |
| 单 exe 体积 | ≤ 18 MB | 含内嵌字体与后端依赖 | ✅ **17.31 MB**（阶段 4 `lto=false` + `+crt-static`，见下） |
| 零运行时依赖 | 干净 Win10 VM 能跑 | 不装 WebView2/.NET 的机器 | ✅ **`dumpbin /DEPENDENTS` 确认只剩系统 DLL**（`+crt-static` 静态链 CRT，无 VCRUNTIME140/UCRT，见下「零依赖」段）；干净机器双击实跑仍待真机复核 |
| 中文渲染 | 无豆腐块、无字形错乱、小字清晰 | 六页目检 | ✅ 阶段 3 六页全部目检通过（两轮 judge） |
| 中文输入法 | 三处输入框能打中文、候选框位置正确 | `phase0/tools/ime-drive.ps1` | ✅ preedit/候选/提交/退格全对 |
| 软件回退下中文 | 与 GPU 后端观感一致 | `ICED_BACKEND=tiny-skia` 截图对比 | ✅ 平均差 0.65/255 |
| 视觉不退步 | 与 Tauri 版并排对比，软阴影/圆角/留白/过渡在位 | 截图对比 | ✅ 阴影圆角一致；✅ 过渡/渐变/按钮四态阶段 1 已并排验证（`shots/compare-*.png`） |
| 功能对等 | 全流程通过（新建→启动→日志→停止→删除、插件装卸） | `iced_test`，见下 | ✅ 阶段 4 `iced_test` 14 例全绿（`src/tests.rs`，view↔update 契约）；真实启停链路另由 `--e2e` 覆盖 |

**关于「测法」的两个教训**（写清楚是因为按错的方法量会得出反向结论）：

- **进程树必须递归遍历。** WebView2 是 `dshdesk.exe` → `msedgewebview2.exe`（browser）→ GPU/renderer/utility 子进程共 7 个。只走一层父子关系只找到 2 个，内存少算 230 MB、CPU 少算 100 倍（0.0015% 而不是 0.2573%）——照这个数字对比，上一代反而"更省"。
- **别报 `PrivateMemorySize64`。** 那是提交的虚拟内存，NVIDIA 驱动会撑到 200+ MB 而页面并不驻留。任务管理器「内存」列显示的是 `\Process(*)\Working Set - Private`，本文所有内存数字都用后者。

**首帧真相（阶段 4 排查，推翻了阶段 0 的 145 ms）**：阶段 0 那个漂亮的 145 ms 是**探针**测的，而探针没有 `windows_subsystem="windows"`——它是**控制台子系统**程序。产品为了双击启动不闪黑框，用的是 **GUI 子系统**。同一份产品代码，只改子系统标志，本机实测：

| 配置 | 首帧 |
|---|---|
| 控制台子系统 + DX12 | **136 ms** |
| GUI 子系统 + DX12（当前产品） | **~1270 ms** |
| GUI 子系统 + Vulkan | 497 ms |
| GUI 子系统 + GL | 456 ms |
| GUI 子系统 + tiny-skia（软件） | 212 ms |

用文件计时把 1.27 s 拆开：`main` 入口到 `init-closure` 只 5 ms，**全部耗在 `init-closure → 首次 view`**，即 iced 建 wgpu compositor（适配器枚举 + 请求设备）这一段。与 LTO 无关（关掉 lto 仍 1300 ms）、与日志量无关（`RUST_LOG=off` 只降到 907，且 stderr 重定向到文件仍 1300，说明不是写句柄卡）、与首帧内容复杂度无关（探针场景更简单但同样要枚举适配器）。

**根因是这台机器的驱动**：`iced_wgpu` 的适配器日志显示枚举出 **6 个完全重复的 RTX 4060 Laptop GPU + 1 个 Microsoft Basic Render Driver**。DX12 的 `EnumAdapters1` 在 GUI 子系统进程里逐个探测这 7 个适配器异常慢。**这是本机 NVIDIA 驱动（32.0.16.1062）的病态，不是代码缺陷**——干净机器只有 1 个适配器，不会付这笔钱。

**为什么不干脆换 Vulkan**：Vulkan 本机 497 ms 确实快，但①仍超 300 ms 阈值；②不是所有目标机都有可用 Vulkan 驱动（老 Intel、部分 VM），而 DX12 是 Windows 上最稳的；③§8 的 38 MB 内存优势是照 DX12 定的。`WGPU_BACKEND=Vulkan,DX12` 掩码反而更慢（1430 ms，因为仍会枚举 DX12）。**结论：保持 DX12，把首帧判据的复核放到「干净 Win10 VM」那一行一起做**——那才是这个指标该落的地方。当前这台机器的 1.27 s 记为已知机器特异性开销。

**体积**：阶段 0 探针 15.22 MB → 阶段 2 后端接入 16.27 MB → 阶段 3 六页 18.78 MB（`lto="thin"`）→ `lto="fat"` 17.35 MB → **阶段 4 实测 `lto=false` 反而最小（17.10 MB）且构建快 10 倍**（单编译单元下跨单元内联本就没多少可做，fat 的激进内联还撑大代码），故定 `lto=false` → **再叠 `+crt-static` 静态链 CRT 到 17.31 MB**（见下「零依赖」）。其中 4.34 MB 是内嵌字体。原目标「≤ 15 MB」是照探针定的，接了后端就不现实，**放宽到 ≤ 18 MB**——判据是「零运行时依赖的单文件分发」，不是跟 Tauri 的 3 MB 比（那 3 MB 后面挂着 100+ MB 的 WebView2 runtime）。去掉 `svg` feature 还能再省约 1.8 MB，但图标要改 `canvas` 手绘，**暂不砍**。

**零依赖（阶段 4 补，纠正一个想当然）**：一直以为「Rust 静态链接 = 天然零依赖」，但 `dumpbin /DEPENDENTS` 打出来才发现默认 MSVC 构建**动态依赖 `VCRUNTIME140.dll`**（VC++ 运行库）和一批 `api-ms-win-crt-*`（UCRT）——这些**不是** Windows 自带组件，一台没装过 VC++ 可再发行组件的干净 Win10 VM 会直接起不来，正好砸在「零运行时依赖」这个核心卖点上。解法：`.cargo/config.toml` 里加 `-C target-feature=+crt-static`，把 CRT 编进 exe。之后 `dumpbin /DEPENDENTS` 只剩 `kernel32 / user32 / gdi32 / ole32 / ws2_32 / dwmapi / bcrypt / imm32 / uxtheme / opengl32 / shell32 / ntdll / advapi32` 加一个 `api-ms-win-core-synch-l1-2-0`（Win8.1+ 自带的核心 api-set）——全是系统 DLL。代价 +0.21 MB（17.10 → 17.31），换来的是「拷到一个裸 Windows 上双击就能跑」名副其实。**教训：「零依赖」是要用 dumpbin 验的，不是靠「Rust 嘛肯定静态」的直觉。**

### 测试方案变更：`iced_test` 取代 UIA 脚本

**已实测确认**：iced 0.14 **没有 AccessKit / 任何可访问性集成**（0.14.0 的 `Cargo.toml` 里无 `accesskit` feature、无该依赖）。阶段 0 拿探针窗口跑了一遍 UIA：`AutomationElement.FromHandle` 能拿到窗口本身（`name='DshDesk Phase 0 探针' type=ControlType.Window`），但 **`FindAll(TreeScope::Descendants)` 返回 0 个后代**——窗口内部对 UIA 完全不可见。这意味着：

> **上一代的 `e2e.ps1` 会完全失效。** 那套脚本靠 UI Automation 按名字找按钮、Invoke 点击、读 Text 元素断言，在 iced 窗口上一个控件都找不到。

顺带一个连带影响：**外部脚本无法「点中」某个控件**，只能按屏幕坐标点。阶段 0 的 IME 测试因此改成「程序自己 `operation::focus` 聚焦输入框，脚本只负责激活窗口和敲键」——而且**绝不能往窗口里点鼠标**，因为 `text_input` 在任何落空的点击上都会 unfocus。

替代方案是官方的 **`iced_test`**（0.14，作者本人维护），它在**框架内部**模拟交互，比 UIA 更可靠：


```rust
// 按控件包含的文字选中并点击（&str 实现了 Selector trait）
let mut ui = simulator(app.view());
let _ = ui.click("新建版本");
ui.typewrite("e2e-test");
let _ = ui.click("创建");

// 把模拟产生的消息喂回 update，然后断言状态
for message in ui.into_messages() {
    app.update(message);
}
assert!(app.profiles.iter().any(|p| p.name == "e2e-test"));

// 断言渲染结果：重建 view 再 find
let ui = simulator(app.view());
assert!(ui.find("e2e-test").is_ok(), "列表里应出现新版本");
```

可用 API（已核对 `iced_test` 文档索引）：
- `simulator(view)` → `Simulator`
- `Simulator::click(selector)` / `typewrite(text)` / `tap_key(key)` / `find(selector)` / `into_messages()` / `snapshot()`
- `Selector` trait，`&str` 的实现是「按控件包含的文本选中」
- `screenshot(program, theme, viewport, scale, duration)` —— 截图，可做视觉回归
- `emulator` 模块 —— headless 运行整个 app
- `ice` 模块 —— 可共享的测试用例格式；配 `iced_tester`（带录制器，依赖 `rfd` 原生文件对话框）可交互式录制测试

好处：**比 UIA 脚本更快更稳**（无窗口焦点争夺、无 DPI 坐标换算、无「WebView2 未就绪导致空树」这类竞态——上一代为此折腾了好几轮）；坏处：它测的是「view 树 + update 逻辑」，**不覆盖真实 GPU 渲染与真实 IME**，那两项仍需手动验证（已在上表列为独立条目）。

**阶段 4 已实测确认**（`iced_selector` 0.14 随 `iced_test` 一起拉下来，读了源码）：`Selector` 对 `widget::Id` **有实现**（`impl Selector for widget::Id`，按 `candidate.id()` 精确匹配），所以按 id 选中是可行的——只是我们的控件大多没设 id。`&str` 的实现是 **`content == self` 精确相等**（不是子串），`Text` 和 `TextInput` 都参与匹配。歧义处理：本套用例靠「只在对应页面点 + 选唯一文案」规避（如多行的「删除」按钮，只在有单行数据的 seeded_app 上点）；真要按行定位，给控件 `.id(widget::Id::new(...))` 再用 Id 选择器即可。另外 `click` 要求目标 `visible_bounds` 非空（滚出视口的点不到），`find` 不要求可见。


### 已知风险与预案

| 风险 | 预案 |
|---|---|
| **无 AccessKit：屏幕阅读器不可用，且 UIA 自动化失效** | **已实测确认**（UIA 后代数 = 0）。自动化改用 `iced_test`（见上）。可访问性本身**本次接受退步**并在 README 声明——上一代 WebView2 自带完整 a11y 树，这是原生化的隐性代价。若将来必需，需等 iced 支持或自行接 AccessKit |
| ~~中文 IME 实测有问题~~ | ✅ **已消除**：微软拼音 preedit/候选框位置/提交/整字退格全部正常，见 [phase0/REPORT.md](phase0/REPORT.md) §3 |
| ~~空闲 CPU 降不下来~~ | ✅ **已消除**：0.0000%，120s 内 0 帧 |
| 内存降不到 40 MB | **已发生，且已定位**：GPU 后端 79.8 MB，其中约 75 MB 是显卡驱动 + wgpu 设备常驻，不可控。目标已改为分档（§1），判据变成「显著低于上一代 199 MB」，实测降幅 60%。若将来在别的机器上超过 120 MB，需重新评估 |
| iced 0.14 已 9 个月无新版（0.13→0.14 隔 15 个月），遇框架 bug 无人修 | 视严重程度：小问题自己 fork 打补丁；阻塞性问题考虑 §2 的 GPUI 备选。注意 master 已是 `0.15.0-dev`，可关注但**不用 git 依赖**（API 会破坏性变更） |
| 无可用 GPU 的机器（虚拟机、远程桌面、老显卡） | ✅ **已实测**：`ICED_BACKEND=tiny-skia` 下中文完全正常（不像 Slint 的软渲染器只支持西文），与 GPU 输出平均差 0.65/255；而且内存反而更低（15.3 MB） |
| SVG 图标无法动态着色 | 改 `canvas` 手绘（图标简单）或每主题存一份 |
| `blur`/亚克力窗口效果 | iced 的 `window::Settings::blur` **在 Windows 上是 no-op**（文档明确只支持 macOS/Linux）。设计里不依赖毛玻璃，无影响 |
| **iced 的 `Shadow` 没有 spread** | 阶段 0 新发现。上一代大量用负 spread 收缩阴影（`0 20px 40px -24px`），iced 侧只能靠调小 `blur_radius` 近似。实测观感等价（40 → 30），已写进 `phase0/src/theme.rs` 的注释 |


### 失败退出条件

**任一条成立就停止本方向，回到 Tauri 版**：
- 中文输入法不可用或体验明显差于 WebView2 —— ✅ 阶段 0 已排除
- 空闲 CPU 无法降到 1% 以下 —— ✅ 阶段 0 已排除
- 内存相比上一代（199 MB 私有工作集）降幅 < 30% —— ✅ 实测降 60%
- 视觉明显退步且无法在合理工作量内补齐 —— 阶段 1 复核

上一代（Tauri 版）保持可用状态，**不因本重构而删除或停止维护**，直到 Dshnext 全部达标。

---

## 10. 实施顺序

分阶段，每阶段都有可验证产出，早失败早退出。

**阶段 0：可行性验证 —— ✅ 已完成，全部通过**

产出：`phase0/` 探针工程（`src/main.rs` 三个场景 + `theme.rs` 令牌）、四个测量脚本、字体构建脚本、[实测报告](phase0/REPORT.md)。

1. ✅ `cargo new` + iced 依赖（`default-features = false`，按 §2 清单），跑起窗口
2. ✅ 四件最可能翻车的事全部验过：
   - **中文渲染**：内嵌 GB2312 子集 + `advanced-shaping`，清晰、无豆腐块、生僻字走系统回退、等宽数字对齐
   - **中文输入法**：微软拼音候选框贴光标、preedit 就地显示、Commit 完整、退格按整字
   - **空闲占用**：CPU/GPU 双 0，120s 内 0 帧；内存 79.8 MB（DX12）需按 §1 调整目标
   - **软件回退**：`ICED_BACKEND=tiny-skia` 下中文正常，与 GPU 输出几乎一致
3. ✅ 软阴影圆角卡片三档模糊半径，观感到位（发现 `Shadow` 无 spread）
4. ✅ 顺手多验了三项：首帧耗时（145 ms，比 Tauri 版快 91 ms）、单 exe 体积（15.22 MB）、UIA 树确实为空

**阶段 0 遗留、进阶段 1 之前必须做的三件事**：

- 把探针里验证过的 `theme.rs`、字体加载、DX12 限定搬进产品代码（不是重写，是搬）
- 视觉对照要做真正的并排图：目前只对比了阴影与圆角，**过渡动画、按钮四态、渐变都还没画**
- `phase0/` 保留不删。它是唯一能快速复现性能数字的地方，阶段 1 之后每次改动都该重跑一遍 `measure-idle.ps1`

**阶段 1：视觉地基 —— ✅ 已完成**

产出：`src/theme.rs`（两套完整令牌）、`src/ui/{anim,card,button,icon}.rs`、`src/app.rs`（demo 页）、`src/main.rs`（DX12 限定 + 字体加载 + `--shot`/`--autotest`/`--drawlog`）。出图在 `shots/phase1-{dark,light}.png`，并排对比在 `shots/compare-{dark,light}.png`（左上一代、右本代）。

5. ✅ `theme.rs` 两套令牌：色值逐条抄自 `../src/styles.css`，用 `rgb!`/`rgba!` 宏从十六进制展开（const 上下文可用，免手算浮点）。`Palette` 保持 `Copy` 按值传。
6. ✅ `ui/card.rs` + `ui/button.rs`：六类变体（primary/secondary/danger/teal/quiet-danger/ghost）× 三档尺寸 × 禁用态，主按钮与 teal 按钮是 180° 竖向渐变。**`Background::Gradient` 已验证**：`Linear::new(Radians)` 的角度与 CSS `linear-gradient` 同向（`Radians::to_distance` 内部 angle−90°、y 轴向下，所以 π 即自上而下）。**svg 动态换色已验证**：`svg::Style{color}` 是渲染后像素级 RGB 替换（保留 alpha），单色描边图标等价于 `currentColor`。
7. ✅ `ui/anim.rs`：`Tween`（ease-out cubic 近似 CSS `ease`）+ `AnimState`（key→值/补间两张表）。hover 过渡用 `mouse_area` 的 enter/exit 驱动，**不读 `button::Status::Hovered`**（那是阶跃的）。订阅生命周期用两种方式验证：
   - `--autotest`：程序自己触发一次 hover 进/出，出帧日志显示动画期 75 帧、结束后连续三个 5s 窗口 delta=0。
   - 真实鼠标（`SetCursorPos` 进按钮→移到空隙）：enter 一次补间、exit 一次补间，稳定后 delta=0；60s 空闲 CPU 0.0000%、GPU 0.00%（release 构建）。

阶段 1 踩到的新坑（已写进 AGENTS.md）：`Text<'a>` 对 `'a` **不变**，文本封装函数不能钉死 `'static`；`mouse_area` 要求 `Message: Clone + 'static`；数帧的 1x1 widget 放 scrollable 里会被视口剔除（draws 恒 0），必须放常驻可见区。


**阶段 2：后端接入 —— ✅ 已完成**

产出：`src/core/`（7 个模块，含新增 `event.rs`）、`src/bridge.rs`（channel 与 ProcMap 的全局持有）、`app.rs` 的后端 Message 分支与真实数据卡片。

8. ✅ `core/event.rs`：`CoreEvent`（EnvProgress/Log/Url/Exit）+ `EventSink`（`UnboundedSender`）。三个耦合模块按 §3 的替换表机械改写，**业务逻辑一行未动**，改完全库 `grep tauri` 为零。unbounded 是刻意的：日志洪峰时反压会把 dsh 卡在写管道上。
9. ✅ `Subscription::run` 接 channel，事件进 `update()`。**两个坑**：
   - `Subscription::run` 的 builder 是**裸函数指针 `fn()`**，捕获不了 channel。做法是 `main` 里 `bridge::init()` 建好 channel，发送端进 `OnceLock`、接收端进 `Mutex<Option<_>>` 等订阅第一次取走。让 `bridge::events()` 返回 `Subscription<CoreEvent>`、调用方自己 `.map()`，闭包就无需捕获。
   - `ProcMap` 要跨 Task 共享而 `Task::perform` 的 future 必须 `'static`，同样用全局 `OnceLock<Arc<ProcMap>>` 解决。
10. ✅ 端到端打通并实测：配置读写（切主题落盘 `config.json`，与上一代同一文件）、环境探测（`dsh 0.1.1-rc.2 / node v24.19.0 / pnpm 10.23.0` 真实读出）、profile 列表（扫 `$DSH_HOME/profiles/`）、进程启停。

**`--e2e` 全链路实测**（程序自己跑，不依赖鼠标坐标）：启动 `web` → 2s 后收到 `CoreEvent::Url{url:"http://127.0.0.1:3080"}` 与 stdout 行 → 8s 后 taskkill → 收到 `Exit{code:1}` 与系统日志「已发送停止指令 (PID 10340)」。四个事件全部按类型到达 `update()`，日志卡按 stream 着色显示。

**进程轮询按需订阅**（§8 第 1 条的兑现）：`procs` 非空才挂 `time::every(2s)`，上一代是无条件 `setInterval(2000)`。空闲实测仍 **delta=0**（连续三个 5s 窗口零帧）、私有工作集 **80.6 MB**、单 exe **16.27 MB**（后端依赖 reqwest/zip/tokio 让体积从 15.9 涨了 0.36 MB）。

**阶段 3：页面移植 —— ✅ 已完成**

产出：`src/pages/`（六页 + 共享外壳）、`src/ui/{widgets,modal}.rs`、`src/update.rs`（从 app.rs 拆出，纯状态迁移）。出图在 `shots/p3-*.png`。

11. ✅ 环境页：三项检测（Node/dsh/pnpm）+ 版本下拉 + 安装/卸载，进度事件流实时进控制台
12. ✅ 版本管理页：新建/重命名/复制/删除，四种模态（§7.2 落地）
13. ✅ 首页：hero + meta 信息带（固定网格）+ 运行实例列表，启动/停止/打开界面
14. ✅ 控制台页：来源过滤 + `auto_scroll` 跟随尾部 + 复制/清空
15. ✅ 插件管理页：已装/市场两个分页 + 搜索 + 装卸
16. ✅ 设置页：五张卡（外观/模型访问/启动行为/下载源/关于），dirty 才允许保存；`open_mode` 项已移除（§5）但字段保留——两代共用 `config.json`，删了会让上一代读不到自己的设置

**模态实现（§7.2 兑现）**：`stack![base, opaque(mask)]`。三条踩到的：
- `opaque` **必须**包在遮罩上，否则点遮罩会穿透点到下层按钮
- **ESC 关闭只能走全局键盘订阅**：模态是覆盖层，拿不到键盘焦点
- 遮罩淡入复用 §7.1 的补间（key = `"modal"`），关闭时补间回 0，动画结束订阅自动撤

**大文本性能（§7.3 的实际取法）**：没用 `text_editor`，改成**只构造尾部 200 行** Element，更早的行折叠成一句「…前 N 行已折叠（用「复制」导出完整日志）」。日志的使用场景就是看最新的；翻历史交给复制导出。比 `text_editor` 简单，也避开了它的编辑态包袱。

**两个真 bug，都是实测发现的**：
1. 反复进环境页会重复发版本请求——`dsh_versions.is_empty()` 在请求飞在半路时仍为真。加 `versions_loading` 标志。
2. 列表选中行底色炸开：`accent_soft`（5% 蓝）叠在 `#18181a` 上，实测得到 `(31,36,69)`——蓝通道从 26 冲到 69，整行盖过行内按钮。原因是**关掉 `web-colors` 后 iced 按物理（线性空间）混色**，深底上叠带色相的半透明会被放大得离谱。改成不透明的 `row_selected` 令牌。**纪律：半透明叠色只用于中性灰（hover），带色相的一律写死不透明值。**

**验收**：两轮 judge 视觉验收，六页最终全 pass（首轮 3 页 fail：游离的「→」按钮、控制台空状态顶部对齐、设置页混进开发者自述文案，均已修）。交互实测（`phase0/tools/interact-probe.ps1`）：模态开关、ESC、Ctrl+1..6、侧边栏点击全部有对应 Message 到达 `update()`。空闲仍 **delta=0**、私有工作集 **72.8 MB**、CPU **0%**。

**阶段 4：收尾 —— ✅ 完成**

17. ✅ 图标全套：补 `node/harness/download/market` 四个专属图形，环境页三行（Node/dsh/pnpm）与插件市场两来源（catalog/npm）各用对图标，不再借用侧边栏图标。toast、空状态阶段 3 已在位。
18. ✅ `iced_test` 全流程用例：`src/tests.rs` 14 例全绿（导航/模态全流程/主题/插件分页/控制台/设置 dirty/启停消息）。**测的是 view↔update 契约**——`simulator` 不跑 `update` 返回的 `Task`，真实子进程 I/O 仍由 `--e2e` 覆盖。踩到：`&str` 选择器是**精确匹配**（`content == self`），空草稿点「创建」会冒泡到遮罩产生 `CloseDialog`（故断言「无 DialogConfirm」而非「无消息」）；喂 `Start/Stop` 前必须 `bridge::init()`。
19. ✅ 性能实测对照 §9：空闲 CPU 0.0015%、GPU 0.00%、出帧 delta=0、私有工作集 88.5 MB（稳定不爬）、单 exe 17.31 MB、零运行时依赖（`dumpbin` 确认仅系统 DLL）全达标；**首帧发现阶段 0 的 145 ms 是控制台子系统探针的假象**，产品 GUI+DX12 本机 ~1.27 s（驱动枚举 6 个重复适配器的机器特异性开销，见 §9「首帧真相」）。本机可测项全部签字，唯一挂起的是「干净 Win10 VM 首帧复核」——非本机可闭环，列入 §13 待办。
20. ✅ 打包：单 exe（`target/release/dshnext.exe` 17.31 MB，`+crt-static` 零依赖）+ 可选 NSIS（`packaging/installer.nsi`，每用户安装，注册表标识 `Dshnext` 与上一代 `DshDesk` 分开）；README 按阶段 4 实测数字重写；六页与 Tauri 版并排对比图 `shots/compare-p4/`（左 Tauri 右 Dshnext）。

**阶段 5：切页动画与排版收口 —— ✅ 完成**

21. ✅ 切页入场动画（§7.7）：`ui/reveal.rs` 一个透明包装 widget，平移 + 背景色面纱，两者共用 `anim::PAGE`；侧边栏选中态同帧交叉淡入（`anim::NAV` + `app.prev_page`）。**空闲零帧不变**（`--drawlog`：切页那 5s 出 31 帧，之后 `delta=0`），`cargo test` 仍 14/14。三个新坑记进 AGENTS.md（面纱必须自己 `with_layer`、位移用 `with_translation` 不动布局、`animate_to` 起不了重播所以加了 `restart`）。
22. ✅ 排版与层级收口（§6「排版与层级收口」）：字号十档收成八档令牌、纵向节奏分 `GAP_CARD`/`GAP_SECTION` 两档（靠 `widgets::page_stack` 嵌层实现）、卡片高度分 `card()`/`card_hero()` 两档（圆角 16/20 + 两级阴影）、按钮新增 `Accent` 变体把 `Primary` 收成每屏至多一个。
23. ✅ 视觉验收：12 张图（六页 × 明暗）交 judge，暗色 5 pass + 1 fail、亮色 6 pass；那条 fail 是设置页复选框说明行的缩进——量下来确实比内容列多 24 逻辑像素，改法是删掉「对齐复选框标签」的缩进回到内容列（它是整张卡里唯一不在内容列上的一行）。修完重出，两处左边缘同为 x=375。对比图 `shots/compare-p5/`，动画取证 `shots/anim/`。
24. ✅ 安装包跟上：`makensis installer.nsi` 重编，包内 exe 与 `target/release/dshnext.exe` 的 SHA-256 相同（18,160,128 字节 → 安装包 6,814,064 字节，lzma 37.3%）。装卸一圈实测：`/S /D=<目录>` 静默安装后文件哈希一致、注册表卸载项齐全、装出来的 exe 能自截图；卸载后安装目录与注册表都清干净了，但**开始菜单文件夹会留下**——`RMDir` 不删非空目录，卸载段只写了 `RMDir` 没删那两个 `.lnk`。已补 `Delete` 两条快捷方式，重编后再验证一遍才全清。`EstimatedSize` 同步到 17734 KB。
25. ✅ 删掉切页面纱（用户反馈）：面纱盖整块区域、底色比主区实际背景暗，切页时背景肉眼可见变暗（实测过渡中主区平均亮度 9.8 vs 落定 25.5），像闪屏。`reveal` 只留位移。复测：过渡中间帧与落定帧主区空白背景恒定 6.0，两帧像素差只落在内容区（位移中的标题+卡片），`cargo test` 仍 14/14、空闲零不变。§7.7 已改写。

---

## 11. 与上一代的关系

- **已替换**。上一代曾计划并行维护，2026-09-05 决定删除，代码只存在于 git 历史；Dshnext 是唯一主线。
- **数据完全兼容**：共用 `%LOCALAPPDATA%\DshDesk\`（config.json、runtime、home/profiles），老用户装上新版直接接着用。
  - 注意：`config.json` 里的 `open_mode` 字段在 Dshnext 中被忽略（§5），反序列化需容忍未知/无用字段（`serde(default)` 已在用）。
- **术语与交互保持一致**：页面名、按钮文案、快捷键（Ctrl+1..6）都不变，降低学习成本。

---

## 12. 已知取舍清单

原生化不是纯赚，这四项是明确的退步或未解项，写在这里避免以后被当成 bug：

| 项 | 状态 | 说明 |
|---|---|---|
| **内置 WebUI 窗口** | ❌ 移除 | 改为系统浏览器打开（§5）。设置里的「Web 界面打开方式」选项一并移除 |
| **可访问性（屏幕阅读器）** | ❌ 退步 | iced 0.14 无 AccessKit 集成，**已实测**：UIA 树里窗口后代数 = 0。上一代靠 WebView2 白送完整 a11y 树，原生版暂无。需在 README 声明 |
| **UIA 自动化脚本** | ⚠️ 换方案 | 第一代 `scripts/e2e.ps1` 失效（已随第一代删除），改用 `iced_test`（§9）。截图脚本 `shot.ps1`/`capture-pages.ps1` 仍可用（PrintWindow 与框架无关）；另外 iced 自带 `window::screenshot()`，能在进程内取图，比 PrintWindow 更干净——阶段 0 的渲染截图就是这么来的 |
| **常驻内存 40 MB 的原目标** | ⚠️ 放宽 | GPU 后端做不到，79.8 MB 里约 75 MB 是显卡驱动常驻。目标改为分档（§1），判据变成「显著低于上一代」 |
| **`Shadow` 无 spread** | ⚠️ 近似 | 上一代 CSS 用负 spread 收缩阴影，iced 只能调小 `blur_radius` 近似，观感等价 |
| **OpenType feature（`tnum` 等）** | ❌ 不可用 | iced 从不设置 cosmic-text 的 `font_features`。等宽数字只能靠字体天然等宽（§6） |
| **`letter-spacing`（负字距）** | ❌ 不可用 | iced 0.14 的 `Text` 没有 `letter_spacing` API。CSS 大标题的 −1.2px 负字距抄不了，hero 数字只能用 48px/600（§6 暗色精修） |
| **毛玻璃/亚克力窗口** | — | iced 的 `blur` 在 Windows 上是 no-op；本设计不依赖该效果，无影响 |

反过来，明确的收益（括号内是阶段 0 实测值）：

| 项 | 收益 |
|---|---|
| 分发 | 单 exe（15.22 MB），不再要求目标机器有 WebView2 runtime |
| 空闲 CPU/GPU | reactive rendering，不重绘就不出帧（**0.0000% / 0.00%**，对上一代 0.26% / 1.72%） |
| 常驻内存 | **79.8 MB**（DX12）对上一代 **199 MB**，降 60%；无 GPU 时 15.3 MB |
| 进程数 | **1 个**，对上一代 7 个 |
| 软阴影成本 | SDF shader 解析式求值，比 WPF 的每帧 GPU 模糊便宜得多，视觉不用为性能让步 |
| 类型安全 | 后端事件从 JSON 变成 Rust 枚举，字段错误编译期就报 |
| 首帧 | **145 ms** 对上一代 **236 ms**（无 WebView2 runtime 初始化，也不用等 bundle 加载） |
| 测试 | `iced_test` 在框架内模拟，比 UIA 快且无焦点/DPI/竞态问题 |


---

## 附：本文档中未验证的事项

写明以免被当成已确认的事实。阶段 0 已经消掉四条，剩下四条按阶段推进。

**阶段 0 已验证（见 [phase0/REPORT.md](phase0/REPORT.md)）**：

- ~~iced 软件渲染器（tiny-skia）对中文的支持程度~~ → ✅ 完全正常，与 GPU 输出平均差 0.65/255
- ~~iced 0.14 中文 IME 的实际体验~~ → ✅ 微软拼音全链路正常
- ~~Tauri/WebView2 的内存基线具体数字（原写 80–150 MB 是估计）~~ → ✅ 实测 **199 MB 私有工作集 / 391 MB 工作集 / 7 个进程**
- ~~`scrollable` 的 culling~~ → 部分回答：`Column::draw` 用 `bounds().intersects(viewport)` 剔除子元素，剔除逻辑确实存在；但**上百条插件行的实际帧时间仍需阶段 3 实测**

**阶段 1 已验证**：

- ~~`Background::Gradient` 的 API 细节（主按钮竖向渐变）~~ → ✅ 角度与 CSS 同向，π=自上而下；demo 页主按钮/teal 按钮/品牌方块三处渐变正常
- ~~`svg` widget 能否动态换色~~ → ✅ `svg::Style{color}` 像素级 RGB 替换保留 alpha，单色描边图标等价 currentColor

**仍未验证**：

- ~~`iced_test` 是否支持按 id 选控件~~ —— ✅ 阶段 4 已确认：`iced_selector` 对 `widget::Id` 有 `Selector` 实现（见 §9 选择器说明）
- `window::Settings::platform_specific` 里 Windows 相关字段（drag-drop、skip_taskbar 等） —— docs.rs 是 Linux 构建，未列出
- 干净 Win10 VM（无 WebView2/.NET/VC++ 运行库）能否直接跑单 exe —— **构建层面已用 `dumpbin` 验死**（`+crt-static` 后只剩系统 DLL，无 VCRUNTIME140/UCRT/WebView2/.NET）；剩「在真·裸机上双击实跑」这一经验验证需要一台 VM，本机无法模拟。连同「干净机器首帧复核」一起列为交付前必做项。
- 换一台机器（AMD/Intel 集显、无独显）的内存数字 —— 目前只有一台 RTX 4060 的数据，75 MB 驱动开销可能因厂商而异
- 真实页面（六页都开过）的稳态内存 —— 阶段 3 复核；demo 页 98.6 MB 是刻意加密的最坏情况，不是真实页面数字








