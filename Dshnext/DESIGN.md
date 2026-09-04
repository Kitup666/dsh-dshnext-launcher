# Dshnext — 原生 Rust 重构设计文档

DshDesk（Tauri + React）的原生重写。**后端逻辑整体复用，前端换成纯 Rust GPU 渲染**，目标是把「低占用」和「高视觉」同时拿到。

- 状态：**阶段 0 可行性验证已完成并全部通过**（[phase0/REPORT.md](phase0/REPORT.md)），可以进入阶段 1
- 上一代：`../src-tauri`（Rust 后端）+ `../src`（React 前端），已可用并出过 NSIS 安装包
- 本目录：`src/core/` 是从上一代直接复制的后端模块（1163 行），`phase0/` 是探针工程与实测报告，`docs/*.tauri-reference` 是对照用的旧文件

---

## 1. 目标与非目标

### 硬目标

下表的「实测」列是阶段 0 在同一台机器上量出来的真实数字，不是估计。

| 维度 | 上一代（Tauri/WebView2，实测） | Dshnext 目标 | 阶段 0 实测 |
|---|---|---|---|
| 空闲 CPU | 0.26%（全核）/ 3.6%（单核） | **≈ 0%**（不重绘就不出帧） | ✅ **0.0000%**，120s 内 0 帧 |
| 空闲 GPU | 1.72% | **≈ 0%** | ✅ **0.00%** |
| 常驻内存（私有工作集） | **199 MB**（7 个进程） | GPU 后端 **≤ 90 MB**<br>软件后端 **≤ 30 MB** | ✅ **79.8 MB**（DX12）<br>**15.3 MB**（tiny-skia） |
| 冷启动到**首帧** | 236 ms | **≤ 300 ms** | ✅ **145 ms** |
| 分发体积 | 3 MB + 依赖系统 WebView2 | **单 exe ≤ 15 MB，零运行时依赖** | ⚠️ **15.22 MB**（含 4.34 MB 字体） |
| 视觉质量 | 软阴影/圆角/过渡齐全 | **不退步**（软阴影、圆角、渐变、过渡动画全保留） | ✅ 软阴影/圆角观感一致 |
| 中文质量 | WebView2 灰度 AA | **不退步或更好** | ✅ 无豆腐块，等宽数字对齐 |

内存目标原写「≤ 40 MB」，阶段 0 证明这在有独显驱动的机器上不可能：79.8 MB 里约 75 MB 是 NVIDIA 用户态驱动 + wgpu 设备/队列的常驻开销，iced 自身加内嵌字体只占 5 MB 左右。**因此改为按后端分档**，判据仍是「必须显著低于上一代」——实测 199 → 79.8 MB，降幅 60%。

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

### 需要重写的部分

`commands.rs`（Tauri 的 22 个 `#[tauri::command]`）**整体废弃**，改为 iced 的 `Task::perform(async_fn, Message::Done)`。旧文件留在 `docs/commands.rs.tauri-reference` 供对照，确保功能不漏。

进程树清理（`taskkill /PID /T /F`）与 `CREATE_NO_WINDOW`（`0x0800_0000`）标志原样保留——这两个是 Windows 上的必需品，跟 UI 框架无关。

---

## 4. 架构与目录

iced 是 Elm 架构：`State → view() → Element → 用户操作 → Message → update() → 新 State`。天然契合「状态变化才重绘」。

```
Dshnext/
├── DESIGN.md
├── Cargo.toml
├── build.rs                    # 嵌入图标与清单（DPI-aware、无控制台）
├── assets/
│   ├── icons/                  # 已从上一代复制
│   └── fonts/                  # 内嵌字体，见 §6
├── docs/
│   ├── commands.rs.tauri-reference
│   └── Cargo.toml.tauri-reference
└── src/
    ├── main.rs                 # iced::application 入口、窗口设置
    ├── app.rs                  # 顶层 State / Message / update / view / subscription
    ├── theme.rs                # 设计令牌（两套主题），见 §6
    ├── core/                   # ← 复用的后端，仅解耦不改逻辑
    │   ├── mod.rs
    │   ├── event.rs            # 新增：CoreEvent / EventSink
    │   ├── store.rs
    │   ├── profiles.rs
    │   ├── envres.rs
    │   ├── installs.rs
    │   ├── procman.rs
    │   └── plugins.rs
    ├── pages/                  # 六个页面，与上一代一一对应
    │   ├── home.rs
    │   ├── profiles.rs
    │   ├── plugins.rs
    │   ├── env.rs
    │   ├── console.rs
    │   └── settings.rs
    └── ui/                     # 自造的通用组件，见 §7
        ├── card.rs
        ├── button.rs
        ├── sidebar.rs
        ├── list_row.rs
        ├── modal.rs            # 自造（iced 无 modal）
        ├── toast.rs
        ├── tag.rs
        ├── segmented.rs
        ├── icon.rs             # 描边图标，见 §6
        └── anim.rs             # 过渡动画驱动，见 §7
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
| 主按钮渐变 | `linear-gradient(180deg, hi, base)` | `Background::Gradient(Linear)` | ✅ 内置（渐变 API 细节待验证） |
| hover 过渡 | `transition: background .14s` | 自造，见 §7 | ⚠️ 要写 |
| 主题切换 | `data-theme` 换变量组 | 换 `Palette` 常量，即时生效 | ✅ 免费（颜色插值动画要自己写） |
| 中文字体 | 系统 `Microsoft YaHei UI` | 内嵌字体 + cosmic-text | ⚠️ 见下 |
| 描边图标 | 内联 SVG | `iced::widget::svg` 或 `canvas` 画路径 | ✅ 内置 |
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


---

## 7. 要自己造的东西

网页栈白送、iced 没有的，共五件。这是本次重构的**真实工作量所在**。

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

### 7.2 模态对话框（`ui/modal.rs`）

iced 无 modal。用 `stack!` + `opaque`（文档明确说它用来拦截鼠标、防事件穿透）+ `center` 自拼，需要：
- 半透明遮罩（`bg_app` 55% alpha）
- 淡入动画（复用 7.1）
- **ESC 关闭**：`keyboard::on_key_press` 订阅
- 点遮罩关闭、点内容不关闭
- 焦点陷阱（Tab 不跑到背景控件）——iced 的焦点模型待验证，可能需要在模态开启时禁用背景控件

对应上一代的 `PromptModal`（新建/重命名/复制/手动装插件）和 `ConfirmModal`（删除/卸载）。

### 7.3 日志控制台的窗口化（`pages/console.rs`）

`scrollable` + `column` 只有 primitive culling（视口外不绘制），**布局仍全量计算**。2000 行日志每帧算 layout 会卡。

两个选项：
- **首选**：用 `text_editor`（内部按 buffer 管理，天然支持大文本），只读模式，配 `auto_scroll`。
- 备选：自己做窗口化——按滚动偏移只把可见区间（约 40 行）构造成 `Element`，上下用 `Space` 撑出总高度。

`auto_scroll`（0.14 新增）正是「日志跟随尾部」要的，不用自己算。

### 7.4 虚拟滚动（版本/插件列表）

插件市场可能有上百条。`table`（0.14 新增）或 `scrollable` 的 culling 够不够要实测；若不够，同 7.3 的窗口化手法。

### 7.5 描边图标（`ui/icon.rs`）

上一代是内联 SVG（10 个图标：launch/versions/plugins/env/console/settings/node/harness/download/market/play/sleep/log）。iced 有 `svg` widget，直接把 SVG 字符串 `include_str!` 进来即可，**但要确认 svg widget 是否支持 `currentColor` 式的动态着色**——若不支持，改用 `canvas` 手绘路径（图标简单，可行）或为每个主题各存一份 SVG。

### 汇总工作量

| 项 | 难度 | 说明 |
|---|---|---|
| 后端解耦（Emitter → channel） | 低 | 机械替换，1163 行逻辑不动 |
| 六个页面 view() | 中 | 控件齐全，主要是布局搬运 |
| 设计令牌 + 主题 | 低 | 色值照抄 CSS |
| 过渡动画 | **中高** | 要写 tween + 严格管理订阅生命周期 |
| 模态 | 中 | stack + opaque 自拼 |
| 日志控制台 | 中 | text_editor 或窗口化 |
| 字体子集化 + shaping 纪律 | 中 | 踩坑就是满屏豆腐块 |
| 图标 | 低 | SVG 直接用 |

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
| 常驻内存 | GPU ≤ 90 MB / 软件 ≤ 30 MB | **私有工作集**，六个页面都开过之后 | ✅ 79.8 / 15.3 MB |
| 首帧画好 | ≤ 300 ms | `phase0/tools/first-paint.ps1`（按窗口表面颜色数判定，不是窗口出现） | ✅ 145 ms |
| 单 exe 体积 | ≤ 15 MB | 含内嵌字体 | ⚠️ 15.22 MB，见下 |
| 零运行时依赖 | 干净 Win10 VM 能跑 | 不装 WebView2/.NET 的机器 | 未测（阶段 4） |
| 中文渲染 | 无豆腐块、无字形错乱、小字清晰 | 六页目检 | ✅ 探针页通过 |
| 中文输入法 | 三处输入框能打中文、候选框位置正确 | `phase0/tools/ime-drive.ps1` | ✅ preedit/候选/提交/退格全对 |
| 软件回退下中文 | 与 GPU 后端观感一致 | `ICED_BACKEND=tiny-skia` 截图对比 | ✅ 平均差 0.65/255 |
| 视觉不退步 | 与 Tauri 版并排对比，软阴影/圆角/留白/过渡在位 | 截图对比 | ✅ 阴影圆角一致；过渡待阶段 1 |
| 功能对等 | 全流程通过（新建→启动→日志→停止→删除、插件装卸） | `iced_test`，见下 | 未测（阶段 4） |

**关于「测法」的两个教训**（写清楚是因为按错的方法量会得出反向结论）：

- **进程树必须递归遍历。** WebView2 是 `dshdesk.exe` → `msedgewebview2.exe`（browser）→ GPU/renderer/utility 子进程共 7 个。只走一层父子关系只找到 2 个，内存少算 230 MB、CPU 少算 100 倍（0.0015% 而不是 0.2573%）——照这个数字对比，上一代反而"更省"。
- **别报 `PrivateMemorySize64`。** 那是提交的虚拟内存，NVIDIA 驱动会撑到 200+ MB 而页面并不驻留。任务管理器「内存」列显示的是 `\Process(*)\Working Set - Private`，本文所有内存数字都用后者。

**体积 15.22 MB 擦线**：其中 4.34 MB 是字体。去掉 `svg` feature 能降到 13.38 MB，但图标就得改 `canvas` 手绘。**暂不砍**，阶段 1 之后代码量还会涨，到时候再按实际情况决定。

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

按 id 选中控件的 API 未在索引页确认（可能在 `iced_selector` 里），**未验证**；若只能按文本选中，需注意同文本控件的歧义（如多行都有「删除」按钮）——上一代 UIA 脚本已踩过这个坑，靠「按行标签定位 + 纵向坐标匹配」解决，`iced_test` 侧需要等价手段，可能要给控件加唯一文案或用 `iced_selector`。


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

**阶段 1：视觉地基**
5. `theme.rs` 两套令牌（照抄 `../src/styles.css` 的色值；暗色那套已在 `phase0/src/theme.rs` 验证过）
6. `ui/card.rs` + `ui/button.rs`：软阴影卡片 + 四类按钮，做一个 demo 页并排对比 Tauri 版截图
7. `ui/anim.rs`：hover 过渡 + 订阅生命周期，**验证动画结束后空闲 CPU 回到 0**（这条不过关就等于白做）


**阶段 2：后端接入**
8. `core/event.rs`，六个模块的 Emitter → channel 机械替换
9. `Subscription::run` 接 channel，事件进 `update()`
10. 环境探测与配置读写打通（最简单的两条链路，验证端到端）

**阶段 3：页面移植**（按依赖顺序）
11. 环境页（只读展示 + 安装动作，验证进度事件流）
12. 版本管理页（CRUD + 模态，验证 §7.2）
13. 首页（hero + meta 带 + 启动/停止，验证进程管理与日志事件）
14. 控制台页（验证 §7.3 大文本性能）
15. 插件管理页（市场检索 + 装卸）
16. 设置页（表单 + 主题切换；注意 `open_mode` 项已移除，见 §5）

**阶段 4：收尾**
17. 图标全套、toast、空状态
18. `iced_test` 写全流程用例（§9）
19. 性能实测对照 §9 表格，逐项签字
20. 打包（单 exe + 可选 NSIS）、README、与 Tauri 版并排截图对比

---

## 11. 与上一代的关系

- **不是替换，是并行**。`../src-tauri` + `../src` 继续可用，`Dshnext/` 达标后才考虑谁是主线。
- **数据完全兼容**：共用 `%LOCALAPPDATA%\DshDesk\`（config.json、runtime、home/profiles），两个版本可互换使用，用户无感。
  - 注意：`config.json` 里的 `open_mode` 字段在 Dshnext 中被忽略（§5），反序列化需容忍未知/无用字段（`serde(default)` 已在用）。
- **术语与交互保持一致**：页面名、按钮文案、快捷键（Ctrl+1..6）都不变，降低学习成本。

---

## 12. 已知取舍清单

原生化不是纯赚，这四项是明确的退步或未解项，写在这里避免以后被当成 bug：

| 项 | 状态 | 说明 |
|---|---|---|
| **内置 WebUI 窗口** | ❌ 移除 | 改为系统浏览器打开（§5）。设置里的「Web 界面打开方式」选项一并移除 |
| **可访问性（屏幕阅读器）** | ❌ 退步 | iced 0.14 无 AccessKit 集成，**已实测**：UIA 树里窗口后代数 = 0。上一代靠 WebView2 白送完整 a11y 树，原生版暂无。需在 README 声明 |
| **UIA 自动化脚本** | ⚠️ 换方案 | `../scripts/e2e.ps1` 失效，改用 `iced_test`（§9）。截图脚本 `shot.ps1`/`capture-pages.ps1` 仍可用（PrintWindow 与框架无关）；另外 iced 自带 `window::screenshot()`，能在进程内取图，比 PrintWindow 更干净——阶段 0 的渲染截图就是这么来的 |
| **常驻内存 40 MB 的原目标** | ⚠️ 放宽 | GPU 后端做不到，79.8 MB 里约 75 MB 是显卡驱动常驻。目标改为分档（§1），判据变成「显著低于上一代」 |
| **`Shadow` 无 spread** | ⚠️ 近似 | 上一代 CSS 用负 spread 收缩阴影，iced 只能调小 `blur_radius` 近似，观感等价 |
| **OpenType feature（`tnum` 等）** | ❌ 不可用 | iced 从不设置 cosmic-text 的 `font_features`。等宽数字只能靠字体天然等宽（§6） |
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

**仍未验证**：

- `Background::Gradient` 的 API 细节（主按钮竖向渐变） —— 结构已读（`Gradient::Linear{angle, stops:[Option<ColorStop>;8]}`），但没实际画过 —— 阶段 1 验证
- `svg` widget 能否动态换色 —— 阶段 1 验证
- `iced_test` 是否支持按 id 选控件（`iced_selector` 未读） —— 阶段 4 前需确认
- `window::Settings::platform_specific` 里 Windows 相关字段（drag-drop、skip_taskbar 等） —— docs.rs 是 Linux 构建，未列出
- 干净 Win10 VM（无 WebView2/.NET）能否直接跑单 exe —— 阶段 4 验证
- 换一台机器（AMD/Intel 集显、无独显）的内存数字 —— 目前只有一台 RTX 4060 的数据，75 MB 驱动开销可能因厂商而异








