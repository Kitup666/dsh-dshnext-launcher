# DshDesk / Dshnext 开发须知

给接手的人（包括未来的自己）。只写**踩过并且会再踩**的坑，不写通用常识。

## 项目现状

- `src/` + `src-tauri/`：**第一代，Tauri 2 + React，功能完整可用**，已出 NSIS 安装包（3,071,492 字节）。全流程与插件流程各跑通过一次。**不因 Dshnext 重构而停止维护。**
- `Dshnext/`：原生 Rust 重写（iced 0.14 + wgpu）。**阶段 0～3 已完成**（可行性 / 视觉地基 / 后端接入 / 六页移植）。见 `Dshnext/phase0/REPORT.md` 与 `Dshnext/DESIGN.md`。下一步是阶段 4「收尾」。
- `Dshnext/phase0/`：探针工程，**保留不删**——它是唯一能快速复现性能数字的地方，每次改动都该重跑 `tools/measure-idle.ps1`。

阶段 0 验过的三样已搬进产品代码：`theme.rs` 令牌、字体加载、`WGPU_BACKEND=dx12` 限定（白省 38 MB）。

## 环境

**每条 cargo / npm 命令前都要 `export no_proxy='*' NO_PROXY='*'`。** 机器上配了 127.0.0.1:7890 代理但它是死的，不绕过就一切网络操作超时。cargo 用的是 `rsproxy-sparse` 源。

- Shell 是 **Git Bash on Windows**，工作目录 `D:\DshDesk`
- **`/tmp` 就是 `%TEMP%`**（`C:\Users\24453\AppData\Local\Temp`）。`nohup cmd > /tmp/x.log` 之后用 Python 读 `/tmp/x.log` 会 FileNotFound——Python 看的是真正的 POSIX 路径。写日志用 `"$TEMP/x.log"`，读的时候用完整 Windows 路径。
- Rust 1.92.0 stable MSVC，edition 2024 可用
- Python 3.11.9，已装 `fonttools` `brotli` `pillow`
- `Dshnext/phase0/tools/NotoSansSC-var.ttf`（17 MB 变体源字体）**在磁盘上但不入库**，别重新下载；下载地址在 `build_fonts.py` 头注释里

**改代码前先 `taskkill //F //IM <name>.exe`**，否则 `cargo build` 报 `拒绝访问 (os error 5)`——Windows 不让覆盖正在运行的 exe。

## PowerShell 脚本

1. **`.ps1` 必须是纯 ASCII，或者加 UTF-8 BOM**（`node scripts/add-bom.cjs <file>`）。Windows PowerShell 没 BOM 时按 ANSI 读，中文字面量变成 `鎺у埗鍙?`，所有按中文名匹配的逻辑静默失败。第一代 e2e 因此挂了 8 个断言。
2. **`Add-Type` 里用 `System.Drawing` 要显式 `-ReferencedAssemblies System.Drawing`**（不是 `System.Drawing.Common`），否则内联类编译失败、类型根本不存在，报错是莫名的 `TypeNotFound`。
3. `GetCurrentThreadId` 在 **kernel32**，不在 user32。P/Invoke 声明错了会运行时才崩。
4. `SendKeys` **表达不了 Ctrl+Space**（`"^{ }"` 报「关键字无效」）。切输入法用 `PostMessage(WM_INPUTLANGCHANGEREQUEST)`。
5. Rust 程序的日志是 UTF-8，**PowerShell 和 grep 显示为乱码**。要读内容用 Python：`open(p,'rb').read().decode('utf-8')`。

## Windows 界面自动化

1. **`SetForegroundWindow` 在调用方不是前台进程时静默失败**。必须先 `AttachThreadInput` 借用当前前台线程的输入队列。不做这步，所有按键会跑到别的窗口，看起来像被测程序坏了。
2. **CapsLock 开着时微软拼音直接透传英文**，IME 根本不打开。测中文输入前先 `GetKeyState(0x14)` 检测并关掉。
3. **不要往 iced 窗口里点鼠标。** `text_input` 在任何落空的点击上都会 unfocus，点在卡片背景上就把焦点弄丢了，`on_input` 再也不触发。聚焦交给程序自己（`operation::focus`），脚本只负责激活窗口和敲键。
4. 读坐标前先 `SetProcessDPIAware()`。屏幕 2560×1600 @125% 缩放，不声明会拿到逻辑坐标、截错区域。
5. **UIA 或截图之前先恢复并重定位窗口。** 最小化的窗口 rect 是 -25600,-25600，UIA 树是空的，截出来是 200×34 的残片。
6. 截图用 `PrintWindow(hwnd, hdc, 2)`（PW_RENDERFULLCONTENT）抓窗口自己的表面，别的程序抢焦点也污染不了。
7. **iced 窗口在 UIA 树里后代数为 0**（无 AccessKit）。第一代的 `scripts/e2e.ps1` 对 Dshnext 完全失效，阶段 4 要换 `iced_test`。
8. **点击测试前必须确认目标点上没有别的窗口。** `WindowFromPoint` 返回的不是被测窗口就白点了——ZCode 的应用内浏览器窗格（`Chrome_RenderWidgetHostHWND`，属 msedge 进程）会盖在屏幕右侧，害我一度以为 iced 的 `on_press` 坏了。可靠做法：先 `SetWindowPos(hwnd, HWND_TOPMOST, x, y, w, h, SWP_NOACTIVATE)` 把窗口顶到最上并挪到已知位置，每次点击前用 `WindowFromPoint` 断言命中。脚本见 `Dshnext/phase0/tools/click-probe.ps1`。
9. **窗口几何变了要重算坐标。** 最大化后客户区原点和宽度全变，拿旧坐标点「还原」按钮会点空，看起来像按钮失灵。每次点击前重新 `ClientToScreen` + `GetClientRect`，再按缩放比（125%）换算逻辑像素。

## Dshnext 出图与验收

- `--page <名>` 直接开在某一页（home/profiles/plugins/env/console/settings），`--tall` 把窗口开到 1280×1400 好把设置页一屏截完
- `--shot 路径 --after 毫秒` 自截图退出。**`--after` 要给够**：六页都会在开窗时发环境探测（三次 `xx --version`，每次可能几秒），2 秒会截到还没填好的界面，给 4000 稳当
- 出图后交 judge 子代理做视觉验收，不要自己看图。两轮下来六页从 3 fail 到全 pass

## 性能测量

1. **进程树必须递归遍历。** WebView2 是 `dshdesk.exe` → `msedgewebview2.exe`(browser) → GPU/renderer/utility 共 7 个进程。只走一层父子关系只找到 2 个，**内存少算 230 MB、CPU 少算 100 倍**——照那个数字比，反倒是 WebView2「更省」。
2. **报内存要用 `\Process(*)\Working Set - Private`**（任务管理器「内存」列），不是 `PrivateMemorySize64`。后者是提交的虚拟内存，NVIDIA 驱动能撑到 200+ MB 而页面并不驻留。
3. **冷启动要测「首帧画好」，不是「窗口出现」。** Tauri 空窗口 92 ms 就弹出来了，内容要等 WebView2 加载 bundle（236 ms）。判定方式见 `phase0/tools/first-paint.ps1`（数窗口表面的颜色数）。
4. 测空闲出帧数**不能走 Message**，否则订阅自己会触发下一帧，测出来是假的。探针的做法是自定义 widget 在 `draw()` 里累加原子计数。

## iced 0.14 具体坑

1. **零尺寸 widget 永远不会被绘制。** `Column::draw` 用 `bounds().intersects(viewport)` 剔除子元素，零面积矩形永不相交。（这条剔除逻辑对日志虚拟滚动是好消息。）
2. **`.theme()` / `.style()` 要传具名函数，不能传闭包**，否则报 `implementation of FnOnce is not general enough`（HRTB 推不出来）。
3. **令牌结构体做成 `Copy` 按值传**，用 `&Palette` 会撞 `view()` 返回 `Element<'_>` 的生命周期。
4. 是 `iced::widget::space::horizontal()`，**没有** `horizontal_space()`。
5. **`Shadow` 没有 spread。** 第一代 CSS 大量用负 spread 收缩阴影（`0 20px 40px -24px`），iced 只能靠调小 `blur_radius` 近似（40 → 30 观感等价）。
6. **OpenType feature 用不了。** iced 从不设置 cosmic-text 的 `font_features`，`tnum` 不可达。等宽数字只能靠字体天然等宽——**换字体前先验十个数字的 advance 是否一致**。
7. **子集字体必须写 name ID 16/17**（typographic family/subfamily）。`fontdb` 优先用 ID 16 做家族键；只改 ID 1 会让 SemiBold 注册成独立家族，`Font::with_name(...) + Weight::Semibold` **静默落到系统字体**，截图上看不出来。`varLib.instancer` 还要带 `--update-name-table`。
8. 后端切换：`ICED_BACKEND=tiny-skia` 选软件渲染器；`WGPU_BACKEND=dx12` 限定 GPU 后端。wgpu 是在 compositor 创建时才读后者，**晚于 `main`**，所以进程内 `set_var` 有效。
9. **`Text<'a>` 对 `'a` 不变（invariant）。** 文本封装函数签名写 `impl IntoFragment<'static> -> Text<'static>` 会让 `format!` 出来的串塞不进短生命周期的 `Column`（报 "borrowed data escapes"）。统一写 `<'a>`。
10. **`mouse_area` 要求 `Message: Clone + 'static`。** 一路传染到所有包了它的组件函数签名。
11. **`stack!` 里的覆盖层会吃掉下层点击。** `Stack::update` 逆序派发、先到的先 capture。透明热区放最上层是对的，但**热区之间的空隙必须是裸 `Space`，不能包 `mouse_area`**，否则整个内容区点不动。
12. **无边框窗口（`decorations: false`）连缩放边框一起没了**，八向 `drag_resize` 热区、拖动 `window::drag`、最小化/最大化/关闭全要自己接。细节见 DESIGN.md §7.6。
13. **`Subscription::run` 的 builder 是裸函数指针 `fn()`，捕获不了任何状态。** channel 不能建好再传进去，只能放全局（`OnceLock` + `Mutex<Option<_>>` 等订阅第一次 `take()`）。同理 `Task::perform` 的 future 要 `'static`，跨 Task 共享的东西（如 `ProcMap`）也得放全局。做法见 `Dshnext/src/bridge.rs`。
14. **事件流订阅要返回原始类型让调用方自己 `.map()`**（`bridge::events() -> Subscription<CoreEvent>`），否则为了把 `fn(CoreEvent) -> Message` 塞进裸函数指针得 transmute。
15. **关掉 `web-colors` 后 iced 按物理（线性空间）混色，深底上叠带色相的半透明会被放大得离谱。** 5% 的 `#5b76ff` 叠在 `#18181a` 上实测出 `(31,36,69)`——蓝通道从 26 冲到 69，选中行直接盖过行内按钮。**纪律：半透明叠色只用于中性灰（hover），带色相的一律写死不透明值。**
16. **`checkbox()` 在 0.14 只收 `is_checked`**，标签走 `.label()`（0.13 是 `checkbox(label, value)`）。
17. **`opaque()` 必须包在模态遮罩上**，否则点遮罩会穿透到下层按钮。**ESC 关模态只能走全局键盘订阅**——覆盖层拿不到键盘焦点。
18. **`pick_list` 的 `L: Borrow<[T]>` 接受 `Vec<T>`**，不用为了凑 `&'a [T]` 去 leak。
19. **`Text<'a>` 的 `IntoFragment` 参数别钉 `'static`**（同第 9 条），页面里到处是 `format!` 出来的串。

## Dshnext 后端复用

1. **`core/` 是从第一代 `src-tauri/src/` 复制的，业务逻辑一行不改。** 改完 `grep -rn tauri src/core` 必须为零。唯一批量改动是模块路径 `crate::xxx` → `crate::core::xxx`。
2. **`EventSink` 用 unbounded channel 是刻意的。** 日志洪峰时反压会把 dsh 卡在写管道上；上限交给 UI 侧的环形缓冲（`VecDeque` 上限 2000）。
3. **`core/mod.rs` 顶部有 `#![allow(dead_code)]`。** 阶段 2 只接了四条链路，安装/插件/市场的函数还没调用方，但它们是上一代验证过的逻辑，**别删**。
4. **验证后端链路用 `--e2e`，不要用鼠标坐标。** 程序自己跑「启动第一个 profile → 8s → 停止」，`RUST_LOG=dshnext=debug` 能看到每个 `CoreEvent` 到达 `update()`。
5. **`RUST_LOG=dshnext=debug` 会打出每条 Message**（`update.rs` 开头，已排除高频的 Tick/ToastTick）。交互测不出效果时先看这个——分得清「消息没到」和「到了但逻辑不对」。
6. **交互实测用 `phase0/tools/interact-probe.ps1`**：模态、ESC、Ctrl+1..6、侧边栏点击一套跑完。坐标是**逻辑像素**，脚本按 1.25 缩放换算；窗口 1600×1120 物理 = 1280×896 逻辑，x 超过 1280 就点到客户区外面去了（第一版就是这么白点的）。
7. **「在途请求」不能只看结果是否为空判重。** 反复进环境页会重复拉版本列表——`dsh_versions.is_empty()` 在请求飞在半路时仍为真。要单独一个 `versions_loading` 标志。

## 第一代（Tauri）专有

- Vite 必须 `watch: { ignored: ["**/src-tauri/**"] }`，否则监视 `dshdesk_lib.dll` 报 EBUSY
- 自定义 hook 要单独放文件（`src/useToasts.ts`），跟组件混在一起会让 HMR 报 `Could not Fast Refresh` 并把页面搞死
- 端口 1420 被残留的 vite 占住时，先杀进程再重启
- 插件市场用 **npm `keywords:dsh-plugin` 检索**，不是 GitHub topic——后者返回一堆无关仓库（deepseek-harness 21 万星、reactive-resume 之类）
