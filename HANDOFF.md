# 上下文压缩交接（2026-09-06）

给压缩后的下一个会话。当前所有工作已提交，工作树干净，测试 14/14 通过。

## 本轮做了什么（对应提交）

- `f645c6d` — **滚动盖顶 bug 修复 + 顶/底滚动渐隐帏幕**。
  - Bug 根因：iced 0.14 嵌套 `with_layer` 的 bounds **不与父裁剪求交**（`push_clip` 直接 push），scrollable 内的玻璃卡片传内容坐标全尺寸 bounds 就滚出裁剪层画到标题栏上。修法：frosted.rs 四处层边界一律传 `clipped = bounds.intersection(viewport)`。已记 AGENTS.md 坑 31。
  - 帏幕：glass.wgsl 新增 **mode 2**（不透明背景场 bg_app+光球 + 带高 alpha 渐隐），`GlassQuad` 从 `Option<CardGlass>` 改成 `Kind` 枚举 {Background, Card, Veil}，新 widget `glass_pipeline::fade_veil`，接在 pages/mod.rs 滚动分支的 `stack![scrollable, 顶帏, 底帏]` 里。
- `fbdc40f` — **窗口条与品牌块拆成独立栏位**。细窗口条 `TITLEBAR_H=40`（只管拖动+三键，横跨主区列上方），品牌头部单元 `BRAND_H=64` 归侧边栏自带（`titlebar::brand_cell`，自带拖动热区）。布局从「column[titlebar, row[sidebar, divider, main]]」改成「row[sidebar(含品牌), divider, column[titlebar, main]]」。顶帏 24→14px。
- 遗留资产变更（前几次会话的 PNG 重编码）已在 f645c6d 一并入库，渲染验证正常。

## 用户正在逐轮调的视觉参数（都是单值，改动成本低）

| 参数 | 位置 | 现值 | 备注 |
|---|---|---|---|
| 顶帏带宽 | `src/pages/mod.rs` scrollable 分支 `fade_veil(pal, 14.0, true)` | 14 | 用户逐轮要求收窄：44→24→14。必须**窄于**内容顶 padding 44（AGENTS.md 坑 32） |
| 底帏带宽 | 同上 `fade_veil(pal, 44.0, false)` | 44 | 用户没提过意见 |
| 窗口条高 | `src/ui/titlebar.rs` `TITLEBAR_H` | 40 | 用户嫌 64 宽 |
| 品牌块高 | `src/ui/titlebar.rs` `BRAND_H` | 64 | 独立于窗口条 |
| frosted 的 MAIN_ORIGIN | `src/ui/frosted.rs` | `(233, TITLEBAR_H)` | 引用常量自动跟随，别手动改 |

## 帏幕 shader 的内部约定（改之前必读）

glass.wgsl mode 2 复用了两个 uniform 字段扛方向数据：`grain_tile.x` = 带高（**物理 px**，CPU 侧已乘 scale），`grain_tile.y` = 方向旗（0=顶帏、1=底帏）；渐隐距离**从锚定边量**（顶帏量 uv.y、底帏量 1−uv.y），`alpha = 1 − smoothstep(0,1,d)`。第一版曾把底帏的 alpha 写成从顶部量的 smoothstep 本身，导致整页被盖黑——方向别再反。tiny-skia 回退是顶点 alpha 的两三角形 `Mesh::Solid`（没用渐变 quad API）。

## 下一步（用户明确的队列）

1. **NSIS 安装包重打**（packaging/）——用户说等视觉定稿再打；视觉这轮还在逐参数迭代，未定稿。打完必须真装真卸一遍 + 哈希比对包内 exe（AGENTS.md NSIS 节 + 记忆里有教训）。
2. 视觉参数定稿后可考虑更新记忆文件 `dshnext-frosted-grain-glass.md`（现还停在 shader 架构描述，缺帏幕和新布局）。

## 环境纪律（每轮都会踩）

- cargo 前先 `taskkill //F //IM dshnext.exe`，否则 os error 5；每条 cargo 命令前 `export no_proxy='*' NO_PROXY='*'`。
- 用户偏好小视觉改动**自己验收**，跳过 judge 出图循环（记忆 visual-tweaks-self-verify）；改动后我方出一张 `--shot` 截图自查即可。
- 出图命令：`./target/release/dshnext.exe --page settings --after 4500 --shot "$TEMP/x.png"`，读图用完整 Windows 路径 `C:/Users/24453/AppData/Local/Temp/x.png`。
