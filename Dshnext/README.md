# Dshnext

DshDesk 的**原生 Rust 重写**（iced + wgpu，无 WebView）。目标：低 CPU/GPU/内存 + 视觉不退步 + 单 exe 零运行时依赖。

**当前状态：阶段 0 可行性验证已完成，五项全部通过，可以进入阶段 1（视觉地基）。**

- 阶段 0 实测报告：[phase0/REPORT.md](phase0/REPORT.md)
- 完整设计：[DESIGN.md](DESIGN.md)
- 上一代（可用）：`../src-tauri` + `../src`，Tauri 2 + React，已出 NSIS 安装包

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
phase0/      阶段 0 探针工程 + 实测报告 + 四个测量脚本（保留，用于性能回归）
src/core/    从上一代复制的后端（1163 行，逻辑不改，只切断 Tauri 耦合）
src/pages/   六个页面（待写）
src/ui/      通用组件（待写）
assets/      图标（已复制）、内嵌字体（4.34 MB，阶段 0 已生成）
docs/        旧 commands.rs / Cargo.toml，移植时对照用
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

DESIGN.md §10 的**阶段 1：视觉地基** —— `theme.rs` 两套令牌、软阴影卡片与四类按钮、过渡动画（关键验证点：动画结束后空闲 CPU 要回到 0）。

进阶段 1 之前要先把探针里验证过的三样搬进产品代码：`theme.rs` 令牌、字体加载、`WGPU_BACKEND=dx12` 限定（白省 38 MB）。
