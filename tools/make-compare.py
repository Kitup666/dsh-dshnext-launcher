#!/usr/bin/env python3
"""生成「上一代 Tauri（左）× Dshnext（右）」并排对比图到 shots/compare-<代>/。

用法：python tools/make-compare.py [--gen p5] [页面名...]   不带页面名则六页全出。
输入：shots/tauri-pages/0N-<name>.png 与 shots/<代>-<name>-dark.png
      （后者用 `dshnext --shot --page <name> --theme dark` 现出）
CJK 标签靠系统 msyh.ttc；面板按高度 760 等比缩放，深色底拼一张。
"""
import sys, os
from PIL import Image, ImageDraw, ImageFont

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SHOTS = os.path.join(ROOT, "shots")
PANEL_H = 760
LABELBAR = 46
PAD, GAP = 24, 24
BG = (24, 24, 26)
FONT = "C:/Windows/Fonts/msyh.ttc"
# tauri-pages 的文件名前缀编号 → 页面名
PAGES = {"home": "01", "profiles": "02", "plugins": "03",
         "env": "04", "console": "05", "settings": "06"}


def scale(im):
    w = round(im.width * PANEL_H / im.height)
    return im.resize((w, PANEL_H), Image.LANCZOS)


def compose(name, gen):
    out_dir = os.path.join(SHOTS, "compare-" + gen)
    tauri = Image.open(os.path.join(SHOTS, "tauri-pages", f"{PAGES[name]}-{name}.png")).convert("RGB")
    dsh = Image.open(os.path.join(SHOTS, f"{gen}-{name}-dark.png")).convert("RGB")
    t, d = scale(tauri), scale(dsh)
    W = PAD + t.width + GAP + d.width + PAD
    H = LABELBAR + PANEL_H + PAD
    canvas = Image.new("RGB", (W, H), BG)
    dr = ImageDraw.Draw(canvas)
    f = ImageFont.truetype(FONT, 22)
    dr.text((PAD, 12), "上一代 Tauri（WebView2）", font=f, fill=(200, 200, 205))
    dr.text((PAD + t.width + GAP, 12), "Dshnext（iced 原生）", font=f, fill=(120, 150, 255))
    canvas.paste(t, (PAD, LABELBAR))
    canvas.paste(d, (PAD + t.width + GAP, LABELBAR))
    os.makedirs(out_dir, exist_ok=True)
    out = os.path.join(out_dir, f"{name}.png")
    canvas.save(out)
    print("wrote", out, canvas.size)


if __name__ == "__main__":
    args = sys.argv[1:]
    gen = "p5"
    if "--gen" in args:
        i = args.index("--gen")
        gen = args[i + 1]
        del args[i:i + 2]
    for n in args or list(PAGES):
        if n not in PAGES:
            print("未知页面", n); continue
        compose(n, gen)
