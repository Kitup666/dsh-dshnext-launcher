#!/usr/bin/env python3
"""烘一张低alpha噪点瓦片，给「假高斯模糊」卡片当颗粒层。

用法固定种子，输出 assets/textures/grain.png（640x430 = 默认窗口逻辑尺寸
1280x860 的一半，iced 里拉伸 2x，线性过滤后正好是柔和的磨砂颗粒）。

设计：每像素随机取纯白或纯黑，alpha 独立随机 0..A——白粒提亮、黑粒压暗，
叠在深色卡面上就是 ±几级 RGB 的细颗粒。A 不要大：卡面隔着 glass alpha 只
透一部分，太亮会变噪点海报。

瓦片四边有 FEATHER 像素的 alpha 淡出带：卡片铺瓦片是整面铺，圆角是容器
裁的——没有淡出带的话颗粒会顶到矩形边上，从圆角外一丝丝「漏」出去。
拉伸 2x 后淡出带约 12px（逻辑），比 16px 圆角短，藏在卡内，安全。
"""

import random
from PIL import Image

W, H = 640, 430
A = 18  # 峰值 alpha（/255），白黑两色共用
FEATHER = 12  # 四边淡出带宽度（像素，瓦片坐标系；2x 显示后 ≈24 物理）
OUT = "assets/textures/grain.png"

random.seed(20260906)  # 可复现：同样的瓦片进 git，重新跑不变

def falloff(x, y):
    """到最近边的距离 → 0..1 淡出系数（feather 带内线性升到 1）。"""
    d = min(x, y, W - 1 - x, H - 1 - y)
    return min(1.0, d / FEATHER)

px = []
for y in range(H):
    for x in range(W):
        v = 255 if random.random() < 0.5 else 0
        a = int(random.random() * random.random() * A * falloff(x, y))
        px.append((v, v, v, a))

img = Image.new("RGBA", (W, H))
img.putdata(px)
img.save(OUT, optimize=True)
print(f"wrote {OUT} ({W}x{H}, alpha<={A}, feather={FEATHER}px)")

