// Run: bun Designed/branding/erpx-icons/uppercase/generate.mjs
// All four capital letters are vector geometry; logo files require no fonts.
import { mkdir, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";
import sharp from "sharp";

const root = path.dirname(fileURLToPath(import.meta.url));
const green = "#176B50";
const white = "#FFFFFF";
const ink = "#172D25";
const dark = "#141C17";
const muted = "#64746E";
const variants = [
  { id: "connect", name: "CONNECT", label: "เส้นมน เชื่อมต่อกัน", note: "เรียบ ชัด และเป็นมิตร", number: "01" },
  { id: "modules", name: "MODULES", label: "รูปทรงประกอบเป็นระบบ", note: "มีจังหวะของแต่ละส่วน", number: "02" },
  { id: "flow", name: "FLOW", label: "เส้นโค้งไหลต่อเนื่อง", note: "นุ่มนวลและมีความเคลื่อนไหว", number: "03" },
];
const xml = (value) => String(value).replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll('"', "&quot;");
const svg = (width, height, title, body) => `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" role="img" aria-labelledby="title"><title id="title">${xml(title)}</title>${body}</svg>\n`;
const rect = (x, y, width, height, fill, radius = 0, stroke = "none") => `<rect x="${x}" y="${y}" width="${width}" height="${height}" rx="${radius}" fill="${fill}" stroke="${stroke}"/>`;
const text = (x, y, value, size = 16, color = ink, weight = 500, more = "") => `<text x="${x}" y="${y}" font-family="Arial, Sukhumvit Set, Thonburi, sans-serif" font-size="${size}" font-weight="${weight}" fill="${color}" ${more}>${xml(value)}</text>`;
const thai = (x, y, value, size = 18, color = muted, weight = 500, more = "") => `<text x="${x}" y="${y}" font-family="Sukhumvit Set, Thonburi, sans-serif" font-size="${size}" font-weight="${weight}" fill="${color}" ${more}>${xml(value)}</text>`;

function letter(letter, style, color) {
  const stroke = (d) => `<path d="${d}" fill="none" stroke="${color}" stroke-width="32" stroke-linecap="${style === "modules" ? "square" : "round"}" stroke-linejoin="round"/>`;
  if (letter === "E") {
    if (style === "modules") {
      return `<path d="M0 0H160V32H40V48H0ZM0 64H128V96H40V112H0ZM0 128H160V160H0Z" fill="${color}"/>`;
    }
    if (style === "flow") return stroke("M144 16H80C40 16 16 40 16 80S40 144 80 144H144M16 80H124");
    return stroke("M144 16H16V144H144M16 80H120");
  }
  if (letter === "R") {
    if (style === "modules") {
      return stroke("M16 144V16H88C124 16 144 28 144 48S124 80 88 80H16") + `<path d="M76 80H116L160 144L144 160Z" fill="${color}"/>`;
    }
    const leg = style === "flow" ? "M88 80C112 100 132 120 144 144" : "M88 80L140 140";
    return stroke(`M16 144V16H88C124 16 144 28 144 48S124 80 88 80H16${leg}`);
  }
  if (letter === "P") return stroke("M16 144V16H88C124 16 144 28 144 48S124 80 88 80H16");
  if (style === "flow") return stroke("M16 16C52 68 108 92 144 144M144 16C108 68 52 92 16 144");
  if (style === "modules") return stroke("M24 24L136 136M136 24L24 136");
  return stroke("M16 16L144 144M144 16L16 144");
}

function lettering(variant, color, square = false) {
  return [..."ERPX"].map((character, i) => {
    const x = square ? 64 + (i % 2) * 224 : i * 208;
    const y = square ? 64 + Math.floor(i / 2) * 224 : 0;
    return `<g transform="translate(${x} ${y})">${letter(character, variant.id, color)}</g>`;
  }).join("");
}

function tileBody(variant) {
  return rect(0, 0, 512, 512, green, 112) + lettering(variant, white, true);
}

function placedTile(variant, x, y, size) {
  return `<svg x="${x}" y="${y}" width="${size}" height="${size}" viewBox="0 0 512 512">${tileBody(variant)}</svg>`;
}

function placedWord(variant, x, y, width, color = green) {
  return `<svg x="${x}" y="${y}" width="${width}" height="${width * 160 / 784}" viewBox="-2 -2 788 164">${lettering(variant, color)}</svg>`;
}

function pixels(variant, x, y, size) {
  return `<image x="${x}" y="${y}" width="${size}" height="${size}" href="data:image/png;base64,${variant.pngs.get(size).toString("base64")}"/>`;
}

// PNG-compressed ICO frames preserve the independently rendered small glyphs.
function ico(pngs) {
  const frames = [16, 32, 48].map((size) => ({ size, image: pngs.get(size) }));
  const directory = Buffer.alloc(6 + frames.length * 16);
  directory.writeUInt16LE(1, 2);
  directory.writeUInt16LE(frames.length, 4);
  let offset = directory.length;
  frames.forEach(({ size, image }, i) => {
    const entry = 6 + i * 16;
    directory[entry] = directory[entry + 1] = size;
    directory.writeUInt16LE(1, entry + 4);
    directory.writeUInt16LE(32, entry + 6);
    directory.writeUInt32LE(image.length, entry + 8);
    directory.writeUInt32LE(offset, entry + 12);
    offset += image.length;
  });
  return Buffer.concat([directory, ...frames.map((frame) => frame.image)]);
}

for (const variant of variants) {
  const folder = path.join(root, variant.id);
  await mkdir(folder, { recursive: true });
  variant.pngs = new Map();
  await writeFile(path.join(folder, "icon.svg"), svg(512, 512, `ERPX ${variant.name} — uppercase icon`, tileBody(variant)));
  for (const size of [16, 32, 48, 512]) {
    const image = await sharp(Buffer.from(svg(size, size, `ERPX ${variant.name}`, `<g transform="scale(${size / 512})">${tileBody(variant)}</g>`))).png().toBuffer();
    variant.pngs.set(size, image);
    await writeFile(path.join(folder, `icon-${size}.png`), image);
  }
  await writeFile(path.join(folder, "favicon.ico"), ico(variant.pngs));
  for (const [suffix, color] of [["", green], ["-white", white]]) {
    const wordmark = svg(1024, 256, `ERPX ${variant.name} — uppercase wordmark`, `<g transform="translate(120 48)">${lettering(variant, color)}</g>`);
    await writeFile(path.join(folder, `wordmark${suffix}.svg`), wordmark);
    await sharp(Buffer.from(wordmark)).png().toFile(path.join(folder, `wordmark${suffix}.png`));
    const mark = svg(512, 512, `ERPX ${variant.name} — uppercase transparent mark`, lettering(variant, color, true));
    await writeFile(path.join(folder, `mark${suffix}.svg`), mark);
    await sharp(Buffer.from(mark)).png().toFile(path.join(folder, `mark${suffix}-512.png`));
  }
  variant.enlarged = await sharp(variant.pngs.get(16)).resize(128, 128, { kernel: "nearest" }).png().toBuffer();
}

function comparison() {
  let body = rect(0, 0, 1400, 1280, "#F6F8F6");
  body += text(48, 52, "ERPX / UPPERCASE IDENTITY", 12, green, 700, 'letter-spacing="2"');
  body += thai(48, 116, "ERPX เต็มคำ ตัวพิมพ์ใหญ่ทั้งหมด", 40, ink, 600);
  body += thai(48, 160, "โลโก้แนวนอน ERPX · ไอคอนสองบรรทัด ER / PX · ไม่มีจุดท้ายชื่อ", 19, muted);
  variants.forEach((variant, i) => {
    const y = 204 + i * 332;
    body += rect(48, y, 1304, 304, white, 24, "#DFE7E1");
    body += text(80, y + 44, variant.number, 13, green, 700, 'letter-spacing="1.5"');
    body += text(80, y + 85, variant.name, 23, ink, 700, 'letter-spacing=".8"');
    body += thai(80, y + 127, variant.label, 18, muted);
    body += thai(80, y + 155, variant.note, 16, muted);
    body += placedWord(variant, 350, y + 58, 590);
    body += rect(336, y + 204, 620, 68, dark, 12);
    body += placedWord(variant, 520, y + 212, 254, white);
    body += placedTile(variant, 1114, y + 32, 160);
    [16, 32, 48].forEach((size, j) => {
      const cx = 1080 + j * 108;
      body += pixels(variant, cx - size / 2, y + 235 - size / 2, size);
      body += text(cx, y + 281, `${size} px`, 12, muted, 400, 'text-anchor="middle"');
    });
  });
  body += thai(48, 1246, "เขียว ERPX #176B50 · ตัวอักษรวาดเป็นเวกเตอร์ · SVG / PNG / ICO", 17, muted);
  body += text(1352, 1246, "DESIGN STUDY 02", 12, muted, 500, 'text-anchor="end" letter-spacing="1.4"');
  return svg(1400, 1280, "ERPX — เปรียบเทียบโลโก้และไอคอนตัวพิมพ์ใหญ่เต็มคำ", body);
}

function sizeCheck() {
  let body = rect(0, 0, 1200, 714, "#F6F8F6");
  body += thai(36, 66, "ERPX — ตรวจขนาดจริงของ favicon", 34, ink, 600);
  body += thai(36, 106, "แถวบน: 16 / 32 / 48 px · แถวล่าง: ภาพขยาย 16 px แบบเห็นพิกเซล", 18, muted);
  variants.forEach((variant, i) => {
    const x = 36 + i * 388;
    body += text(x, 164, variant.name, 22, ink, 700);
    [false, true].forEach((isDark, row) => {
      const y = 190 + row * 128;
      body += rect(x, y, 352, 108, isDark ? dark : white, 16);
      [16, 32, 48].forEach((size, j) => {
        const cx = x + 60 + j * 112;
        body += pixels(variant, cx - size / 2, y + 40 - size / 2, size);
        body += text(cx, y + 89, `${size} px`, 12, isDark ? "#A0B1A7" : muted, 400, 'text-anchor="middle"');
      });
    });
    body += text(x, 470, "16 PX / 8× PIXEL VIEW", 12, muted, 600, 'letter-spacing="1.4"');
    body += `<image x="${x}" y="491" width="128" height="128" href="data:image/png;base64,${variant.enlarged.toString("base64")}"/>`;
    body += rect(x + 184, 491, 128, 128, dark, 16);
    body += `<image x="${x + 184}" y="491" width="128" height="128" href="data:image/png;base64,${variant.enlarged.toString("base64")}"/>`;
  });
  body += thai(36, 680, "เปิดภาพที่ 100% เพื่อดูขนาดจริง · ไอคอนทุกแบบประกอบด้วย E, R, P และ X ตัวพิมพ์ใหญ่", 17, muted);
  return svg(1200, 714, "ERPX — uppercase favicon size check", body);
}

function inContext() {
  let body = rect(0, 0, 1400, 1040, "#F6F8F6");
  body += text(48, 52, "ERPX / UPPERCASE IN CONTEXT", 12, green, 700, 'letter-spacing="1.7"');
  body += thai(48, 114, "ERPX ในพื้นที่ทำงาน", 42, ink, 600);
  body += thai(48, 156, "ภาพจำลองโลโก้เต็มคำและ favicon บนพื้นสว่างและมืด", 20, muted);
  variants.forEach((variant, i) => {
    const x = 48 + i * 444;
    body += text(x, 228, `${variant.number}  ${variant.name}`, 23, ink, 700);
    [false, true].forEach((isDark, row) => {
      const y = 260 + row * 324;
      const base = isDark ? "#1B261F" : white;
      const chrome = isDark ? "#303E36" : "#E2E8E4";
      const foreground = isDark ? white : ink;
      body += rect(x, y, 416, 278, base, 16);
      body += rect(x, y, 416, 50, chrome, 16);
      body += rect(x, y + 25, 416, 25, chrome);
      body += rect(x + 12, y + 12, 278, 38, base, 9);
      body += rect(x + 12, y + 38, 278, 12, base);
      body += pixels(variant, x + 26, y + 24, 16);
      body += text(x + 52, y + 37, "ERPX · Work connected", 12, foreground);
      body += text(x + 266, y + 37, "×", 16, isDark ? "#A0B1A7" : muted);
      body += text(x + 310, y + 37, "+", 19, foreground);
      body += rect(x, y + 50, 416, 228, isDark ? dark : "#E8EFE7", 16);
      body += rect(x, y + 50, 416, 20, isDark ? dark : "#E8EFE7");
      body += placedTile(variant, x + 28, y + 82, 33);
      body += placedWord(variant, x + 74, y + 89, 94, isDark ? white : green);
      body += text(x + 28, y + 174, "Good work.", 33, foreground, 500, 'letter-spacing="-1"');
      body += text(x + 28, y + 215, "Greater clarity.", 33, foreground, 500, 'letter-spacing="-1"');
      body += text(x + 28, y + 249, "Your workspace. Your people. Connected.", 11, isDark ? "#A0B1A7" : muted);
    });
    body += rect(x, 910, 416, 70, white, 14, "#E2E8E4");
    body += placedTile(variant, x + 24, 930, 31);
    body += placedWord(variant, x + 66, 937, 83, green);
    body += text(x + 386, 951, "SIDEBAR / 31 PX", 10, muted, 500, 'text-anchor="end"');
  });
  body += thai(48, 1020, "favicon 16 px · ไอคอนหน้าล็อกอิน 33 px · ไอคอน sidebar 31 px", 16, muted);
  return svg(1400, 1040, "ERPX — uppercase logo and favicon in light and dark interfaces", body);
}

for (const [filename, source] of [["comparison", comparison()], ["size-check", sizeCheck()], ["in-context", inContext()]]) {
  await writeFile(path.join(root, `${filename}.svg`), source);
  await sharp(Buffer.from(source)).png().toFile(path.join(root, `${filename}.png`));
}
console.log("Exported 3 uppercase ERPX logo/icon families and 3 review boards.");
