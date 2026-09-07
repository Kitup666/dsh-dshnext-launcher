# Dshnext 现状架构（Current-State Snapshot）

> 按「现状架构记录」方法论产出：**只记已实现的**，不做目标设计、不打分。
> 每条事实标 `OBSERVED`（源码直接可证）/ `DOCUMENTED`（有文档但未复核）/ `INFERRED`（由证据推断）/ `UNKNOWN`。
> **本文档是活的架构地图，架构改动后同步更新**；DESIGN.md 是设计决策与阶段日志，两者分工不同。

- **快照身份**：branch master；本文快照写于 2026-09-06（HEAD `2e90fe3` 前后），其后追加：目录迁移 + 进度条、runtime 守卫 + 单实例互斥、「停止并继续」、NSIS 重打（详见 git log）。行数/Message 数等精确数字以该日快照为准。
- **快照日期**：2026-09-06（更新 2026-09-07）
- **规模**：`src/` 共 8628 行 Rust；`core/` 1255 行（复用自上一代）；单 crate、单 exe。

---

## 1. 系统总览

| 项 | 事实 | 证据 |
|---|---|---|
| 语言/框架 | Rust 1.92 (edition 2024)，iced 0.14（实际 0.14.2）+ wgpu，Elm 架构（State → view → Message → update） | `Cargo.toml`；`main.rs:119-168` OBSERVED |
| 目标平台 | Windows 10/11 x64 only；无边框窗口（`decorations: false`）、DWM 圆角、DX12 后端 | `main.rs:130-157` OBSERVED |
| 分发 | 单 exe（+crt-static 零运行时依赖）+ NSIS 安装包（packaging/） | DESIGN.md §9 DOCUMENTED |
| 后端进程 | 由 `core/procman.rs` spawn 的 `dsh` 子进程，UI 本体 1 进程 | `src/core/procman.rs` OBSERVED |

**分层与依赖方向**（grep 验证 2026-09-06）：

```
main.rs ──► app.rs（State/Message） ◄── update.rs（update/subscription）
   │              │    ▲                      │
   │              ▼    │                      ├──► bridge.rs ──► core/（后端，零上层依赖）
   └──► pages/（view/外壳）─► ui/（自造组件，不依赖 core）──► iced/iced_wgpu
theme.rs（令牌，叶子）
```

- `src/core/` 对 `ui/pages/app/bridge` 的依赖 = **0**（grep 证实）；唯一 "tauri" 命中是 `core/mod.rs:1` 的文档注释。
- `src/ui/`、`src/theme.rs` 不依赖 core，是纯叶子层。
- OBSERVED，`grep -rn "crate::ui|crate::pages|crate::app" src/core/` 与 `grep -rln "crate::core" src/ui/ src/theme.rs` 均为空。

## 2. 组件清单

### 2.1 应用层

| 文件 | 行数 | 职责 | 关键事实 |
|---|---|---|---|
| `main.rs` | 209 | 入口、CLI 解析、窗口设置、DX12 限定、字体注入、PNG 写盘 | CLI：`--theme/--shot/--after/--page/--switch-to/--switch-at/--tall/--drawlog/--autotest/--e2e/--all-backends` OBSERVED |
| `app.rs` | 337 | `Dshnext` 状态 + `Message` 枚举（约 60 个变体）+ 通用 helper | 状态分组：窗口/导航浮层/后端数据/环境页/插件页/控制台页/设置页 OBSERVED |
| `update.rs` | 781 | 全部状态迁移（`update` + `subscription`），阶段 3 从 app.rs 拆出 | 每条 Message 打 debug 日志（Tick/ToastTick 除外）OBSERVED |
| `bridge.rs` | 66 | core 异步世界 ↔ iced 的桥：`OnceLock<EventSink>` + `Mutex<Option<EventStream>>` + `Arc<ProcMap>` 全局持有 | 存在原因：`Subscription::run` builder 是裸 `fn()` 捕获不了状态；`Task::perform` future 必须 `'static` OBSERVED |
| `tests.rs` | 325 | `iced_test` 14 例（view↔update 契约，不跑 Task） | DOCUMENTED（DESIGN.md §9，`cargo test` 全绿） |

### 2.2 后端（`core/`，1255 行，业务逻辑与上一代逐行相同）

| 模块 | 行数 | 职责 |
|---|---|---|
| `event.rs` | 83 | `CoreEvent`{EnvProgress/Log/Url/Exit} + `EventSink`（unbounded，日志洪峰不反压）+ `channel()` |
| `store.rs` | 65 | config.json 读写、数据目录 `%LOCALAPPDATA%\DshDesk\` |
| `profiles.rs` | 186 | profile 增删改查、目录模板 |
| `envres.rs` | 249 | 环境探测（`--version` ×3）、版本查询、PATH 拼装 |
| `installs.rs` | 220 | Node 下载解压、dsh/pnpm 安装 |
| `procman.rs` | 192 | spawn dsh（CREATE_NO_WINDOW）、日志转发、`taskkill /T /F` 杀树、`ProcMap` |
| `plugins.rs` | 240 | 插件装卸、npm `keywords:dsh-plugin` 市场检索 |
| `platform.rs` | ~110 | reg.exe 封装（自启/主题探测）、端口探测（Free/Http/Other）、系统名 | OBSERVED |
| `diag.rs` | ~150 | 脱敏诊断报告（Key 只出长度）；导出到「文档」目录 | OBSERVED |
| `selfupdate.rs` | ~180 | 自更新：Releases 查版 → 下载 → SHA256（PowerShell）→ 换身 .old | OBSERVED |

### 2.3 UI 层（`ui/`，自造组件 + 渲染管线）

| 文件 | 行数 | 职责 | 关键事实 |
|---|---|---|---|
| `mod.rs` | 61 | 文本纪律入口：`txt/txt_bold/mono` 全项目唯一文本通道（钉死内嵌字体 + Advanced shaping）；字体字节 include | OBSERVED |
| `theme.rs`（在 src/） | 332 | 两套 `Palette` 令牌 + 字号/间距/圆角常量（FS_*/GAP_*/R_*） | OBSERVED |
| `widgets.rs` | 670 | 通用控件大杂烩：tag/dot/icon_badge/list_row/divider/empty/busy/field/kv/input/secret_input/check/dropdown/segmented/page_head/page_stack/slim_scrollbar/strip/Toast/toast_host/hoverable/ellipsize | 第一代散文件（sidebar/list_row/toast/tag/segmented）收编于此 OBSERVED |
| `card.rs` | 181 | 卡片容器（`card()`/`card_hero()` 两档高度） | OBSERVED |
| `button.rs` | 255 | 六变体（primary/secondary/danger/teal/quiet-danger/ghost/accent）× 三尺寸 × 禁用态 | OBSERVED |
| `anim.rs` | 118 | `Tween`/`AnimState` 补间驱动；动画期才订阅 `frames()`，空闲零出帧的基石 | OBSERVED |
| `icon.rs` | — | 描边图标（assets/icons/*.svg，`include_bytes!`） | OBSERVED |
| `titlebar.rs` | 231 | 无边框窗口三件套 | `TITLEBAR_H=40`（拖动+三键细条，横跨主区）、`BRAND_H=64`（品牌头部单元，归侧边栏）、`SIDEBAR_W=232`、`resize_grips`（八向 6px 热区）OBSERVED |
| `modal.rs` | 231 | 自造模态（stack + opaque 遮罩 + ESC 全局键盘订阅 + Dialog/Draft） | OBSERVED |
| `reveal.rs` | 165 | 切页入场：`with_translation` 位移（1=刚切 0=落定），t=0 直通零开销 | OBSERVED |
| `frosted.rs` | 387 | **假高斯模糊玻璃卡**：布局跟随内容的自定义 widget；glass 底 quad → 窗口锚定的光球+颗粒共享场（卡片=裁剪窗）→ 内容；滚动平移从 `viewport.pos − MAIN_ORIGIN` 反推；四处嵌套 `with_layer` 都传 `bounds.intersection(viewport)`（iced 嵌套层不复交父裁剪） | `MAIN_ORIGIN=(233, TITLEBAR_H)` OBSERVED |
| `glass_pipeline.rs` | 568 | **自定义 wgpu primitive 管线**（iced_wgpu 0.14 官方通路）：`GlassQuad{Kind::{Background,Card,Veil}}` → `glass.wgsl` 三模式；`background_field()` 环境光、`fade_veil()` 滚动渐隐帏幕 | OBSERVED |
| `glass.wgsl` | — | 单 fragment 三分支：0=背景球场、1=卡片（sheen→渐变→球场→sRGB 灰修正→颗粒→SDF 圆角描边）、2=帏幕（不透明背景场 + 锚定边 smoothstep alpha 坡道） | 帏幕约定：`sheen`=bg 色、`grain_tile.x`=带高物理 px、`grain_tile.y`=方向旗 OBSERVED |
| `glow_mesh.rs` | 183 | `ambient_specs(pal)` 两光球参数唯一来源（窗口绝对坐标，时钟冻结 40.0）；tiny-skia 回退用 Mesh::Solid 扇形 | OBSERVED |
| `tray.rs`（src/） | ~140 | 系统托盘（tray-icon 0.24）：thread_local 持有（TrayIcon !Send），事件流轮询全局 channel | OBSERVED |
| `win32.rs`（src/） | ~70 | 窗口显隐（iced 无 hide/show）：FindWindowW 按标题 + ShowWindow + AttachThreadInput 前置 | OBSERVED |

### 2.4 外壳布局（`pages/mod.rs`，507 行）

```
stack[ shell, toast_host, resize_grips ]
  shell = container( stack[ ambient(背景球场), inner ] )
    inner = column[ titlebar(40px 细条), row[ sidebar(brand_cell 64px + nav + foot), side_divider, main_area ] ]
      main_area = reveal( stack[ scrollable(page_body), fade_veil(14,top), fade_veil(44,bottom) ] )
  模态有 dialog 时整块套 modal::overlay
```
OBSERVED（`pages/mod.rs:99-209`）。Console/Home 不走 scrollable；DrawTicker 放侧边栏（放 scrollable 里会被视口剔除永远不 draw）。

## 3. 关键链路

### 3.1 启动

`main()` → `bridge::init()`（建 channel，**先于** application）→ `iced::application(Dshnext::new, update, pages::view)` → `Message::Opened(id)` 批量触发 `RefreshEnv/RefreshProfiles/PollProcs` + 可选 shot/switch/autotest 定时任务。OBSERVED（`update.rs:21-57`）。

### 3.2 后端事件流（唯一跨线程数据通道）

```
core 函数(tx: &EventSink) → unbounded channel → bridge::events() (Subscription<CoreEvent>)
  → update.rs Message::Core(e) → push_log / urls / sys_log → view 重绘
```
订阅只建一次（recipe hash 去重，STREAM.take() 防重入）。OBSERVED（`bridge.rs:45-66`）。

### 3.3 轮询与空闲纪律

- 进程轮询 `time::every(2s)` **仅当 `procs` 非空**才挂。
- 动画帧 `frames()` 仅当 `anim.is_animating()`。
- 两者皆无 → `Subscription::none()`，空闲零出帧（`--drawlog` 可验）。OBSERVED + DOCUMENTED（DESIGN.md §8）。

### 3.4 渲染栈（视觉的关键 4 层）

1. `ambient`：`GlassQuad::Background` 全窗球场（wgpu）或 Mesh 扇形（tiny-skia）。
2. 卡片：`frosted` widget 自绘 glass quad + 共享颗粒场（窗口锚定，卡片只是裁剪窗）。
3. 内容：普通 iced widget（同 layer 内 quad 先于 text 的次序由 `with_layer` 保证颗粒不污染文字）。
4. 帏幕：`fade_veil` 覆盖滚动区 Fill×Fill，带高 14px(顶)/44px(底)，事件直通（不实现 update，Stack 逆序派发返回 Ignored）。

OBSERVED（`frosted.rs` 头注释、`glass_pipeline.rs:465`、`pages/mod.rs:121-129`）。

## 4. 与 DESIGN.md 的漂移清单（本文档更新后 DESIGN.md §4 已同步指向这里）

| DESIGN.md 说法 | 实际（OBSERVED） |
|---|---|
| §4 目录树：`ui/sidebar.rs / list_row.rs / toast.rs / tag.rs / segmented.rs` | **不存在**，全部收编进 `widgets.rs` |
| §4 目录树未提 | 新增：`ui/{frosted, glass_pipeline, glow_mesh, reveal, titlebar}.rs` + `glass.wgsl`；`bridge.rs`；`update.rs`；`tests.rs` |
| §4「app.rs = State/Message/update/view/subscription」 | update 在 `update.rs`、view 在 `pages/`（app.rs 头注释明说） |
| §2 依赖清单缺 | 实际多出：`iced_wgpu`+`iced_renderer`+`bytemuck`（玻璃管线）、`png`（--shot）、`env_logger`/`log`、`image` feature |
| §7 标题栏一节（BRAND 在 TITLEBAR 内） | fbdc40f 起拆开：40px 细条横跨主区，品牌块是侧边栏独立 64px 头部单元 |
| §1「软回退 15.3 MB 探针」等数字 | 仍是 DOCUMENTED 历史数字，玻璃管线加入后未重测 → **重跑 `measure-idle.ps1` 的活还在待办** |

## 5. 证据索引

- 依赖方向：`grep -rn "crate::ui|crate::pages|crate::app|crate::update|crate::bridge" src/core/`（空）；`grep -rln "crate::core" src/ui/ src/theme.rs`（空）
- 行数：`wc -l src/**/*.rs`（2026-09-06）
- 布局：`src/pages/mod.rs` view()/sidebar()；常量：`src/ui/titlebar.rs:20-24`
- CLI 全集：`src/main.rs:3-9`；启动任务批：`src/update.rs` `Message::Opened` 分支
- 帏幕/玻璃约定另见：AGENTS.md 坑 31/32/33、记忆 `dshnext-frosted-grain-glass.md`
