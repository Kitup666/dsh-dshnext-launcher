# 生成 Dshnext 应用图标：DeepSeek 蓝鲸剪影 + 圆角渐变底。
# 产物：assets/icons/app.ico（exe 嵌入，多尺寸）+ master.png（1024 母版图）。
# 改设计就改下面的控制点，重跑：python tools/make-icon.py
import math
from PIL import Image, ImageDraw

S = 2048  # 4x 超采样后缩到 1024，边缘干净

def cubic(p0, p1, p2, p3, n=64):
    pts = []
    for i in range(n + 1):
        t = i / n
        mt = 1 - t
        x = mt**3*p0[0] + 3*mt**2*t*p1[0] + 3*mt*t**2*p2[0] + t**3*p3[0]
        y = mt**3*p0[1] + 3*mt**2*t*p1[1] + 3*mt*t**2*p2[1] + t**3*p3[1]
        pts.append((x, y))
    return pts

def whale_body():
    # 身体（不含尾鳍）：钝头、缓背、细尾柄、饱满腹部。
    segs = [
        ((0.120, 0.545), (0.122, 0.470), (0.150, 0.415), (0.215, 0.398)),
        ((0.215, 0.398), (0.360, 0.372), (0.520, 0.392), (0.640, 0.438)),
        ((0.640, 0.438), (0.700, 0.452), (0.740, 0.455), (0.775, 0.455)),
        # 尾柄端面（短竖线，叶从这里长出）
        ((0.775, 0.455), (0.780, 0.468), (0.780, 0.492), (0.775, 0.505)),
        ((0.775, 0.505), (0.740, 0.505), (0.700, 0.508), (0.640, 0.522)),
        ((0.640, 0.522), (0.470, 0.610), (0.300, 0.632), (0.190, 0.610)),
        ((0.190, 0.610), (0.150, 0.598), (0.128, 0.578), (0.120, 0.545)),
    ]
    pts = []
    for s in segs:
        pts.extend(cubic(*s))
    return pts

def whale_fluke(cx, cy, a, b, deg):
    # 一片尾叶：长轴 a、短轴 b 的椭圆绕中心旋转 deg。圆头靠椭圆天然给出。
    th = math.radians(deg)
    pts = []
    for i in range(73):
        t = i / 72 * 2 * math.pi
        x = a * math.cos(t)
        y = b * math.sin(t)
        pts.append((cx + x*math.cos(th) - y*math.sin(th),
                    cy + x*math.sin(th) + y*math.cos(th)))
    return pts

def main():
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    # 圆角方形底：对角渐变，从左上亮蓝到右下深蓝（与应用 accent #5b76ff 同族）
    top, bot = (0x7A, 0x88, 0xFF), (0x3A, 0x47, 0xC8)
    grad = Image.new("RGBA", (S, S))
    gd = ImageDraw.Draw(grad)
    for y in range(S):
        t = y / S
        # 对角：再混一点 x 方向
        gd.line([(0, y), (S, y)], fill=(
            int(top[0] + (bot[0]-top[0]) * t),
            int(top[1] + (bot[1]-top[1]) * t),
            int(top[2] + (bot[2]-top[2]) * t), 255))
    mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, S-1, S-1], radius=int(S*0.225), fill=255)
    img.paste(grad, (0, 0), mask)

    # 鲸鱼：近白，微偏蓝（纯白在小尺寸会「炸」出底）。身体 + 两片宽扁尾叶同色叠加。
    whale = (0xF2, 0xF5, 0xFF, 255)
    d.polygon([(x*S, y*S) for x, y in whale_body()], fill=whale)
    for cy, deg in ((0.415, -24), (0.545, 24)):
        d.polygon([(x*S, y*S) for x, y in whale_fluke(0.825, cy, 0.120, 0.052, deg)], fill=whale)

    # 眼睛：小而靠后（鲸鱼的眼在头部后下方，大眼是卡通鱼的特征）
    ex, ey, er = 0.235*S, 0.470*S, 0.013*S
    d.ellipse([ex-er, ey-er, ex+er, ey+er], fill=(0x2E, 0x3A, 0x9E, 255))

    master = img.resize((1024, 1024), Image.LANCZOS)
    master.save("assets/icons/master.png")

    sizes = [(16,16),(24,24),(32,32),(48,48),(64,64),(128,128),(256,256)]
    ico = master.resize((256, 256), Image.LANCZOS)
    ico.save("assets/icons/app.ico", sizes=sizes)

    # 运行时窗口图标（iced::window::icon::from_rgba 只吃原始 RGBA）：
    # include_bytes! 嵌入 64x64（任务栏小图标 16 的 4 倍，缩放后仍清晰），16 KB。
    win = master.resize((64, 64), Image.LANCZOS)
    with open("assets/icons/window-64.rgba", "wb") as f:
        f.write(win.tobytes())
    print("wrote assets/icons/master.png + app.ico + window-64.rgba")

if __name__ == "__main__":
    main()
