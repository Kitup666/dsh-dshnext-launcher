# 生成 Dshnext 应用图标：以 assets/icons/artwork-src.png 为源。
# 2026-09-07 起源图换为「环形徽章」版（蓝环 + 环内少女抱鲸 + 呆毛出环），作者已把
# 形状烘进 alpha（环外/环内背景全透明），所以不再走旧版的黑角抠圆角卡：
#   alpha bbox -> 补成正方形（内容居中）-> master.png
#   -> app.ico（16..256 七档）+ window-64.rgba（窗口/托盘）+ brand.png（侧栏 34px 显示）
# 重跑：python tools/make-icon.py
import numpy as np
from PIL import Image

ALPHA_THRESHOLD = 8  # 低于此值视为空

def content_bbox(alpha):
    op = alpha > ALPHA_THRESHOLD
    cols = op.any(axis=0).nonzero()[0]
    rows = op.any(axis=1).nonzero()[0]
    return int(cols.min()), int(rows.min()), int(cols.max()) + 1, int(rows.max()) + 1

def main():
    src = Image.open("assets/icons/artwork-src.png").convert("RGBA")
    a = np.array(src)[:, :, 3]
    x0, y0, x1, y1 = content_bbox(a)
    w, h = x1 - x0, y1 - y0
    s = max(w, h)  # 内容居中补成正方形
    cx, cy = (x0 + x1) // 2, (y0 + y1) // 2
    half = s // 2
    master = src.crop((cx - half, cy - half, cx - half + s, cy - half + s))
    master.save("assets/icons/master.png")

    ico = master.resize((256, 256), Image.LANCZOS)
    ico.save("assets/icons/app.ico", sizes=[(16,16),(24,24),(32,32),(48,48),(64,64),(128,128),(256,256)])

    # 运行时窗口/托盘图标（iced::window::icon::from_rgba 只吃原始 RGBA），16 KB
    win = master.resize((64, 64), Image.LANCZOS)
    with open("assets/icons/window-64.rgba", "wb") as f:
        f.write(win.tobytes())

    # 侧栏品牌头像（brand_cell 里 34 逻辑 px 显示）
    master.resize((136, 136), Image.LANCZOS).save("assets/icons/brand.png")
    print(f"content {w}x{h} from bbox {(x0,y0,x1,y1)} -> master {s}x{s} / ico / window-64.rgba / brand.png")

if __name__ == "__main__":
    main()
