# Dshnext

DshDesk 的**原生 Rust 重写**（iced + wgpu，无 WebView）。目标：低 CPU/GPU/内存 + 视觉不退步 + 单 exe 零运行时依赖。

**当前状态：设计阶段，尚未开始编码。**

- 完整设计：[DESIGN.md](DESIGN.md)
- 上一代（可用）：`../src-tauri` + `../src`，Tauri 2 + React，已出 NSIS 安装包

## 目录

```
src/core/    从上一代复制的后端（1163 行，逻辑不改，只切断 Tauri 耦合）
src/pages/   六个页面（待写）
src/ui/      通用组件（待写）
assets/      图标（已复制）、内嵌字体（待加）
docs/        旧 commands.rs / Cargo.toml，移植时对照用
```

## 与上一代的差异

| | DshDesk（Tauri） | Dshnext（iced） |
|---|---|---|
| 前端 | React + WebView2 | 纯 Rust GPU 渲染 |
| 分发 | 3 MB + 需系统 WebView2 | 目标单 exe ≤ 15 MB，零依赖 |
| 内存 | 80–150 MB | 目标 ≤ 40 MB |
| WebUI 窗口 | 内置窗口 | **交给系统浏览器**（唯一功能退步） |
| 数据目录 | `%LOCALAPPDATA%\DshDesk\` | 同一个，完全兼容 |

## 下一步

先做 DESIGN.md §10 的**阶段 0**：验证中文渲染、中文输入法、空闲占用三件事。任一不达标就终止本方向、留在 Tauri 版。
