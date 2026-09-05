#!/usr/bin/env python3
"""烘一张低alpha噪点瓦片，给「假高斯模糊」卡片当颗粒层。

用法固定种子，输出 assets/textures/grain.png（1024x688，2x2 张铺满
(0,0)-(2048,1376) 窗口逻辑坐标）。

颗粒场是**窗口锚定的共享贴图**（2026-09-06 用户定的架构）：所有磨砂卡
只当裁剪窗，采样同一片场——不再是每卡各自拉伸瓦片。拉伸倍数 1.0（瓦片
1:1 贴逻辑像素），颗粒保持 1px 级才「细」。

设计：每像素随机取纯白或纯黑，alpha 独立随机 0..A——白粒提亮、黑粒压暗，
叠在深色卡面上就是 ±几级 RGB 的细颗粒。A 不要大：卡面隔着 glass alpha 只
透一部分，太亮会变噪点海报。

**没有四边淡出带**（FEATHER 已删）：瓦片要拼接成场，边上有淡出带就是
每隔 1024/688px 一条暗缝。圆角处颗粒会露出卡外一丝（alpha≤18·0.15，
±2 级 RGB），实测不可见，不再按卡羽化。
"""

import random
from PIL import Image

W, H = 1024, 688
A = 18  # 峰值 alpha（/255），白黑两色共用
OUT = "assets/textures/grain.png"

random.seed(20260906)  # 可复现：同样的瓦片进 git，重新跑不变

px = []
for y in range(H):
    for x in range(W):
        v = 255 if random.random() < 0.5 else 0
        a = int(random.random() * random.random() * A)
        px.append((v, v, v, a))

img = Image.new("RGBA", (W, H))
img.putdata(px)
img.save(OUT, optimize=True)
print(f"wrote {OUT} ({W}x{H}, alpha<={A}, no feather)")


