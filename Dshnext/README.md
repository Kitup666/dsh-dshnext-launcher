# Dshnext

DshDesk 的**原生 Rust 重写**（iced 0.14 + wgpu，无 WebView）。目标：低 CPU/GPU/内存 + 视觉不退步 + 单 exe 零运行时依赖。

**当前状态：阶段 0～5 全部完成。** 六个页面可用、后端真实接通、`iced_test` 用例全绿、性能逐项实测、单 exe + NSIS 安装包已产出；阶段 5 补了切页入场动画与一轮排版/层级收口，空闲零帧不变。

功能完整：六个页面（启动/版本管理/插件管理/环境/控制台/设置）、四种模态、toast、
Ctrl+1..6 切页、无边框自绘标题栏。后端是真的——`dsh`/`node`/`pnpm` 版本从子进程读出，
profile 扫真实目录，启停走 spawn/taskkill，日志经 tokio channel 进 UI。

## 实测结果（阶段 4，本机 RTX 4060 / 14 核 / Win11）

| 指标 | Tauri 上一代 | Dshnext | 判据 |
|---|---|---|---|
| 空闲 CPU（全核） | 0.26% | **0.0015%** | ✅ < 0.1% |
| 空闲 GPU | 1.72% | **0.00%** | ✅ ~0% |
| 空闲出帧 | 常驻 setInterval | **delta=0**（开窗后零帧；切页动画那 5s 出 31 帧，之后立刻归零） | ✅ 0 |
| 常驻内存（私有工作集） | 199 MB（7 进程） | **88.5 MB**（DX12，稳定不爬） | ✅ ≤ 90，降 56% |
| 单 exe | 3 MB + 需 WebView2 | **17.31 MB** 零依赖（含 4.34 MB 字体；`+crt-static`） | ✅ ≤ 18 |
| 首帧画好 | 236 ms | ⚠️ 本机 GUI+DX12 **~1.27 s**（见下） | 待干净 VM 复核 |
| 功能对等 | — | `iced_test` 14 例全绿 + `--e2e` 启停链路 | ✅ |

**首帧这一行是阶段 4 排查出来的真相**：阶段 0 那个漂亮的 145 ms 是**探针**测的，而探针是控制台子系统程序（没设 `windows_subsystem="windows"`）。产品为了双击启动不闪黑框用 GUI 子系统。同一份代码只改子系统标志：控制台 136 ms、GUI+DX12 1270 ms、GUI+Vulkan 497 ms、GUI+软件渲染 212 ms。慢在 wgpu 建设备那一段，根因是**这台机器的 NVIDIA 驱动枚举出 6 个重复适配器**（`iced_wgpu` 日志可见），属机器特异性开销，干净单显卡机器不会付这笔钱。保持 DX12（兼容性 + 内存优势），首帧判据留到「干净 Win10 VM」那一行复核。完整分析见 [DESIGN.md §9「首帧真相」](DESIGN.md)。

## 怎么跑

```bash
cd Dshnext
cargo run --release                      # 开窗，鼠标 hover 看过渡，按 T 切主题
cargo test                               # 14 例 iced_test（headless，无需 GPU/窗口）
cargo run --release -- --shot out.png --after 5000 --theme dark   # 自截图退出
cargo run --release -- --autotest --drawlog   # 程序自己触发 hover，出帧日志证明动画结束帧归零
cargo run --release -- --e2e                  # 自动跑「启动第一个 profile → 8s → 停止」，验证后端链路
cargo run --release -- --page env             # 直接开在某一页（home/profiles/plugins/env/console/settings）
cargo run --release -- --page settings --tall # 长页面出图用，窗口开到 1280x1400
# 截切页动画的中间帧：--switch-at 与 --after 的差就是快门落在过渡的第几毫秒
cargo run --release -- --page home --switch-to settings --switch-at 4000 --shot mid.png --after 4095
```

调试交互用 `RUST_LOG=dshnext=debug`，每条 Message 都会打出来（Tick/ToastTick 已排除）。
诊断后端选择临时开 `RUST_LOG=iced_wgpu=info`（默认 warn，因为适配器列表打印很贵）。

窗口是**无边框**的（`decorations: false`），改成自绘标题栏（`src/ui/titlebar.rs`）：
标题区可拖动，右上最小化/最大化-还原/关闭三键，四边四角 6px 缩放热区。

## 打包

- **单 exe（推荐，绿色版）**：`cargo build --release` → `target/release/dshnext.exe`（17.31 MB，零运行时依赖，拷走即用）。
  `.cargo/config.toml` 里已固化 `-C target-feature=+crt-static`：默认 MSVC 构建会动态依赖 `VCRUNTIME140.dll`（VC++ 运行库，干净机器上没有），静态链 CRT 后 `dumpbin /DEPENDENTS` 只剩 kernel32/user32/gdi32 这类系统 DLL——这才是真正的「零运行时依赖」。
- **NSIS 安装包（可选）**：`packaging/installer.nsi`，用 `makensis` 编译 → 每用户安装（`$LOCALAPPDATA\Programs\Dshnext`，免管理员），带开始菜单项与卸载器。注册表标识用 `Dshnext` 与上一代的 `DshDesk` 分开，避免互相覆盖卸载项。

## 出图与对照

- 六页 × 明暗：`shots/p5-<页面>-{dark,light}.png`（阶段 4 的那批留在 `p4-*`）
- **与上一代并排对比**：`shots/compare-p5/<页面>.png`（左 Tauri、右 Dshnext，六页齐全；`python tools/make-compare.py --gen p5` 重出）
- 切页动画取证：`shots/anim/{t075,t035,settled}.png`（同一次过渡的三个时刻）
- 暗色精修依据：`shots/compare-orevx.png`；设计全文：[DESIGN.md](DESIGN.md)
- 阶段 0 探针与测量脚本：`phase0/`（保留，性能回归用；每次改动重跑 `phase0/tools/measure-idle.ps1`）

## 目录

```
phase0/       阶段 0 探针工程 + 实测报告 + 测量脚本（保留，用于性能回归）
src/main.rs   入口：DX12 限定 + 字体加载 + --shot/--autotest/--drawlog/--e2e/--switch-to
src/app.rs    顶层 State/Message
src/update.rs 全部状态迁移 + subscription（空闲零订阅）
src/tests.rs  iced_test 用例（view↔update 契约，14 例）
src/bridge.rs core 与 iced 的桥：channel / ProcMap 全局持有 + Subscription 事件流
src/theme.rs  设计令牌：两套配色 + 字号阶梯 + 圆角/节奏/阴影两档层级
src/ui/       通用组件：anim/button/card/icon/modal/reveal/titlebar/widgets
src/core/     从上一代复制的后端（业务逻辑不改 + 新增 event.rs）
src/pages/    六个页面
assets/       图标（10 个 svg）、内嵌字体（4.34 MB）
packaging/    NSIS 安装包脚本
```

## 与上一代的差异

| | DshDesk（Tauri） | Dshnext（iced） |
|---|---|---|
| 前端 | React + WebView2 | 纯 Rust GPU 渲染 |
| 进程数 | 7 | **1** |
| 分发 | 3 MB + 需系统 WebView2 | 单 exe 17.31 MB，零依赖（CRT 静态链） |
| 内存 | 199 MB | 88.5 MB（GPU）/ 15.3 MB（软件） |
| WebUI 窗口 | 内置窗口 | **交给系统浏览器** |
| 可访问性 | WebView2 自带 a11y 树 | **暂无**（iced 0.14 无 AccessKit，UIA 树里后代数为 0） |
| 自动化测试 | UIA 脚本（`../scripts/e2e.ps1`） | `iced_test`（框架内模拟，`src/tests.rs`） |
| 数据目录 | `%LOCALAPPDATA%\DshDesk\` | 同一个，完全兼容 |

**已知退步**：无屏幕阅读器支持（可访问性）；本机 GUI+DX12 首帧偏慢（驱动特异性）。取舍清单见 [DESIGN.md §12](DESIGN.md)。

上一代（Tauri 版）保持可用，**不因本重构而删除或停止维护**，直到 Dshnext 在干净机器上全部达标。
