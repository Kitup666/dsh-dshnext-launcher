// 给 PowerShell 脚本加 UTF-8 BOM：Windows PowerShell 无 BOM 时按 ANSI 读取，
// 脚本里的中文字面量会变成乱码，导致按名字匹配 UI 元素全部失败。
const fs = require("fs");
const path = require("path");

const files = process.argv.slice(2);
if (files.length === 0) {
  console.error("usage: node add-bom.cjs <file.ps1> [...]");
  process.exit(1);
}
for (const f of files) {
  const p = path.resolve(f);
  const buf = fs.readFileSync(p);
  if (buf[0] === 0xef && buf[1] === 0xbb && buf[2] === 0xbf) {
    console.log(`already has BOM: ${p}`);
    continue;
  }
  fs.writeFileSync(p, Buffer.concat([Buffer.from([0xef, 0xbb, 0xbf]), buf]));
  console.log(`BOM added: ${p}`);
}
