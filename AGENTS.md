# Dshnext 开发须知

给接手的人（包括未来的自己）。只写**踩过并且会再踩**的坑，不写通用常识。

## 项目现状

- 仓库根就是 Dshnext：**原生 Rust 重写**（iced 0.14 + wgpu，无 WebView）。**阶段 0～5 已完成**（可行性 / 视觉地基 / 后端接入 / 六页移植 / 收尾 / 切页动画与排版收口）。见 `phase0/REPORT.md` 与 `DESIGN.md`。剩的活是干净 VM 上复核首帧与零依赖。
- 第一代（Tauri 2 + React）已于 2026-09-05 删除，代码在 git 历史 `11a2401^` 之前（`git show 11a2401^:src-tauri/src` 可翻出）。它的 `src-tauri/src/` 就是本仓库 `src/core/` 的来源；数据目录 `%LOCALAPPDATA%\DshDesk\` 不变，老用户无缝换用。
- `phase0/`：探针工程，**保留不删**——它是唯一能快速复现性能数字的地方，每次改动都该重跑 `tools/measure-idle.ps1`。

阶段 0 验过的三样已搬进产品代码：`theme.rs` 令牌、字体加载、`WGPU_BACKEND=dx12` 限定（白省 38 MB）。

## 环境

**每条 cargo / npm 命令前都要 `export no_proxy='*' NO_PROXY='*'`。** 机器上配了 127.0.0.1:7890 代理但它是死的，不绕过就一切网络操作超时。cargo 用的是 `rsproxy-sparse` 源。

- Shell 是 **Git Bash on Windows**，工作目录 `D:\DshDesk`
- **`/tmp` 就是 `%TEMP%`**（`C:\Users\24453\AppData\Local\Temp`）。`nohup cmd > /tmp/x.log` 之后用 Python 读 `/tmp/x.log` 会 FileNotFound——Python 看的是真正的 POSIX 路径。写日志用 `"$TEMP/x.log"`，读的时候用完整 Windows 路径。
- Rust 1.92.0 stable MSVC，edition 2024 可用
- **机器级已配 sccache（2026-09-05）**：用户环境变量 `RUSTC_WRAPPER=C:\Users\24453\.cargo\bin\sccache.exe` + `CARGO_PROFILE_DEV_DEBUG=line-tables-only`。依赖 crate 编译结果跨项目共享，`cargo clean` 后重编从十几分钟降到一两分钟；debug 构建不再生成 GB 级 PDB。**新项目自动继承**，无需再配。看缓存命中 `sccache --show-stats`；缓存目录 `%LOCALAPPDATA%` 下默认上限 10 GB。若某次构建报找不到 rustc wrapper，是 setx 未被当前 shell 读到——重开终端。
- Python 3.11.9，已装 `fonttools` `brotli` `pillow`
- `phase0/tools/NotoSansSC-var.ttf`（17 MB 变体源字体）**在磁盘上但不入库**，别重新下载；下载地址在 `build_fonts.py` 头注释里

**改代码前先 `taskkill //F //IM dshnext.exe`**，否则 `cargo build` 报 `拒绝访问 (os error 5)`——Windows 不让覆盖正在运行的 exe。

## PowerShell 脚本

1. **`.ps1` 必须是纯 ASCII，或者加 UTF-8 BOM**（`node tools/add-bom.cjs <file>`）。Windows PowerShell 没 BOM 时按 ANSI 读，中文字面量变成 `鎺у埗鍙?`，所有按中文名匹配的逻辑静默失败。第一代 e2e 因此挂了 8 个断言。
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
7. **iced 窗口在 UIA 树里后代数为 0**（无 AccessKit）。第一代的 `scripts/e2e.ps1`（已随第一代删除）对 Dshnext 完全失效，UI 测试走 `iced_test`。
8. **点击测试前必须确认目标点上没有别的窗口。** `WindowFromPoint` 返回的不是被测窗口就白点了——ZCode 的应用内浏览器窗格（`Chrome_RenderWidgetHostHWND`，属 msedge 进程）会盖在屏幕右侧，害我一度以为 iced 的 `on_press` 坏了。可靠做法：先 `SetWindowPos(hwnd, HWND_TOPMOST, x, y, w, h, SWP_NOACTIVATE)` 把窗口顶到最上并挪到已知位置，每次点击前用 `WindowFromPoint` 断言命中。脚本见 `phase0/tools/click-probe.ps1`。
9. **窗口几何变了要重算坐标。** 最大化后客户区原点和宽度全变，拿旧坐标点「还原」按钮会点空，看起来像按钮失灵。每次点击前重新 `ClientToScreen` + `GetClientRect`，再按缩放比（125%）换算逻辑像素。

## 出图与验收

- `--page <名>` 直接开在某一页（home/profiles/plugins/env/console/settings），`--tall` 把窗口开到 1280×1400 好把设置页一屏截完
- `--shot 路径 --after 毫秒` 自截图退出。**`--after` 要给够**：六页都会在开窗时发环境探测（三次 `xx --version`，每次可能几秒），2 秒会截到还没填好的界面，给 4000 稳当
- **批量出图必须一进程一杀一 sleep**（`taskkill //F //IM dshnext.exe; sleep 1.2` 再启下一个）。紧循环连开 12 个 `--shot` 进程时，上一个窗口没退干净，`PrintWindow` 会抓到**残留窗口**——表现成某页截成了另一页（首页截出设置页）、或截成 min-size 的 1100×702 残片。出完用「选中项图标是 accent 蓝」这个主题无关的信号逐张校验页面身份，别只看尺寸。
- 出图后交 judge 子代理做视觉验收，不要自己看图。两轮下来六页从 3 fail 到全 pass
- **judge 只看得到图上有的东西。** 插件页默认停在「已安装」分页，市场行根本没渲染，让它验「市场行按钮样式」只会换回一条 Unverified。要验分页/悬停/展开后的状态，得先加命令行开关把程序开在那个状态上。
- **judge 抓对齐问题比人靠谱，但结论要自己量一遍再改。** 它报「说明行比标签多缩进 20 物理像素」，量下来确实是 375 vs 405（缩进 24 逻辑像素）——但那 24px 当初是**故意**加的（对齐复选框标签）。真正的毛病是它成了整张卡里唯一不在内容列上的一行，所以改法是删掉缩进、回到内容列，不是微调数值。

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
13. **`Subscription::run` 的 builder 是裸函数指针 `fn()`，捕获不了任何状态。** channel 不能建好再传进去，只能放全局（`OnceLock` + `Mutex<Option<_>>` 等订阅第一次 `take()`）。同理 `Task::perform` 的 future 要 `'static`，跨 Task 共享的东西（如 `ProcMap`）也得放全局。做法见 `src/bridge.rs`。
14. **事件流订阅要返回原始类型让调用方自己 `.map()`**（`bridge::events() -> Subscription<CoreEvent>`），否则为了把 `fn(CoreEvent) -> Message` 塞进裸函数指针得 transmute。
15. **关掉 `web-colors` 后 iced 按物理（线性空间）混色，深底上叠带色相的半透明会被放大得离谱。** 5% 的 `#5b76ff` 叠在 `#18181a` 上实测出 `(31,36,69)`——蓝通道从 26 冲到 69，选中行直接盖过行内按钮。**纪律：半透明叠色只用于中性灰（hover），带色相的一律写死不透明值。**
16. **`checkbox()` 在 0.14 只收 `is_checked`**，标签走 `.label()`（0.13 是 `checkbox(label, value)`）。
17. **`opaque()` 必须包在模态遮罩上**，否则点遮罩会穿透到下层按钮。**ESC 关模态只能走全局键盘订阅**——覆盖层拿不到键盘焦点。
18. **`pick_list` 的 `L: Borrow<[T]>` 接受 `Vec<T>`**，不用为了凑 `&'a [T]` 去 leak。
19. **`Text<'a>` 的 `IntoFragment` 参数别钉 `'static`**（同第 9 条），页面里到处是 `format!` 出来的串。
20. **`windows_subsystem="windows"`（GUI 子系统）会让 wgpu DX12 首帧慢 ~1.1 s。** 同一份代码只改子系统标志：控制台子系统 136 ms、GUI 子系统 1270 ms（本机 RTX 4060，驱动枚举出 6 个重复适配器）。慢在 `init-closure → 首次 view` 之间的 compositor 建设备，与 LTO/日志/内容复杂度都无关。换 Vulkan(497)/GL(456) 快些但不通用，`Vulkan,DX12` 掩码反而更慢（仍枚举 DX12）。**测首帧必须用产品同款子系统**——阶段 0 探针没设这个属性，那个 145 ms 是控制台数字，误导了一版。
21. **`iced_test` 的 `&str` 选择器是精确相等（`content == self`），不是子串**；`Selector` 对 `widget::Id` 也有实现（控件设 `.id()` 即可按 id 选）。`click` 要求目标 `visible_bounds` 非空（滚出视口点不到），`find` 不要求。禁用按钮上的文字被点会冒泡到外层 `mouse_area`——空草稿点模态「创建」不触发 `DialogConfirm`，但会产生遮罩的 `CloseDialog`，所以断言要写「没有 DialogConfirm」而不是「没有任何消息」。喂 `Start/Stop` 前必须先 `bridge::init()`（update 里同步取 `bridge::sink()`）。
22. **「零运行时依赖」要用 `dumpbin /DEPENDENTS` 验，别信「Rust 肯定静态」的直觉。** 默认 MSVC 构建动态依赖 `VCRUNTIME140.dll`（VC++ 运行库）+ 一批 `api-ms-win-crt-*`（UCRT），干净裸机上没有就直接起不来。解法：`.cargo/config.toml` 里 `[build] rustflags = ["-C","target-feature=+crt-static"]`，之后只剩系统 DLL（代价 +0.21 MB）。注意 rustflags 是全局的，`cargo test`/debug 也会静态链（功能无碍，链接稍慢）；临时要动态 CRT 就命令行 `RUSTFLAGS=""` 覆盖。
23. **没有全局 opacity，淡入只能靠「盖一层背景色面纱」，而面纱必须自己 `with_layer`。** `renderer::Style` 只有 `text_color`；wgpu/tiny-skia 在**同一层内都是先画完所有 quad 再画所有 text**，面纱和内容同层的话文字会盖在面纱上面——底色淡入而文字全程清晰，比不做动画更怪。`with_layer` 走 `push_clip`，新层序号更大，稳定画在内容之后。
24. **入场位移用 `renderer.with_translation`，不要动 padding/height。** 后者每帧重新布局（设置页六张卡整树重排），前者只影响 draw、布局逐帧复用。也不要用 `Transformation::scale`：会连文字一起缩，cosmic-text 非整数缩放要么每帧重栅格化要么拉伸图集，本来就没 hinting 的中文会更糊。
25. **要在动画中途截图，光靠 `--shot --after` 不行**——落定态永远是它截到的样子。加了 `--switch-to <页> --switch-at <毫秒>`：开窗后定时切页，`--after` 与它的差就是快门落在过渡的第几毫秒。
26. **`AnimState::animate_to` 起不了「重播」。** 它从当前值出发，而上一次入场落定后当前值已等于目标值，再调等于什么都不动。切页入场要用 `restart(key, from, to, dur, now)` 强制从头跑。补间方向刻意写成 **1 = 刚切、0 = 落定**：`value()` 对无记录的 key 返回 0.0，正好是落定态，冷启动和 `--page` 出图都不必预置初值。

## 后端复用（core/，抄自第一代）

1. **`src/core/` 是从第一代 `src-tauri/src/` 复制的，业务逻辑一行不改。** 改完 `grep -rn tauri src/core` 必须为零。唯一批量改动是模块路径 `crate::xxx` → `crate::core::xxx`。
2. **`EventSink` 用 unbounded channel 是刻意的。** 日志洪峰时反压会把 dsh 卡在写管道上；上限交给 UI 侧的环形缓冲（`VecDeque` 上限 2000）。
3. **`core/mod.rs` 顶部有 `#![allow(dead_code)]`。** 阶段 2 只接了四条链路，安装/插件/市场的函数还没调用方，但它们是上一代验证过的逻辑，**别删**。
4. **插件市场用 npm `keywords:dsh-plugin` 检索，不是 GitHub topic**——后者返回一堆无关仓库（deepseek-harness 21 万星、reactive-resume 之类）。
5. **验证后端链路用 `--e2e`，不要用鼠标坐标。** 程序自己跑「启动第一个 profile → 8s → 停止」，`RUST_LOG=dshnext=debug` 能看到每个 `CoreEvent` 到达 `update()`。
6. **`RUST_LOG=dshnext=debug` 会打出每条 Message**（`update.rs` 开头，已排除高频的 Tick/ToastTick）。交互测不出效果时先看这个——分得清「消息没到」和「到了但逻辑不对」。
7. **交互实测用 `phase0/tools/interact-probe.ps1`**：模态、ESC、Ctrl+1..6、侧边栏点击一套跑完。坐标是**逻辑像素**，脚本按 1.25 缩放换算；窗口 1600×1120 物理 = 1280×896 逻辑，x 超过 1280 就点到客户区外面去了（第一版就是这么白点的）。
8. **「在途请求」不能只看结果是否为空判重。** 反复进环境页会重复拉版本列表——`dsh_versions.is_empty()` 在请求飞在半路时仍为真。要单独一个 `versions_loading` 标志。

## NSIS 安装包（packaging/）

1. **`makensis` 报 `Can't open output file` 通常是上一次装测的 setup 进程还活着。** NSIS 静默安装（`/S`）在文件拷完后自身可能仍驻留，占着输出文件名。`tasklist //FI "IMAGENAME eq Dshnext_0.1.0_x64-setup.exe"` 一查就见，`taskkill //F //PID` 掉再编。
2. **`/D=` 参数必须走 `cmd //c`。** Git Bash 会把 `/D=C:\...` 当路径改写（`/S "/D=..."` 也一样），NSIS 收到畸形参数直接退 2 且什么都不装。写 `cmd //c "setup.exe /S /D=C:\目标"`。`/D=` 还必须是**最后一个**参数且不能加引号（NSIS 的规矩）。
3. **卸载段只写 `RMDir "$SMPROGRAMS\..."` 删不掉开始菜单文件夹**——`RMDir` 不删非空目录，两个 `.lnk` 还在里面，于是每次卸载都留一个死文件夹。必须先逐个 `Delete` 快捷方式再 `RMDir`。这条只有真装真卸一遍才看得见。
4. **验证包内 exe 是不是当前构建，用哈希不用大小。** `7z e setup.exe dshnext.exe` 抽出来跟 `target/release/dshnext.exe` 比 SHA-256；只看字节数会被「改了代码但体积没变」骗过去。
5. `.ps1` 那条 BOM 纪律不适用于 `.nsi`——**`installer.nsi` 保持纯 ASCII**（文件头注释里也写着），makensis 按 ACP 读脚本，中文进去就是乱码。
