// 生成 DshDesk 应用图标（纯 node，无依赖）：蓝色渐变圆角方块 + 白色 "D"
const zlib = require("zlib");
const fs = require("fs");
const path = require("path");

const S = 512;
const px = new Uint8Array(S * S * 4);

function clamp01(v) { return Math.max(0, Math.min(1, v)); }
// SDF 负=内部，coverage=1 内部 → 0 外部，边缘 feather 平滑
function cov(d, feather) { return clamp01(0.5 - d / feather); }
// 圆角矩形 SDF
function sdRoundRect(x, y, cx, cy, hw, hh, r) {
  const qx = Math.abs(x - cx) - (hw - r);
  const qy = Math.abs(y - cy) - (hh - r);
  const ax = Math.max(qx, 0), ay = Math.max(qy, 0);
  return Math.hypot(ax, ay) + Math.min(Math.max(qx, qy), 0) - r;
}

for (let y = 0; y < S; y++) {
  for (let x = 0; x < S; x++) {
    const i = (y * S + x) * 4;
    const t = y / S;
    // 背景渐变 #2F7CF6 → #1B5BD7，圆角方块
    const r0 = 0x2f + (0x1b - 0x2f) * t;
    const g0 = 0x7c + (0x5b - 0x7c) * t;
    const b0 = 0xf6 + (0xd7 - 0xf6) * t;
    const bgD = sdRoundRect(x + 0.5, y + 0.5, S / 2, S / 2, S / 2 - 8, S / 2 - 8, 96);
    const bgA = cov(bgD, 2);

    // 白色 "D" = 环 + 竖条（2x2 超采样）
    let gc = 0;
    for (const [dx, dy] of [[0.25, 0.25], [0.75, 0.25], [0.25, 0.75], [0.75, 0.75]]) {
      const sx = x + dx, sy = y + dy;
      const dist = Math.hypot(sx - 205, sy - 256);
      const ring = Math.abs(dist - 94) - 34;          // 环：中线半径94 半宽34
      const bar = sdRoundRect(sx, sy, 233, 256, 28, 128, 24);
      gc += cov(Math.min(ring, bar), 1.2);
    }
    gc /= 4;

    // white over bg
    const R = Math.round(r0 + (255 - r0) * gc);
    const G = Math.round(g0 + (255 - g0) * gc);
    const B = Math.round(b0 + (255 - b0) * gc);
    const A = Math.round(clamp01(Math.max(bgA, gc)) * 255);
    px[i] = R; px[i + 1] = G; px[i + 2] = B; px[i + 3] = A;
  }
}

// 自检采样
function g(x, y) { const i = (y * S + x) * 4; return [px[i], px[i + 1], px[i + 2], px[i + 3]]; }
const checks = [
  ["bg(50,50) 应为蓝色不透明", g(50, 50)],
  ["corner(480,10) 应透明", g(480, 10)],
  ["ring(85,256) 应白色", g(85, 256)],
  ["bar(233,256) 应白色", g(233, 256)],
  ["hole(205,256) 应蓝色(环内洞)", g(205, 256)],
];
let ok = true;
for (const [name, v] of checks) { console.log(name, "→", v); }

// PNG 编码
function crc32(buf) {
  let c, table = [];
  for (let n = 0; n < 256; n++) {
    c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c >>> 0;
  }
  let crc = 0xffffffff;
  for (const b of buf) crc = table[(crc ^ b) & 0xff] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}
const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(S, 0); ihdr.writeUInt32BE(S, 4);
ihdr[8] = 8; ihdr[9] = 6;
const raw = Buffer.alloc(S * (S * 4 + 1));
for (let y = 0; y < S; y++) {
  raw[y * (S * 4 + 1)] = 0;
  Buffer.from(px.buffer, y * S * 4, S * 4).copy(raw, y * (S * 4 + 1) + 1);
}
const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr),
  chunk("IDAT", zlib.deflateSync(raw, { level: 9 })),
  chunk("IEND", Buffer.alloc(0)),
]);
const out = path.join(__dirname, "icon.png");
fs.writeFileSync(out, png);
console.log("written", out, png.length, "bytes");
