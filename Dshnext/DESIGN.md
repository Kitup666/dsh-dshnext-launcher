# Dshnext — 原生 Rust 重构设计文档

DshDesk（Tauri + React）的原生重写。**后端逻辑整体复用，前端换成纯 Rust GPU 渲染**，目标是把「低占用」和「高视觉」同时拿到。

- 状态：设计阶段，尚未开始编码
- 上一代：`../src-tauri`（Rust 后端）+ `../src`（React 前端），已可用并出过 NSIS 安装包
- 本目录：`src/core/` 是从上一代直接复制的后端模块（1163 行），`docs/*.tauri-reference` 是对照用的旧文件

---

## 1. 目标与非目标

### 硬目标

| 维度 | 上一代（Tauri/WebView2） | Dshnext 目标 |
|---|---|---|
| 空闲 CPU | WebView2 有常驻基线 | **≈ 0%**（不重绘就不出帧） |
| 空闲 GPU | 合成器常驻 | **≈ 0%**（无动画时不提交 frame） |
| 常驻内存 | 80–150 MB | **≤ 40 MB** |
| 冷启动 | 需初始化 WebView2 runtime | **≤ 300 ms 到首帧** |
| 分发体积 | 3 MB + 依赖系统 WebView2 | **单 exe ≤ 15 MB，零运行时依赖** |
| 视觉质量 | 软阴影/圆角/过渡齐全 | **不退步**（软阴影、圆角、渐变、过渡动画全保留） |
| 中文质量 | WebView2 灰度 AA | **不退步或更好** |

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

```toml
iced         = { version = "0.14", features = ["wgpu", "advanced", "image", "svg", "tokio"] }
tokio        = { version = "1", features = ["process", "io-util", "time", "sync", "macros"] }
reqwest      = { version = "0.12", default-features = false, features = ["rustls-tls", "json", "stream"] }
serde        = { version = "1", features = ["derive"] }
serde_json   = "1"
zip          = { version = "2", default-features = false, features = ["deflate"] }
dirs         = "5"
futures-util = "0.3"
open         = "5"      # 替代 tauri-plugin-opener
```

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

cosmic-text 0.19 是 Rust 的 CJK 事实标准：shaping 用 HarfRust，fallback 表直接抄 Chromium/Firefox 并按 locale 区分 Han（避免中日韩字形串味）。但有一条**必须显式处理**：

> iced 的 `text::Shaping` 历史默认是 `Basic`（不做复杂 shaping 与 fallback）。中文**必须**用 `Advanced` 或 0.14 新增的 `Auto`，忘了就是满屏豆腐块或字形错乱。

对策：
- 在 `ui/mod.rs` 里封装 `fn text(s) -> Text` 统一设 `.shaping(Shaping::Advanced)`，**全项目禁止直接用 `iced::widget::text`**（可用 clippy 规则或 review 约束）。
- **内嵌字体**而不是依赖系统字体：`assets/fonts/` 放思源黑体 / Noto Sans SC 的**子集化**版本（常用 3500 字 + 拉丁 + 符号，约 1–2 MB），`iced::Settings { fonts: vec![include_bytes!(..)] }` 加载。这样在任何 Windows 上观感一致，也避免「用户系统没装雅黑」。
- 等宽（路径、版本号、日志、PID）用 Cascadia Mono 子集或 JetBrains Mono。

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
2. **不开 `unconditional-rendering` feature**（iced 用它退回旧的每事件出帧行为）。
3. **日志环形缓冲**用 `VecDeque` 上限 2000，超出从头弹出，避免无限增长。
4. **`view()` 里不做重计算**。过滤/排序/格式化的结果缓存进 State，`view()` 只读不算。
5. **图片/SVG 缓存**。iced 有内部缓存，但确认图标不是每帧重新解析。
6. **release 构建**：`lto = true`、`codegen-units = 1`、`strip = true`、`panic = "abort"`（上一代已用前三条）。
7. **窗口最小化时不出帧**（winit 默认行为，需确认 iced 未覆盖）。

---

## 9. 验收判据与失败退出

### 必须达标（否则本方向失败）

| 指标 | 阈值 | 测法 |
|---|---|---|
| 空闲 CPU | < 0.5% | 任务管理器观察 60s，窗口打开但不操作 |
| 空闲 GPU | ~0% | 任务管理器 GPU 引擎列 |
| 常驻内存 | ≤ 40 MB | 私有工作集，打开六个页面后 |
| 冷启动到首帧 | ≤ 300 ms | 手动计时或加埋点 |
| 单 exe 体积 | ≤ 15 MB | 含内嵌字体 |
| 零运行时依赖 | 干净 Win10 VM 能跑 | 不装 WebView2/.NET 的机器 |
| 中文渲染 | 无豆腐块、无字形错乱、小字清晰 | 六页目检 |
| 中文输入法 | 三处输入框能打中文、候选框位置正确 | 手动测 |
| 视觉不退步 | 与 Tauri 版并排对比，软阴影/圆角/留白/过渡在位 | 截图对比 |
| 功能对等 | e2e 全流程通过（新建→启动→日志→停止→删除、插件装卸） | 移植 `../scripts/e2e.ps1`（UIA 需换成 iced 的可访问性支持，见下） |

### 已知风险与预案

| 风险 | 预案 |
|---|---|
| iced 的可访问性（AccessKit）支持不足，`e2e.ps1` 的 UIA 驱动失效 | 改为「进程/文件系统断言 + 截图目检」；或在 app 内加一个隐藏的测试用命令通道 |
| 中文 IME 实测有问题 | 若 iced 0.14 的 IME 不达标，**回退到 Tauri 版**（IME 是硬需求，不可妥协） |
| 空闲 CPU 降不下来 | 逐个排查订阅；若是框架层面无解，本方向失败 |
| 内存降不到 40 MB | 可放宽到 60 MB（仍显著优于 WebView2）；若 > 80 MB 则收益不成立 |
| iced 0.14 已 9 个月无新版，遇到框架 bug 无人修 | 视严重程度：小问题自己 fork 打补丁；阻塞性问题考虑 §2 的 GPUI 备选（gpui-component 有 60+ 现成组件，但依赖第三方快照 crate） |
| SVG 图标无法动态着色 | 改 `canvas` 手绘（图标简单）或每主题存一份 |

### 失败退出条件

**任一条成立就停止本方向，回到 Tauri 版**：
- 中文输入法不可用或体验明显差于 WebView2
- 空闲 CPU 无法降到 1% 以下
- 内存优化幅度 < 30%（即 > 100 MB）
- 视觉明显退步且无法在合理工作量内补齐

上一代（Tauri 版）保持可用状态，**不因本重构而删除或停止维护**，直到 Dshnext 全部达标。

---

## 10. 实施顺序

分阶段，每阶段都有可验证产出，早失败早退出。

**阶段 0：可行性验证（最关键，先做）**
1. `cargo new`，加 iced 依赖，跑起一个空窗口
2. **立刻验三件最可能翻车的事**：
   - 内嵌中文字体 + `Shaping::Advanced`，渲染一段中文——看是否清晰无豆腐块
   - 一个 `text_input`，用微软拼音打中文——看 IME 候选框位置与 preedit
   - 空窗口挂 60s，看空闲 CPU/GPU/内存
3. **若这三项任一不达标，本方向就地终止**，不进入阶段 1

**阶段 1：视觉地基**
4. `theme.rs` 两套令牌（照抄 CSS 色值）
5. `ui/card.rs` + `ui/button.rs`：软阴影卡片 + 四类按钮，做一个 demo 页并排对比 Tauri 版截图
6. `ui/anim.rs`：hover 过渡 + 订阅生命周期，验证动画结束后空闲 CPU 回到 0

**阶段 2：后端接入**
7. `core/event.rs`，六个模块的 Emitter → channel 机械替换
8. `Subscription::run` 接 channel，事件进 `update()`
9. 环境探测与配置读写打通（最简单的两条链路，验证端到端）

**阶段 3：页面移植**（按依赖顺序）
10. 环境页（只读展示 + 安装动作，验证进度事件流）
11. 版本管理页（CRUD + 模态，验证 7.2）
12. 首页（hero + meta 带 + 启动/停止，验证进程管理与日志事件）
13. 控制台页（验证 7.3 大文本性能）
14. 插件管理页（市场检索 + 装卸）
15. 设置页（表单 + 主题切换）

**阶段 4：收尾**
16. 图标全套、toast、空状态
17. 性能实测对照 §9 表格，逐项签字
18. e2e 移植或替代方案
19. 打包（单 exe + 可选 NSIS）、README、与 Tauri 版并排截图对比

---

## 11. 与上一代的关系

- **不是替换，是并行**。`../src-tauri` + `../src` 继续可用，`Dshnext/` 达标后才考虑谁是主线。
- **数据完全兼容**：共用 `%LOCALAPPDATA%\DshDesk\`（config.json、runtime、home/profiles），两个版本可互换使用，用户无感。
  - 注意：`config.json` 里的 `open_mode` 字段在 Dshnext 中被忽略（§5），反序列化需容忍未知/无用字段（`serde(default)` 已在用）。
- **术语与交互保持一致**：页面名、按钮文案、快捷键（Ctrl+1..6）都不变，降低学习成本。






