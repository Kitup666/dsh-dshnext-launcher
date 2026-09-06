"""Phase 0 font tool: subset + instance NotoSansSC / Cascadia Mono for embedding.

Answers two DESIGN.md questions with real numbers:
  1. how big is an embeddable CJK subset (design claimed 1-2 MB; answer: 4.34 MB)
  2. do the digits stay tabular after subsetting (yes, and it matters -- iced
     never sets cosmic-text font_features, so `tnum` is unreachable and tabular
     digits can only come from a font that is natively monospaced-digit)

Coverage: ASCII + Latin-1 + CJK punctuation + kana + GB2312 (6763 hanzi). GB2312
is the floor for a Chinese UI: any name a user types, any log line dsh prints.
Rarer hanzi fall back to the system Microsoft YaHei UI via cosmic-text's
per-script fallback table, which is why full GBK (14 MB) is not worth embedding.

Source font is not in git (17 MB). Fetch it with:
  curl -L -o tools/NotoSansSC-var.ttf     "https://gcore.jsdelivr.net/gh/google/fonts@main/ofl/notosanssc/NotoSansSC%5Bwght%5D.ttf"

Requires: pip install fonttools brotli
"""

import codecs
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).parent
OUT = HERE.parent.parent / "assets" / "fonts"
OUT.mkdir(parents=True, exist_ok=True)


def gb2312_codepoints() -> set[int]:
    """Every char GB2312 can encode, as Unicode codepoints."""
    cps = set()
    for hi in range(0xA1, 0xFA):
        for lo in range(0xA1, 0xFF):
            try:
                ch = bytes([hi, lo]).decode("gb2312")
            except UnicodeDecodeError:
                continue
            cps.add(ord(ch))
    return cps


def base_codepoints() -> set[int]:
    cps = set(range(0x20, 0x7F))                       # ASCII printable
    cps |= set(range(0xA0, 0x100))                     # Latin-1 supplement
    cps |= set(range(0x2000, 0x2070))                  # general punctuation
    cps |= set(range(0x3000, 0x3040))                  # CJK punctuation
    cps |= set(range(0x3040, 0x3100))                  # hiragana + katakana
    cps |= set(range(0xFF00, 0xFF61))                  # fullwidth forms
    cps |= {0x2026, 0x2192, 0x2713, 0x2717, 0x25CF, 0x25A0, 0x00B7}
    return cps


def cjk_ext_codepoints() -> set[int]:
    """Beyond GB2312: GBK-only hanzi and CJK Ext-A.

    dsh writes profile names and log lines verbatim, so a user typing a name
    outside GB2312 must not produce tofu. GBK covers 21886 hanzi; the extra
    ~15k glyphs cost about 3 MB, which is the price of never seeing a box.
    """
    cps = set()
    for hi in range(0x81, 0xFF):
        for lo in range(0x40, 0xFF):
            if lo == 0x7F:
                continue
            try:
                ch = bytes([hi, lo]).decode("gbk")
            except UnicodeDecodeError:
                continue
            cps.add(ord(ch))
    return cps


def run(args: list[str]) -> None:
    print("  $", " ".join(args[:3]), "...")
    subprocess.run(args, check=True, stdout=subprocess.DEVNULL)


def instantiate(src: Path, axes: str) -> Path:
    """Pin a variable font to one static weight. Separate step from subsetting.

    --update-name-table matters: without it every instance keeps the variable
    font's default name ("Noto Sans SC Thin"), so Font::with_name can't tell
    Regular from SemiBold.
    """
    return instance(src, axes.split(","), HERE / f"_static_{axes.replace('=', '')}_{src.stem}.ttf")


def instance(src: Path, axes: list[str], dst: Path) -> Path:
    """instancer 要把每个轴单独传一个 AXIS=LOC 参数（"wght=400,wdth=112.5" 单串
    会被当成一个轴的值解析失败）。"""
    run([
        sys.executable, "-m", "fontTools.varLib.instancer", str(src), *axes,
        "--update-name-table",
        "--output", str(dst),
    ])
    return dst


def subset(src: Path, dst: Path, cps: set[int], instance: str | None) -> None:
    if instance:
        src = instantiate(src, instance)
    unicodes = ",".join(f"U+{c:04X}" for c in sorted(cps))
    listfile = HERE / "_unicodes.txt"
    listfile.write_text(unicodes, encoding="ascii")
    run([
        sys.executable, "-m", "fontTools.subset", str(src),
        f"--unicodes-file={listfile}",
        f"--output-file={dst}",
        "--layout-features=locl,ccmp,liga,calt,tnum,zero,kern,mark,mkmk",
        "--no-hinting",
        "--desubroutinize",
        "--name-IDs=1,2,3,4,6",
        "--drop-tables+=vhea,vmtx,VORG",
    ])
    listfile.unlink(missing_ok=True)
    if instance:
        src.unlink(missing_ok=True)


def set_family(path: Path, family: str, subfamily: str) -> None:
    """Force one typographic family across weights.

    fontdb keys faces by name ID 16 (typographic family), falling back to ID 1.
    varLib.instancer writes ID 1 = "Noto Sans SC SemiBold", which registers the
    bold face as a *separate family* -- Font::with_name("Noto Sans SC") then
    silently resolves to the system font instead. Writing ID 16/17 puts both
    weights in one family so weight selection works.
    """
    from fontTools.ttLib import TTFont
    f = TTFont(path)
    name = f["name"]
    full = family if subfamily == "Regular" else f"{family} {subfamily}"
    ps = full.replace(" ", "")
    for nid, value in ((1, family), (2, subfamily), (4, full), (6, ps),
                       (16, family), (17, subfamily)):
        name.setName(value, nid, 3, 1, 0x409)
    f.save(path)


def report(path: Path) -> None:
    from fontTools.ttLib import TTFont
    f = TTFont(path)
    cmap = f.getBestCmap()
    hmtx = f["hmtx"]
    widths = {hmtx[cmap[ord(d)]][0] for d in "0123456789" if ord(d) in cmap}
    tags = set()
    if "GSUB" in f:
        tags = {r.FeatureTag for r in f["GSUB"].table.FeatureList.FeatureRecord}
    print(
        f"  {path.name}: {path.stat().st_size / 1024:.0f} KB, "
        f"{f['maxp'].numGlyphs} glyphs, {len(cmap)} cmap, "
        f"digit widths={sorted(widths)} "
        f"({'tabular already' if len(widths) == 1 else 'PROPORTIONAL - needs tnum'}), "
        f"feats={sorted(tags)}"
    )
    ns = {r.nameID: r.toUnicode() for r in f["name"].names if r.platformID == 3}
    print(f"      family(ID16)={ns.get(16)!r} subfamily(ID17)={ns.get(17)!r} "
          f"weight={f['OS/2'].usWeightClass}")


def main() -> None:
    # GB2312 (6763 hanzi) is the embedded floor. Anything rarer falls back to the
    # system Microsoft YaHei UI through cosmic-text's per-script fallback table,
    # which is why we do not pay 14 MB for full GBK coverage.
    cps = base_codepoints() | gb2312_codepoints()
    print(f"target coverage: {len(cps)} codepoints (GB2312 + latin + kana + punct)")

    sans = HERE / "NotoSansSC-var.ttf"
    print("subsetting sans (wght=400 / 600 static instances)")
    reg = OUT / "NotoSansSC-Regular.subset.ttf"
    semi = OUT / "NotoSansSC-SemiBold.subset.ttf"
    subset(sans, reg, cps, "wght=400")
    subset(sans, semi, cps, "wght=600")
    set_family(reg, "Noto Sans SC", "Regular")
    set_family(semi, "Noto Sans SC", "SemiBold")

    mono_src = Path(r"C:\Windows\Fonts\CascadiaMono.ttf")
    print("subsetting mono (latin only; CJK falls back to sans)")
    mono = OUT / "CascadiaMono.subset.ttf"
    subset(mono_src, mono, base_codepoints(), "wght=400")
    set_family(mono, "Cascadia Mono", "Regular")

    # Display 层（2026-09-07）：Martian Mono SemiExpanded——品牌名 / hero /
    # overline / 统计数字的「机器声」（src/ui/mod.rs FONT_DISPLAY）。
    # 变体源字体不入库，下载（gcore.jsdelivr 直连，同上面 Noto 的路子）：
    #   curl -L -o tools/MartianMono-var.ttf "https://gcore.jsdelivr.net/gh/google/fonts@main/ofl/martianmono/MartianMono%5Bwdth,wght%5D.ttf"
    mm_src = HERE / "MartianMono-var.ttf"
    if mm_src.exists():
        print("subsetting display Martian Mono (latin only; CJK falls back to sans)")
        mm_reg = OUT / "MartianMono-Regular.subset.ttf"
        mm_bold = OUT / "MartianMono-Bold.subset.ttf"
        # wdth 钉在 112.5（SemiExpanded，它的签名宽度）；只有 ASCII，
        # 中文字符由 cosmic-text 按脚本回落 Noto。
        subset(instance(mm_src, ["wght=400", "wdth=112.5"], HERE / "_mm_400.ttf"),
               mm_reg, base_codepoints(), None)
        subset(instance(mm_src, ["wght=700", "wdth=112.5"], HERE / "_mm_700.ttf"),
               mm_bold, base_codepoints(), None)
        set_family(mm_reg, "Martian Mono", "Regular")
        set_family(mm_bold, "Martian Mono", "Bold")
        (HERE / "_mm_400.ttf").unlink(missing_ok=True)
        (HERE / "_mm_700.ttf").unlink(missing_ok=True)
    else:
        print("MartianMono-var.ttf 不在，跳过 display 层（src/ui/mod.rs 会编译失败）")

    print("results:")
    for p in sorted(OUT.glob("*.ttf")):
        report(p)
    total = sum(p.stat().st_size for p in OUT.glob("*.ttf"))
    print(f"  TOTAL embedded: {total / 1024 / 1024:.2f} MB")


if __name__ == "__main__":
    main()
