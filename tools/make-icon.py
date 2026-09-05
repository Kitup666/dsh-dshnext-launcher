# 生成 Dshnext 应用图标：以 assets/icons/artwork-src.png（少女抱鲸，作者已把
# 圆角外填黑）为源。处理：非黑区即卡片 -> 内缩躲开自带抗锯齿 -> 叠 4x 超采样
# 圆角遮罩（半径略小于卡自身，遮罩落在黑边内侧），角外全透明。
# 产物：master.png / app.ico / window-64.rgba。重跑：python tools/make-icon.py
import numpy as np
from PIL import Image, ImageDraw

MASK_RADIUS = 0.180  # 卡自身约 0.19，遮罩略小才不会露出黑角
INSET = 3

def card_bbox(im):
    # 圆角外是纯黑（作者处理过），非黑即卡
    a = np.array(im.convert("RGB")).astype(int)
    nb = (a > 40).any(axis=2)
    cols = nb.any(axis=0).nonzero()[0]
    rows = nb.any(axis=1).nonzero()[0]
    return int(cols.min()), int(rows.min()), int(cols.max()), int(rows.max())

def main():
    src = Image.open("assets/icons/artwork-src.png").convert("RGB")
    x0, y0, x1, y1 = card_bbox(src)
    x0, y0, x1, y1 = x0 + INSET, y0 + INSET, x1 - INSET, y1 - INSET
    w, h = x1 - x0, y1 - y0
    s = min(w, h)
    cx, cy = (x0 + x1) // 2, (y0 + y1) // 2
    half = s // 2
    card = src.crop((cx - half, cy - half, cx - half + s, cy - half + s))

    SS = 4
    mask = Image.new("L", (s * SS, s * SS), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        [0, 0, s * SS - 1, s * SS - 1], radius=int(s * MASK_RADIUS * SS), fill=255
    )
    mask = mask.resize((s, s), Image.LANCZOS)

    master = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    master.paste(card, (0, 0), mask)
    master.save("assets/icons/master.png")

    ico = master.resize((256, 256), Image.LANCZOS)
    ico.save("assets/icons/app.ico", sizes=[(16,16),(24,24),(32,32),(48,48),(64,64),(128,128),(256,256)])

    # 运行时窗口图标（iced::window::icon::from_rgba 只吃原始 RGBA），16 KB
    win = master.resize((64, 64), Image.LANCZOS)
    with open("assets/icons/window-64.rgba", "wb") as f:
        f.write(win.tobytes())
    print(f"card {s}x{s} from bbox {(x0,y0,x1,y1)} -> master/ico/window-64.rgba")

if __name__ == "__main__":
    main()
