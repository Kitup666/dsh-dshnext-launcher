# Dshnext 开发须知

给接手的人（包括未来的自己）。只写**踩过并且会再踩**的坑，不写通用常识。

## 项目现状

- 仓库根就是 Dshnext：**原生 Rust 重写**（iced 0.14 + wgpu，无 WebView）。**阶段 0～5 已完成**（可行性 / 视觉地基 / 后端接入 / 六页移植 / 收尾 / 切页动画与排版收口）。见 `phase0/REPORT.md` 与 `DESIGN.md`。剩的活是干净 VM 上复核首帧与零依赖。
- 第一代（Tauri 2 + React）已于 2026-09-05 删除，代码在 git 历史 `11a2401^` 之前（`git show 11a2401^:src-tauri/src` 可翻出）。它的 `src-tauri/src/` 就是本仓库 `src/core/` 的来源；数据目录 `%LOCALAPPDATA%\DshDesk\` 不变，老用户无缝换用。
- `phase0/`：探针工程，**保留不删**——它是唯一能快速复现性能数字的地方，每次改动都该重跑 `tools/measure-idle.ps1`。

阶段 0 验过的三样已搬进产品代码：`theme.rs` 令牌、字体加载、`WGPU_BACKEND=dx12` 限定（白省 38 MB）。

## 环境

**每条 cargo / npm 命令前都要 `export no_proxy='*' NO_PROXY='*'`。** 127.0.0.1:7890 是 **FlClash**（`tasklist //FI "IMAGENAME eq FlClash.exe"` 可查）：它没开时代理是死的，不绕过就一切网络操作超时；它开着时代理可用，rsproxy/domestic 源照样直连不受影响。cargo 用的是 `rsproxy-sparse` 源。

**被墙域名单独走 FlClash**：`raw.githubusercontent.com` 直连必挂（api.github.com 恰好能直连，别被它骗了以为网络没问题），fetch 这类文件用 `curl -x http://127.0.0.1:7890 ...`；FlClash 没开就退回 WebFetch（服务端抓取，不吃本机网络）。

- Shell 是 **Git Bash on Windows**，工作目录 `D:\DshDesk`
- **`/tmp` 就是 `%TEMP%`**（`C:\Users\24453\AppData\Local\Temp`）。`nohup cmd > /tmp/x.log` 之后用 Python 读 `/tmp/x.log` 会 FileNotFound——Python 看的是真正的 POSIX 路径。写日志用 `"$TEMP/x.log"`，读的时候用完整 Windows 路径。
- Rust 1.92.0 stable MSVC，edition 2024 可用
- **机器级已配 sccache（2026-09-05）**：用户环境变量 `RUSTC_WRAPPER=C:\Users\24453\.cargo\bin\sccache.exe` + `CARGO_PROFILE_DEV_DEBUG=line-tables-only`。依赖 crate 编译结果跨项目共享，`cargo clean` 后重编从十几分钟降到一两分钟；debug 构建不再生成 GB 级 PDB。**新项目自动继承**，无需再配。看缓存命中 `sccache --show-stats`；缓存目录 `%LOCALAPPDATA%` 下默认上限 10 GB。若某次构建报找不到 rustc wrapper，是 setx 未被当前 shell 读到——重开终端。
- Python 3.11.9，已装 `fonttools` `brotli` `pillow`
- `phase0/tools/NotoSansSC-var.ttf`（17 MB 变体源字体）**在磁盘上但不入库**，别重新下载；下载地址在 `build_fonts.py` 头注释里

**改代码前先 `taskkill //F //IM dshnext.exe`**，否则 `cargo build` 报 `拒绝访问 (os error 5)`——Windows 不让覆盖正在运行的 exe。
**实测前必须 `cargo build --release`**——`cargo test` 不更新 `target/release/dshnext.exe`。一天内两次拿旧 exe 跑 e2e/自愈测试得出错误结论（「StartProbed 没出现」「自动重启不生效」，其实都生效了）。
**Git Bash 里别写 `>nul`**——那是 cmd 的设备名，bash 会创建真实文件 `nul`，`git add -A` 直接撞死（`unable to index file 'nul'`）。重定向到黑洞用 `>/dev/null`。

## PowerShell 脚本

1. **`.ps1` 必须是纯 ASCII，或者加 UTF-8 BOM**（`node tools/add-bom.cjs <file>`）。Windows PowerShell 没 BOM 时按 ANSI 读，中文字面量变成 `鎺у埗鍙?`，所有按中文名匹配的逻辑静默失败。第一代 e2e 因此挂了 8 个断言。**`.bat` 同理且没有 BOM 出路**——cmd 按 ACP 读，UTF-8 中文注释变乱码后把行解析搅碎（实测 cmd 拿 `'see'`/`'iles'` 这种碎片当命令执行），dev.bat 里只能写 ASCII 注释。
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

- **单实例互斥对自动化放行**（2026-09-07 起）：裸启动撞上已有实例会前置对方窗口然后退出；`--shot`/`--autotest`/`--e2e`/`--migrate-to`/`--migrate-go` 不拿互斥——批量出图跟用户正开的实例并存是设计内的。反过来，测「双开拦截」时不要带这些旗子。
- **dsh 的 WebUI 只挂在带 token 的地址上**（裸 `http://127.0.0.1:3080/` 是 401「authentication required」），token **只出现在 dsh 启动时的 stdout**（`CoreEvent::Url` 解析），dsh 侧不落盘、CLI 也没有补打的开关。**例外（2026-09-09，有意为之）**：启动器在 Url 事件把最近地址覆盖写进 `<数据目录>/webui-url.txt`（`store::save_webui_url`），供 `DeepseekHarness.exe` 双击独立打开；文件在用户级 LOCALAPPDATA，ACL 天然只本用户。**除此之外启动器仍任何路径都不许开裸地址**：`Started` 里 `auto_open` 走 `pending_open` 等 Url 事件；端口被残留/外部实例占着（探到 Http 且非自己起的）时拿不到 token，只能提示、不能开。启动器重启后对已在跑的实例永远无法恢复入口——要换入口只能停了重起。


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
20. **默认后端已锁 `vulkan`，别改回 `dx12`**（2026-09-06）：dx12 帧提交追不上快速拖边的窗口尺寸，DWM 拉伸上一帧成「放大再缩小」残影（用户实测 vulkan 全无）；vulkan 首帧 ~500ms 也比 dx12 ~1300ms 快。旧机器没 vulkan 时用 `WGPU_BACKEND=dx12` 环境变量兜底。**`windows_subsystem="windows"`（GUI 子系统）会让 wgpu 首帧慢 ~1.1 s（dx12 实测数字）。** 同一份代码只改子系统标志：控制台子系统 136 ms、GUI 子系统 1270 ms（本机 RTX 4060，驱动枚举出 6 个重复适配器）。慢在 `init-closure → 首次 view` 之间的 compositor 建设备，与 LTO/日志/内容复杂度都无关。换 Vulkan(497)/GL(456) 快些但不通用，`Vulkan,DX12` 掩码反而更慢（仍枚举 DX12）。**测首帧必须用产品同款子系统**——阶段 0 探针没设这个属性，那个 145 ms 是控制台数字，误导了一版。
21. **`iced_test` 的 `&str` 选择器是精确相等（`content == self`），不是子串**；`Selector` 对 `widget::Id` 也有实现（控件设 `.id()` 即可按 id 选）。`click` 要求目标 `visible_bounds` 非空（滚出视口点不到），`find` 不要求。禁用按钮上的文字被点会冒泡到外层 `mouse_area`——空草稿点模态「创建」不触发 `DialogConfirm`，但会产生遮罩的 `CloseDialog`，所以断言要写「没有 DialogConfirm」而不是「没有任何消息」。喂 `Start/Stop` 前必须先 `bridge::init()`（update 里同步取 `bridge::sink()`）。
22. **「零运行时依赖」要用 `dumpbin /DEPENDENTS` 验，别信「Rust 肯定静态」的直觉。** 默认 MSVC 构建动态依赖 `VCRUNTIME140.dll`（VC++ 运行库）+ 一批 `api-ms-win-crt-*`（UCRT），干净裸机上没有就直接起不来。解法：`.cargo/config.toml` 里 `[build] rustflags = ["-C","target-feature=+crt-static"]`，之后只剩系统 DLL（代价 +0.21 MB）。注意 rustflags 是全局的，`cargo test`/debug 也会静态链（功能无碍，链接稍慢）；临时要动态 CRT 就命令行 `RUSTFLAGS=""` 覆盖。
23. **没有全局 opacity，淡入只能靠「盖一层背景色面纱」，而面纱必须自己 `with_layer`。** `renderer::Style` 只有 `text_color`；wgpu/tiny-skia 在**同一层内都是先画完所有 quad 再画所有 text**，面纱和内容同层的话文字会盖在面纱上面——底色淡入而文字全程清晰，比不做动画更怪。`with_layer` 走 `push_clip`，新层序号更大，稳定画在内容之后。**但面纱盖的是整块区域，不是内容本身**：区域实际背景比面纱色亮时背景会跟着变暗——切页面纱因此被用户否掉（2026-09-05 删，只留位移）。模态遮罩用同一招没问题，因为「变暗」在那里正是目的。
24. **入场位移用 `renderer.with_translation`，不要动 padding/height。** 后者每帧重新布局（设置页六张卡整树重排），前者只影响 draw、布局逐帧复用。也不要用 `Transformation::scale`：会连文字一起缩，cosmic-text 非整数缩放要么每帧重栅格化要么拉伸图集，本来就没 hinting 的中文会更糊。
25. **要在动画中途截图，光靠 `--shot --after` 不行**——落定态永远是它截到的样子。加了 `--switch-to <页> --switch-at <毫秒>`：开窗后定时切页，`--after` 与它的差就是快门落在过渡的第几毫秒。
26. **`AnimState::animate_to` 起不了「重播」。** 它从当前值出发，而上一次入场落定后当前值已等于目标值，再调等于什么都不动。切页入场要用 `restart(key, from, to, dur, now)` 强制从头跑。补间方向刻意写成 **1 = 刚切、0 = 落定**：`value()` 对无记录的 key 返回 0.0，正好是落定态，冷启动和 `--page` 出图都不必预置初值。
27. **`image::Handle::from_bytes` 的 id 是 `Id::unique()`，在 `view()` 里现造 = 每次都是「新图」**，缓存穿透 → 悬停任何按钮触发 view 重建时头像闪烁。内嵌位图要用 `OnceLock` 把 Handle 存成全局单例（`brand_handle()`）。SVG 的 `from_memory` 没这个问题——它的 id 按内容 hash（`iced_core/src/svg.rs`），`&'static [u8]` 天然稳定，所以界面图标可以直接现造。
28. **iced 0.14 没有 `window::hide/show`**（runtime 全文只有 minimize）——「关闭到托盘」只能自己 `ShowWindow`，hwnd 按标题 `FindWindowW`（src/win32.rs）。标题是找窗的钥匙：`.title()` 与找窗必须同源（`app::WINDOW_TITLE` 常量）。
29. **`tray-icon` 的 `TrayIcon` 不是 Send**（内部 `Rc<RefCell>`）——托盘对象放 `thread_local`，只在主线程 update 里创建/移除；事件走 crate 全局 channel（订阅流跑在 executor worker 线程，只能轮询 channel，别碰托盘对象）。Windows 上托盘的消息窗口依赖创建线程的消息泵——winit 主线程满足。
30. **`Task::perform` 的 future 借不了局部变量**（`latest(&url)` 报 E0597）——API 设计成收所有权 `String`，调用方 clone 进去。
28. **自定义 wgpu 管线走 iced_wgpu 0.14 官方 primitive 通路**（`primitive::{Primitive, Pipeline}` + `Renderer::draw_primitive`，`draw()` 返回 true 可直接画进 iced 的大 pass；见 `src/ui/glass_pipeline.rs`）。三个保命点：同层 flush 顺序固定 **quads→triangles→primitives→images→text**（layer.rs `start()/end()`），primitive 永远在同层 quad/mesh 之后、image/text 之前；`prepare` 拿到的 bounds **已带上层变换**（滚动 with_translation、reveal 位移），卡片矩形不用自己做平移数学；`Pipeline::trim()` 每帧末调用，帧内槽位计数靠它复位。
29. **dst-read 混合（真 backdrop-filter / 逐像素混合模式）在 wgpu 27 下不可达**：primitive 的 `render()` 回调只给 `TextureView`，拿不到 `Texture` 句柄做 `copy_texture_to_texture`，wgpu 也没有 framebuffer fetch。已知底图时用**解析求值**替代读画布（glass.wgsl 在 fragment 里重算光球场——数学上还更准）。
30. **双后端编译时 `iced::Renderer` 是 fallback 枚举**（`Primary`=wgpu、`Secondary`=tiny-skia，变体公开可 match），后端专属 widget 按变体分路（`frosted.rs`/`glass_pipeline.rs` 的背景 widget）。`iced::renderer` 模块是私有的，枚举要从 `iced_renderer` crate 引；wgpu 本体用 `iced::wgpu` 再导出（版本与 iced 锁死，**别单独加 wgpu 依赖**，否则 trait 签名对不上）。
31. **嵌套 `with_layer` 的 bounds 不与父裁剪求交。** `push_clip`（iced_graphics layer.rs）直接 `layers.push(with_bounds(bounds))`，不复交——scrollable 用 `with_layer(visible_bounds)` 裁内容，里面的 frosted 卡片再开自己的层时若传**内容坐标的全尺寸 bounds**，就逃出滚动裁剪：滚上去的卡片画到标题栏上。修法：层边界一律传 `bounds.intersection(viewport)`（frosted.rs 的 `clipped`）。viewport 已是内容坐标系，push_clip 会乘上层变换把它变回物理坐标，不用再手算滚动偏移。
32. **玻璃图元有「viewport == rect」逐位相等契约**（glass_pipeline `snap_to_physical`，2026-09-06）：顶点着色器画全屏大三角形，fragment 用 `phys = rect.xy + uv × rect.wh` 反推屏幕坐标，而 uv 是按 `draw_primitive` 传入的 bounds（→ set_viewport，小数照设）映射的——**bounds 与 u.rect 差半像素，整个坐标系被拉伸，1px 描边的 SDF 随机落到错误的行上**，表现为「卡片描边亮线随机缺某一段/某一条边」（实测还有弧形咬痕）。draw 与 prepare 必须传同一个取整到物理网格的矩形；draw 拿不到 scale，用 prepare 存的原子 static 中转。裁剪层可再放宽 1px（层里只有玻璃图元，圆角外 alpha=0）。
33. **滚动页顶/底渐隐帏幕（`glass_pipeline::fade_veil`，shader 模式 2）是不透明背景场 + 带高 alpha 渐隐**，色 = bg_app + 光球，与窗外背景续上；顶帏带宽必须**窄于**内容顶 padding（现 14 < 44）——等宽的话稍滚一点首行标题就整个被吃掉（用户实测否掉 44）。帏幕不实现 update，stack 逆序派发返回 Ignored，滚轮/拖拽照常到达 scrollable。

## WebView2 桌面窗口（core/webview.rs）

「桌面窗口」打开方式：用系统 WebView2 开一个**启动器自己的顶层窗口**（独立图标/标题，dock 里不跟浏览器混）。踩过的坑：

1. **不能用 wry 的 tao 窗口层**：tao 是 winit 分支，与 iced 主线程的 winit 在同进程抢全局状态（窗口类注册/DPI context），**副线程建 tao 窗口会原生崩溃**（不是 Rust panic，`any_thread`+COM 初始化都压不住，`panic=abort` 下整个进程没）。所以窗口用 `windows` crate 手写原生 Win32（RegisterClassEx/CreateWindowEx/GetMessage 泵），只借 wry 做 WebView2 那层（经 raw-window-handle 递 HWND）。
2. **桌面窗口是独立二进制 `DeepseekHarness.exe`**（workspace 成员 `webui/`，2026-09-09 二次改；此前「同进程线程 → --webview-host 子进程 → --webview-relay 中继 → 硬链接换皮」四代方案全部作废，教训见下）。为什么必须独立 bin：dock/任务管理器按**进程/文件名**归组，而 **MyDockFinder 的图标与显示名读 exe 内嵌资源**（图标资源 + FileDescription）——硬链接与启动器同字节，改名不改皮，图标永远甩不开。webui/ 只有薄 main + 自己的 build.rs（鲸鱼 `assets/icons/webui.ico` + `[package.metadata.winresource]` 写 FileDescription/ProductName；winresource 0.1.31 的字符串版信息**只能走 Cargo.toml metadata**，没有 set_file_description 这类方法）。链接器把没用到的 iced 全剥了，宿主 exe 仅 ~1.9 MB。`ensure_host()` spawn 同目录兄弟 `DeepseekHarness.exe`（缺 → Err 提示重装）；管道协议不变：**stdin 每行一个 URL**（token 不上命令行）、关窗 **stdout 写 `closed`**、父退出 → EOF → `WM_APP_QUIT` 自毁（DestroyWindow 必须属主线程，走 PostMessage）。`host_main()` 双模式判据 = `GetFileType(GetStdHandle(STD_INPUT_HANDLE))` 是否 PIPE：非管道（双击，GUI 子系统无 stdio）→ 独立模式：`win32::raise_if_exists(WEBUI_TITLE)` 有窗即前置退出，否则读数据目录 `webui-url.txt`（见「token 落盘例外」）；读不到 MessageBox 提示后退出。启动器侧 `open()` 在 HOST 为空时先 `raise_if_exists`——已有独立窗口就复用不起第二个（代价：不导航新 URL、close_stops 不生效）。
3. **每窗口 AUMID 走属性存储，不走 ITaskbarList3**（2026-09-09 修正，原结论「走不通」作废）：`ITaskbarList3::SetAppUserModelID` 确实没在 windows 0.62 投影，但 per-window 身份的正路是 `SHGetPropertyStoreForWindow::<IPropertyStore>(hwnd)` + `SetValue(&PKEY_AppUserModel_ID, VT_BSTR PROPVARIANT)` + `Commit()`。features 要 `Win32_UI_Shell_PropertiesSystem`（子模块单独门控）+ `Win32_Storage_EnhancedStorage` + `Win32_System_Com_StructuredStorage` + `Win32_System_Variant`。PROPVARIANT 的联合体套 `ManuallyDrop` 字段**不自动 DerefMut**，要显式 `(*pv.Anonymous.Anonymous).vt = VT_BSTR` 这样写，用完 `ManuallyDrop::drop` 释放 BSTR。**⚠️ 读回陷阱**：`GetValue` 回来的类型是 `VT_VECTOR|VT_UI1`（字节序列化 UTF-16），**不是写进去的 VT_BSTR**——按 bstrVal 解释直接段错误（实测崩在窗口创建后一行）。先验 `vt` 再取。`SetCurrentProcessExplicitAppUserModelID` 仍是进程级（会连主窗口一起改名），别碰。
4. **图标**：`dsh.ico` 用 `include_bytes!` 内嵌，运行时解析 ICO 目录取最大图 → `CreateIconFromResourceEx` → 设进 `WNDCLASSEXW.hIcon/hIconSm`（类图标，`WM_GETICON` 查不到属正常）。
5. **运行时检测**：`reg query` EdgeUpdate 客户端 GUID `{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}`（HKLM WOW6432/HKLM/HKCU 任一存在即在）。缺则 `webview::open` 返回 Err、toast 提示安装，**按用户要求不静默回退浏览器**。
6. 依赖：`wry` + `windows`（Foundation/WindowsAndMessaging/Gdi/LibraryLoader/Com）+ `raw-window-handle`。exe 体积 +~0.3MB（不打包内核，WebView2Loader 系统自带动态加载）。

## 后端复用（core/，抄自第一代）

1. **`src/core/` 是从第一代 `src-tauri/src/` 复制的，业务逻辑一行不改。** 改完 `grep -rn tauri src/core` 必须为零。唯一批量改动是模块路径 `crate::xxx` → `crate::core::xxx`。
2. **`EventSink` 用 unbounded channel 是刻意的。** 日志洪峰时反压会把 dsh 卡在写管道上；上限交给 UI 侧的环形缓冲（`VecDeque` 上限 2000）。
3. **`core/mod.rs` 顶部有 `#![allow(dead_code)]`。** 阶段 2 只接了四条链路，安装/插件/市场的函数还没调用方，但它们是上一代验证过的逻辑，**别删**。
4. **插件市场用 npm `keywords:dsh-plugin` 检索，不是 GitHub topic**——后者返回一堆无关仓库（deepseek-harness 21 万星、reactive-resume 之类）。
5. **验证后端链路用 `--e2e`，不要用鼠标坐标。** 程序自己跑「启动第一个 profile → 8s → 停止」，`RUST_LOG=dshnext=debug` 能看到每个 `CoreEvent` 到达 `update()`。
6. **`RUST_LOG=dshnext=debug` 会打出每条 Message**（`update.rs` 开头，已排除高频的 Tick/ToastTick）。交互测不出效果时先看这个——分得清「消息没到」和「到了但逻辑不对」。
7. **交互实测用 `phase0/tools/interact-probe.ps1`**：模态、ESC、Ctrl+1..6、侧边栏点击一套跑完。坐标是**逻辑像素**，脚本按 1.25 缩放换算；窗口 1600×1120 物理 = 1280×896 逻辑，x 超过 1280 就点到客户区外面去了（第一版就是这么白点的）。
8. **「在途请求」不能只看结果是否为空判重。** 反复进环境页会重复拉版本列表——`dsh_versions.is_empty()` 在请求飞在半路时仍为真。要单独一个 `versions_loading` 标志。

## 目录迁移（core/migrate.rs）

1. **npm 依赖树里全是 NTFS junction，`fs::copy` 对目录链接直接「拒绝访问 (os error 5)」。** 第一版迁移就在 `home/profiles/node_modules` 半路翻车。检测用 `fs::symlink_metadata().is_symlink()`（**Python 的 `os.path.islink` 不认 junction**，排查时别被骗）；重建用 `junction` crate（std 没有 `symlink_junction`，`symlink_dir` 又要特权）。junction 目标是带 `\\?\` 前缀的绝对路径，重建前必须剥前缀做旧根→新根改写，否则删了旧位置链接全断。
2. **迁移是两阶段：先全量复制（单文件 3 次退避重试，吃杀软/索引器的瞬时锁），全部成功才删源。** 复制失败 = 什么都不动原地可重试；删除失败只记 warnings（数据已双份）。
3. **home 复制必须排在数据目录删除之前、「当前生效 home」必须在改道之前抓取**——默认 home 就住在数据目录里（`<数据>/home`），顺序不对会把要复制的源先删掉。这条 bug 曾被测试执行顺序掩盖，加了 junction 测试才现形；迁移测试务必把 home 覆盖设成非默认值再跑。
4. **验证迁移结果别信启动日志的 `data_dir=`**——main.rs 那行打的是迁移前捕获的局部变量。看 pointer 文件（`%LOCALAPPDATA%\DshDesk\launcher-dir.txt`）或迁移后新起的进程。

## NSIS 安装包（packaging/）

1. **`makensis` 报 `Can't open output file` 通常是上一次装测的 setup 进程还活着。** NSIS 静默安装（`/S`）在文件拷完后自身可能仍驻留，占着输出文件名。`tasklist //FI "IMAGENAME eq Dshnext_0.1.0_x64-setup.exe"` 一查就见，`taskkill //F //PID` 掉再编。
2. **`/D=` 参数必须走 `cmd //c`。** Git Bash 会把 `/D=C:\...` 当路径改写（`/S "/D=..."` 也一样），NSIS 收到畸形参数直接退 2 且什么都不装。写 `cmd //c "setup.exe /S /D=C:\目标"`。`/D=` 还必须是**最后一个**参数且不能加引号（NSIS 的规矩）。
3. **卸载段只写 `RMDir "$SMPROGRAMS\..."` 删不掉开始菜单文件夹**——`RMDir` 不删非空目录，两个 `.lnk` 还在里面，于是每次卸载都留一个死文件夹。必须先逐个 `Delete` 快捷方式再 `RMDir`。这条只有真装真卸一遍才看得见。
4. **验证包内 exe 是不是当前构建，用哈希不用大小。** `7z e setup.exe dshnext.exe` 抽出来跟 `target/release/dshnext.exe` 比 SHA-256；只看字节数会被「改了代码但体积没变」骗过去。
5. `.ps1` 那条 BOM 纪律不适用于 `.nsi`——**`installer.nsi` 保持纯 ASCII**（文件头注释里也写着），makensis 按 ACP 读脚本，中文进去就是乱码。
