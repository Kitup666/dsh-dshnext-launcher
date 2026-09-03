// 从截图里量出侧边栏各导航项的中心 y（CSS 像素），供截图脚本使用。
// 原理：高亮项与普通项的亮度差明显，先定位高亮带，再按 CSS 行距推算其余项。
const fs = require("fs");
const zlib = require("zlib");

function readPng(path) {
  const buf = fs.readFileSync(path);
  let off = 8, w = 0, h = 0, bit = 0, color = 0;
  const idat = [];
  while (off < buf.length) {
    const len = buf.readUInt32BE(off);
    const type = buf.toString("ascii", off + 4, off + 8);
    if (type === "IHDR") {
      w = buf.readUInt32BE(off + 8);
      h = buf.readUInt32BE(off + 12);
      bit = buf[off + 16];
      color = buf[off + 17];
    } else if (type === "IDAT") idat.push(buf.slice(off + 8, off + 8 + len));
    off += 12 + len;
  }
  if (bit !== 8 || (color !== 6 && color !== 2)) {
    throw new Error(`unsupported png: bit=${bit} color=${color}`);
  }
  const ch = color === 6 ? 4 : 3;
  const raw = zlib.inflateSync(Buffer.concat(idat));
  const stride = w * ch;
  const px = Buffer.alloc(h * stride);
  // 反 PNG 滤波
  for (let y = 0; y < h; y++) {
    const ft = raw[y * (stride + 1)];
    const src = raw.slice(y * (stride + 1) + 1, y * (stride + 1) + 1 + stride);
    const cur = px.slice(y * stride, (y + 1) * stride);
    const prev = y > 0 ? px.slice((y - 1) * stride, y * stride) : Buffer.alloc(stride);
    for (let x = 0; x < stride; x++) {
      const a = x >= ch ? cur[x - ch] : 0;
      const b = prev[x];
      const c = x >= ch ? prev[x - ch] : 0;
      let v = src[x];
      if (ft === 1) v += a;
      else if (ft === 2) v += b;
      else if (ft === 3) v += (a + b) >> 1;
      else if (ft === 4) {
        const p = a + b - c;
        const pa = Math.abs(p - a), pb = Math.abs(p - b), pc = Math.abs(p - c);
        v += pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      }
      cur[x] = v & 0xff;
    }
  }
  return { w, h, ch, stride, px };
}

const file = process.argv[2];
const scale = Number(process.argv[3] || 1.25);
const img = readPng(file);

// 侧边栏导航区：CSS x 30..270，y 60..420
const x0 = Math.round(30 * scale), x1 = Math.round(270 * scale);
const y0 = Math.round(60 * scale), y1 = Math.min(img.h - 1, Math.round(430 * scale));

const rows = [];
for (let y = y0; y <= y1; y++) {
  let sum = 0, n = 0;
  for (let x = x0; x <= x1; x++) {
    const o = y * img.stride + x * img.ch;
    sum += (img.px[o] + img.px[o + 1] + img.px[o + 2]) / 3;
    n++;
  }
  rows.push({ y, lum: sum / n });
}
const lums = rows.map((r) => r.lum).slice().sort((a, b) => a - b);
const base = lums[Math.floor(lums.length * 0.35)];
const peak = lums[lums.length - 1];
const thr = base + (peak - base) * 0.45;

// 找最长的连续高亮带 = 当前激活项的胶囊
let best = null, run = null;
for (const r of rows) {
  if (r.lum >= thr) {
    run = run ?? { a: r.y, b: r.y };
    run.b = r.y;
  } else if (run) {
    if (!best || run.b - run.a > best.b - best.a) best = run;
    run = null;
  }
}
if (run && (!best || run.b - run.a > best.b - best.a)) best = run;
if (!best) {
  console.error("未找到高亮导航项");
  process.exit(1);
}

const activeCenterCss = (best.a + best.b) / 2 / scale;
// nav-item 高度：padding 9*2 + border 1*2 + line-height(0.9rem*1.6≈20.2) ≈ 40.2 CSS px
const pitch = 40.2;
const centers = [];
for (let i = 0; i < 6; i++) centers.push(Math.round(activeCenterCss + i * pitch));

console.log(
  JSON.stringify(
    {
      file,
      scale,
      activeBandDevice: [best.a, best.b],
      activeCenterCss: Math.round(activeCenterCss),
      pitch,
      centersCss: centers,
    },
    null,
    2
  )
);
