# 阶段 0 实测报告

一台机器上的实测数据，用于判定 `iced` 原生方向是否继续。测试日期 2026-09-04。

**结论：五项验证全部通过，但内存目标（≤ 40 MB）在 GPU 后端下未达标，需要在阶段 1 之前先解决。**

## 测试环境

| 项 | 值 |
|---|---|
| 系统 | Windows 11 (10.0.22631) x64 |
| CPU | 14 逻辑核 |
| GPU | NVIDIA GeForce RTX 4060 Laptop（驱动 610.62），另有 5 个虚拟显示适配器 |
| 屏幕 | 2560×1600 @ 120 DPI（缩放 1.25） |
| Rust | 1.92.0 stable MSVC |
| iced | 0.14.0（`default-features = false`，feature 见 `Cargo.toml`） |
| 对照 | 上一代 Tauri 版 `dshdesk.exe` release 构建 |

## 1. 中文渲染 —— 通过

截图：[render-wgpu.png](shots/render-wgpu.png)

| 检查项 | 结果 |
|---|---|
| 常规中文（GB2312 范围内） | ✅ 清晰，无豆腐块 |
| 字重对比（400 / 600） | ✅ 两档明显可辨 |
| 全角标点「」（）《》…→√ | ✅ 全部正常，间距正确 |
| 生僻字（`龘㸻鑫燚囍`，子集外） | ✅ 由系统雅黑接管，无豆腐块 |
| 日文假名 / 韩文 | ✅ 正常（假名在子集内，韩文走系统回退） |
| 中英数混排 | ✅ 基线对齐正常 |
| 等宽数字对齐 | ✅ 三行不同数字末尾严格对齐 |

放大对照：[crop-rare.png](shots/crop-rare.png)（生僻字与多语种）、[crop-nums.png](shots/crop-nums.png)（等宽数字）

### 字体方案（原设计的「1–2 MB 子集」估计偏低）

`assets/fonts/`，由 `tools/build_fonts.py` 生成：

| 文件 | 大小 | 字形数 | 数字等宽 |
|---|---|---|---|
| `NotoSansSC-Regular.subset.ttf` | 2.16 MB | 7750 | ✅ 天然等宽（521/1000 em） |
| `NotoSansSC-SemiBold.subset.ttf` | 2.16 MB | 7750 | ✅ |
| `CascadiaMono.subset.ttf` | 18 KB | 246 | ✅ |
| **合计** | **4.34 MB** | | |

覆盖范围：ASCII + Latin-1 + 通用/CJK 标点 + 全角 + 平假名片假名 + **GB2312 全部 6763 汉字**。

三个当时不知道、现在确认的事实：

1. **`tnum` feature 用不上，但也不需要。** iced 从不设置 cosmic-text 的 `font_features`（`iced_graphics` 里搜不到该字段），所以 CSS 那个 `font-variant-numeric: tabular-nums` 在 iced 侧没有对应开关。**幸运的是 Noto Sans SC 和 Cascadia Mono 的数字本来就是等宽的**（十个数字 advance 完全相同），不依赖 OpenType feature。**这条要写进纪律：换字体必须先验数字 advance 是否一致。**
2. **`varLib.instancer` 必须带 `--update-name-table`。** 不带的话每个静态实例都继承变体字体的默认名（`Noto Sans SC Thin`），两个字重会注册成两个家族。
3. **必须显式写 name ID 16/17（typographic family/subfamily）。** `fontdb` 优先用 ID 16 做家族键，回退才用 ID 1。只改 ID 1 会让 SemiBold 变成独立家族 `Noto Sans SC SemiBold`，于是 `Font::with_name("Noto Sans SC") + Weight::Semibold` 静默落到系统字体上。第一轮就踩了这个坑。

覆盖范围曾试过扩到 GBK（21886 汉字），字体涨到 14.21 MB，单 exe 直接超标。**选择 GB2312 + 系统回退**：子集外的字由 cosmic-text 的 Windows 回退表（按 script 分流，`Script::Han` + `zh-CN` → `Microsoft YaHei UI`）接管，实测生僻字正常显示。

## 2. 软阴影圆角卡片 —— 通过

截图：[crop-shadow.png](shots/crop-shadow.png)（三档模糊半径 18/30/60）

`container::Style { border: Border{radius}, shadow: Shadow{color, offset, blur_radius} }` 一次绘制出圆角 + 软阴影，观感与上一代 CSS `box-shadow` 一致。

一个 API 差异要记下来：**iced 的 `Shadow` 没有 spread**。上一代大量用负 spread 收缩阴影（`0 20px 40px -24px`），iced 侧只能靠调小 `blur_radius` 近似。`theme.rs` 里 40 → 30 是这么来的，观感等价。

## 3. 中文输入法 —— 通过

截图：[ime-preedit.png](shots/ime-preedit.png)（候选框展开）、[ime-committed.png](shots/ime-committed.png)（提交后）

微软拼音全链路正常：

```
ime Opened
ime Preedit("s", Some(1..1))
ime Preedit("sh", Some(2..2))
...
ime Preedit("sh'en'du'qi'u's'uo", Some(18..18))
input = "桑恩度其实是说"（7 字符）
ime Preedit("", None)
ime Commit("桑恩度其实是说")
ime Closed
input = "桑恩度其实是"（6 字符）      ← 一次退格删掉整个汉字
```

| 检查项 | 结果 |
|---|---|
| 候选框位置 | ✅ 紧贴输入框下沿、对齐光标 x |
| preedit 就地显示 | ✅ 拼音带分隔符 `sh'en'du'qi'u's'uo` 显示在输入框内 |
| 候选列表 | ✅ 六个候选 + 翻页箭头，微软拼音原生 UI |
| Commit | ✅ 完整中文串一次性提交 |
| 退格 | ✅ 按整个汉字删除，不是按字节 |

驱动脚本 `tools/ime-drive.ps1`，三个坑都写在文件头注释里：

- **`SetForegroundWindow` 在调用方不是前台进程时静默失败。** 第一轮所有按键跑到了别的窗口（`ZTools`），日志里一个 IME 事件都没有。要先 `AttachThreadInput` 借用当前前台线程的输入队列。
- **CapsLock 开着时微软拼音直接透传英文**，IME 根本不打开。第二轮日志是 `Character("H")`、`modifiers: 0x0`，看起来像 IME 坏了，其实是大写锁定。脚本现在先检测并关掉。
- **不能往窗口里点鼠标。** iced 的 `text_input` 在任何落空的点击上都会 unfocus，点在卡片背景上就把焦点弄丢了，`on_input` 再也不触发。改成程序自己 `operation::focus` 聚焦，脚本只负责激活窗口和敲键。

## 4. tiny-skia 软件回退 —— 通过

截图：[render-tinyskia.png](shots/render-tinyskia.png)

`ICED_BACKEND=tiny-skia` 下**中文渲染完全正常**，避开了 Slint 那个「软渲染器仅支持西文」的坑。与 GPU 后端逐像素比较：平均差 0.65/255，最大差 67，肉眼分辨不出（差异集中在字形抗锯齿边缘与阴影渐变带）。

顺手确认 GPU 后端之间**逐像素完全一致**：`WGPU_BACKEND=dx12` 与 Vulkan 的输出 `identical=True`，说明视觉不受后端选择影响。

## 5. 空闲占用 —— CPU/GPU 通过，内存不达标

### 出帧数：空闲真的零帧

用一个自定义 widget 在 `draw()` 里累加计数（不经过 Message，否则订阅自己会触发下一帧）：

```
draws total=2 delta=2   ← 开窗两帧
draws total=2 delta=0   ← 之后 115 秒，一帧都没有
```

**「reactive rendering」是真的。** 顺带发现 `Column::draw` 用 `bounds().intersects(viewport)` 剔除子元素——零面积 widget 永远不被绘制（第一版计数器就栽在这），这条剔除逻辑对 §7.3 的日志虚拟滚动是好消息。

### 三方对照（60 秒采样，`tools/measure-idle.ps1`）

| 指标 | iced（GPU，全后端） | iced（tiny-skia） | Tauri 版（7 进程） |
|---|---|---|---|
| 空闲 CPU（全核占比） | **0.0000%** | **0.0000%** | 0.2573% |
| 空闲 CPU（单核占比） | **0.00%** | **0.00%** | 3.60% |
| 空闲 GPU | **0.00%** | **0.00%** | 1.72% |
| 工作集 | 181 MB | **33.9 MB** | 391 MB |
| **私有工作集**（任务管理器「内存」列） | **117.8 MB** | **15.3 MB** | 199.1 MB |
| 进程数 | 1 | 1 | 7 |

两个测量方法上的坑，值得记住：

- **进程树必须递归遍历。** WebView2 是 `dshdesk.exe` → `msedgewebview2.exe`（browser）→ GPU/renderer/utility 子进程。只走一层只找到 2 个进程，内存少算 230 MB，CPU 少算 100 倍（0.0015% vs 0.2573%）。
- **报 `PrivateMemorySize64` 会严重高估。** 那是提交的虚拟内存，NVIDIA 驱动会撑到 200+ MB 而页面并不驻留。任务管理器「内存」列显示的是 `\Process(*)\Working Set - Private`，上表用的是后者。

### 内存归因：117.8 MB 里是什么

| 配置 | 私有工作集 |
|---|---|
| 默认（`Backends::all()`，Vulkan 被选中） | 117.8 MB |
| `--no-aa`（关 MSAAx4） | 117.3 MB |
| `--no-fonts`（不加载 4.34 MB 内嵌字体） | 113.0 MB |
| `WGPU_BACKEND=dx12` | **79.8 MB** |
| `WGPU_BACKEND=vulkan` | 91.4 MB |
| `WGPU_BACKEND=gl` | **57.4 MB** |
| `ICED_BACKEND=tiny-skia`（纯软件） | **15.3 MB** |

**结论：内存的大头是 GPU 驱动，不是 iced，也不是字体。**

- 抗锯齿：0.5 MB，可忽略
- 内嵌字体：4.8 MB，与 4.34 MB 的文件大小相符，合理
- **后端选择：38 MB。** iced 默认 `Backends::all()` —— Vulkan、DX12、GL 三套加载器全部初始化再挑一个。显式限定 DX12 直接省下 38 MB
- 剩下约 75 MB 是 NVIDIA 用户态驱动 + wgpu 设备/队列/管线的常驻开销，**这部分不在我们的控制范围内**

已验证 `--dx12` 开关（进程内 `set_var("WGPU_BACKEND", "dx12")`，因为 wgpu 是在 compositor 创建时才读该变量，晚于 `main`）确实生效：日志里 8 处 `backend: Dx12`、0 处 Vulkan，私有工作集 79.8 MB。**这是产品里限定后端的可行做法。**

### 与 §1 硬目标对照

| 目标 | 阈值 | 实测 | 判定 |
|---|---|---|---|
| 空闲 CPU | ≈ 0% | 0.0000% | ✅ |
| 空闲 GPU | ≈ 0% | 0.00% | ✅ |
| 常驻内存 | ≤ 40 MB | 79.8 MB（DX12）/ 15.3 MB（软件） | ❌ GPU 下超标一倍 |
| 冷启动到首帧 | ≤ 300 ms | 145 ms | ✅ |
| 单 exe | ≤ 15 MB | 15.22 MB | ⚠️ 擦线 |

§9 的失败退出条件里有一条是「内存优化幅度 < 30%（即 > 100 MB）就终止」。按私有工作集算：199.1 → 79.8 MB，**降幅 60%，未触发退出**。但 40 MB 的硬目标在有 GPU 驱动的机器上达不到，**这个目标本身需要修正**（见下文「必须补的三件事」）。

## 6. 冷启动与首帧

原设计只写了「≤ 300 ms 到首帧」，但没定义怎么测。**「窗口出现」和「画面画好」是两个时刻，Tauri 版差得尤其远**——空窗口 92 ms 就出来了，内容要等 WebView2 加载完 bundle。

用窗口自身表面的颜色数判定首帧（`tools/first-paint.ps1`，`PrintWindow(PW_RENDERFULLCONTENT)` + 采样网格数不同颜色，空白表面只有 1–2 种）：

| | 窗口出现 | **首帧画好** |
|---|---|---|
| iced（4 轮平均） | 123 ms | **145 ms** |
| Tauri 版（4 轮平均） | 92 ms | **236 ms** |

**窗口出现 iced 慢 31 ms（要初始化 wgpu 设备），首帧画好 iced 快 91 ms。** 用户感知的是后者。

## 7. 单 exe 体积

| 配置 | 大小 |
|---|---|
| `lto=thin, codegen-units=1, strip=true` | 17.97 MB |
| 加 `panic = "abort"` | **15.22 MB** |
| 再去掉 `svg` feature | 13.38 MB |

其中 4.34 MB 是内嵌字体。`panic = "abort"` 省 2.75 MB，§8 已经写了要开。

`svg` 能再省 1.84 MB，但图标要改用 `canvas` 手绘。**保留 `svg`**：15.22 MB 只超目标 0.22 MB，而阶段 1 还没开始写代码、体积只会再涨，到时候再决定是否砍。

## 必须补的三件事（阶段 1 之前）

1. **§1 的内存目标从「≤ 40 MB」改为分档。** 40 MB 在软件渲染下轻松达到（15.3 MB），在任何有独显驱动的机器上都不可能——75 MB 是驱动常驻，不是我们的代码。建议改成「GPU 后端 ≤ 90 MB，软件后端 ≤ 30 MB，且必须显著低于上一代的 199 MB」。
2. **产品里必须显式限定 `WGPU_BACKEND=dx12`。** 白省 38 MB。iced 没有暴露 `wgpu::Backends` 的设置入口，只能在 `main` 开头 `set_var`。这条要写进 §8 性能纪律。
3. **字体纪律：换字体先验数字 advance。** iced 不设置 OpenType feature，`tnum` 指望不上，等宽数字只能靠字体天然等宽。同时 `build_fonts.py` 的 name table 处理（ID 16/17）不能省。

## 建议同时修正的两处文档

- **§9 的「冷启动 ≤ 300 ms」要区分窗口与首帧**，否则这个指标可以被「先弹空窗口」刷掉。
- **§1「常驻内存：上一代 80–150 MB」是估计值，实测 199.1 MB 私有工作集 / 391 MB 工作集。** 附录里那条「未实测对照」现在可以划掉。

## 复现方法

```bash
cd Dshnext/phase0

# 字体（需要 python + fonttools，源字体已在 tools/）
python tools/build_fonts.py

cargo build --release

# 中文渲染 + 软阴影，自己截图后退出
./target/release/phase0.exe render --shot shots/render-wgpu.png
ICED_BACKEND=tiny-skia ./target/release/phase0.exe render --shot shots/render-tinyskia.png

# 中文输入法（脚本会自己激活窗口、关 CapsLock、切中文、敲拼音）
./target/release/phase0.exe ime &
powershell -ExecutionPolicy Bypass -File tools/ime-drive.ps1

# 空闲占用（先起 idle 场景，再采样 60s）
./target/release/phase0.exe idle &
powershell -ExecutionPolicy Bypass -File tools/measure-idle.ps1 -ProcName phase0 -Seconds 60

# 首帧耗时
powershell -ExecutionPolicy Bypass -File tools/first-paint.ps1 `
  -Exe target/release/phase0.exe -Args render -Rounds 4
```

内存归因用 `--no-aa` / `--no-fonts` / `--dx12` 三个开关组合。
