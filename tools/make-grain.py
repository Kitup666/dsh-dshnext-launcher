#!/usr/bin/env python3
"""烘一张低alpha噪点瓦片，给「假高斯模糊」卡片当颗粒层。

用法固定种子，输出 assets/textures/grain.png（640x430 = 默认窗口逻辑尺寸
1280x860 的一半，iced 里拉伸 2x，线性过滤后正好是柔和的磨砂颗粒）。

设计：每像素随机取纯白或纯黑，alpha 独立随机 0..A——白粒提亮、黑粒压暗，
叠在深色卡面上就是 ±几级 RGB 的细颗粒。A 不要大：背景层全局铺，卡面隔
着 glass alpha 只透一部分，太亮会变噪点海报。
"""

import random
from PIL import Image

W, H = 640, 430
A = 14  # 峰值 alpha（/255），白黑两色共用
OUT = "assets/textures/grain.png"

random.seed(20260906)  # 可复现：同样的瓦片进 git，重新跑不变

px = []
for _ in range(W * H):
    v = 255 if random.random() < 0.5 else 0
    a = int(random.random() * random.random() * A)  # 平方分布：多数像素接近 0
    px.append((v, v, v, a))

img = Image.new("RGBA", (W, H))
img.putdata(px)
img.save(OUT, optimize=True)
print(f"wrote {OUT} ({W}x{H}, alpha<={A})")
