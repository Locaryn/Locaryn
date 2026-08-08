// Temporary script: generate a proper 32x32 PNG + ICO for the Tauri app.
// Run: node scripts/gen-icons.js
// Delete after use.
const fs = require("node:fs");
const zlib = require("node:zlib");
const path = require("node:path");

const W = 32;
const H = 32;
const RAW_LEN = H * (1 + W * 3);
const raw = Buffer.alloc(RAW_LEN);
for (let y = 0; y < H; y++) {
  const off = y * (1 + W * 3);
  raw[off] = 0; // filter: none
  for (let x = 0; x < W; x++) {
    raw[off + 1 + x * 3] = 0x0e; // R
    raw[off + 1 + x * 3 + 1] = 0x11; // G
    raw[off + 1 + x * 3 + 2] = 0x16; // B
  }
}
const compressed = zlib.deflateSync(raw);

const crcTable = [];
for (let n = 0; n < 256; n++) {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  crcTable[n] = c;
}
function crc32(buf) {
  let crc = 0xffffffff;
  for (const b of buf) crc = crcTable[(crc ^ b) & 0xff] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const t = Buffer.from(type, "ascii");
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([t, data])), 0);
  return Buffer.concat([len, t, data, crc]);
}

const sig = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(W, 0);
ihdr.writeUInt32BE(H, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 2; // color type: RGB
ihdr[10] = 0;
ihdr[11] = 0;
ihdr[12] = 0;

const png = Buffer.concat([
  sig,
  chunk("IHDR", ihdr),
  chunk("IDAT", compressed),
  chunk("IEND", Buffer.alloc(0)),
]);

const iconDir = path.join(__dirname, "..", "apps", "desktop", "src-tauri", "icons");
fs.mkdirSync(iconDir, { recursive: true });
fs.writeFileSync(path.join(iconDir, "icon.png"), png);

// ICO: ICONDIR (6) + ICONDIRENTRY (16) + PNG payload
const icoHead = Buffer.alloc(22);
icoHead.writeUInt16LE(0, 0); // reserved
icoHead.writeUInt16LE(1, 2); // type: icon
icoHead.writeUInt16LE(1, 4); // count
icoHead[6] = W; // width
icoHead[7] = H; // height
icoHead[8] = 0; // colorCount
icoHead[9] = 0; // reserved
icoHead.writeUInt16LE(1, 10); // planes
icoHead.writeUInt16LE(32, 12); // bpp
icoHead.writeUInt32LE(png.length, 14); // imageSize
icoHead.writeUInt32LE(22, 18); // offset
const ico = Buffer.concat([icoHead, png]);
fs.writeFileSync(path.join(iconDir, "icon.ico"), ico);

console.log(`png: ${png.length} bytes, ico: ${ico.length} bytes`);
