# Dshnext

DshDesk 的**原生 Rust 重写**（iced + wgpu，无 WebView）。目标：低 CPU/GPU/内存 + 视觉不退步 + 单 exe 零运行时依赖。

**当前状态：阶段 0（可行性）、阶段 1（视觉地基 + 暗色精修 + 无边框窗口）、阶段 2（后端接入）已完成。下一步是阶段 3「页面移植」。**

后端已经真的接上了：环境探测、配置读写、profile 列表、进程启停全部实跑通过，
`dsh`/`node`/`pnpm` 版本是从子进程真实读出来的，日志走 tokio channel 进 UI。

- 阶段 0 实测报告：[phase0/REPORT.md](phase0/REPORT.md)
- 阶段 1 出图：`shots/phase1-{dark,light}.png`，并排对比 `shots/compare-{dark,light}.png`
- 暗色精修（阶段 1.5）：`shots/compare-orevx.png`（左 orevx 参考、中旧暗色、右新暗色），设计依据见 [DESIGN.md §6 暗色精修](DESIGN.md)
- 阶段 2 出图：`shots/phase2-dark.png`（真实环境数据）、`shots/phase2-running.png`（e2e 跑起来后的日志卡）
- 完整设计：[DESIGN.md](DESIGN.md)
- 上一代（可用）：`../src-tauri` + `../src`，Tauri 2 + React，已出 NSIS 安装包

## 怎么跑

```bash
cd Dshnext
cargo run --release                      # 开窗，鼠标 hover 看过渡，按 T 切主题
cargo run --release -- --shot out.png --after 2500 --theme dark   # 自截图退出
cargo run --release -- --autotest --drawlog   # 程序自己触发 hover，出帧日志证明动画结束帧归零
cargo run --release -- --e2e                  # 自动跑一遍「启动第一个 profile → 8s → 停止」，验证后端链路
```

`--drawlog` 每 5s 打印真实绘制次数（独立线程数帧，不走 Message）。空闲时 delta=0；hover 一次补间约十几帧后归零。

窗口是**无边框**的（`decorations: false`），系统那条带最小化/最大化/关闭的原生外框已去掉，改成自绘标题栏（`src/ui/titlebar.rs`）：
- 标题文字区可拖动窗口
- 右上三个按钮：最小化 / 最大化-还原 / 关闭（关闭 hover 变红）
- 窗口四边四角各有 6px 缩放热区，光标会变成对应的缩放箭头

## 阶段 0 实测结果

同一台机器（RTX 4060 / 14 核 / Win11）上量的真实数字，不是估计：

| 指标 | Tauri 版（实测） | Dshnext 探针 | |
|---|---|---|---|
| 空闲 CPU（全核） | 0.26% | **0.0000%** | 120s 内 0 帧 |
| 空闲 GPU | 1.72% | **0.00%** | |
| 常驻内存（私有工作集） | **199 MB**（7 进程） | **79.8 MB**（DX12）<br>15.3 MB（软件渲染） | 降 60% |
| 首帧画好 | 236 ms | **145 ms** | 快 91 ms |
| 单 exe | 3 MB + 需 WebView2 | **15.22 MB** 零依赖 | 含 4.34 MB 字体 |
| 中文渲染 | — | ✅ 无豆腐块，等宽数字对齐 | |
| 中文输入法 | — | ✅ 微软拼音全链路正常 | |
| 无 GPU 时 | — | ✅ tiny-skia 下中文正常 | 与 GPU 输出差 0.65/255 |

内存原目标是 ≤ 40 MB，**没达到，也不可能达到**：79.8 MB 里约 75 MB 是显卡驱动 + wgpu 设备的常驻开销，iced 自身加字体只占 5 MB。目标已按后端分档修正（DESIGN.md §1），判据改成「显著低于上一代」。

## 目录

```
phase0/       阶段 0 探针工程 + 实测报告 + 测量脚本（保留，用于性能回归）
src/main.rs   入口：DX12 限定 + 字体加载 + --shot/--autotest/--drawlog/--e2e
src/app.rs    顶层 State/Message/update/view/subscription
src/bridge.rs core 与 iced 的桥：channel / ProcMap 全局持有 + Subscription 事件流
src/theme.rs  两套设计令牌（照抄上一代 styles.css）
src/ui/       通用组件：anim/card/button/icon/titlebar
src/core/     从上一代复制的后端（1163 行逻辑不改 + 新增 event.rs；阶段 2 已接入）
src/pages/    六个页面（阶段 3）
assets/       图标（svg 已建 6 个）、内嵌字体（4.34 MB，阶段 0 已生成）
docs/         旧 commands.rs / Cargo.toml，移植时对照用
```

## 与上一代的差异

| | DshDesk（Tauri） | Dshnext（iced） |
|---|---|---|
| 前端 | React + WebView2 | 纯 Rust GPU 渲染 |
| 进程数 | 7 | **1** |
| 分发 | 3 MB + 需系统 WebView2 | 单 exe 15.22 MB，零依赖 |
| 内存 | 199 MB | 79.8 MB（GPU）/ 15.3 MB（软件） |
| WebUI 窗口 | 内置窗口 | **交给系统浏览器** |
| 可访问性 | WebView2 自带 a11y 树 | **暂无**（iced 0.14 无 AccessKit，UIA 树里后代数为 0） |
| 自动化测试 | UIA 脚本（`../scripts/e2e.ps1`） | `iced_test`（框架内模拟） |
| 数据目录 | `%LOCALAPPDATA%\DshDesk\` | 同一个，完全兼容 |

取舍清单见 [DESIGN.md §12](DESIGN.md)。

## 下一步

DESIGN.md §10 的**阶段 2：后端接入** —— `core/event.rs`（Emitter → channel 机械替换）、`Subscription::run` 接 channel、环境探测与配置读写打通。

阶段 1 已把探针验证过的三样搬进产品代码：`theme.rs` 令牌、字体加载、`WGPU_BACKEND=dx12` 限定（main 开头 set_var，白省 38 MB）。
