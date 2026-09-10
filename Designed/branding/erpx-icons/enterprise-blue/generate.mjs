// Rebuild the blue identity studies from mark.svg and wordmark.svg masters.
// bun Designed/branding/erpx-icons/enterprise-blue/generate.mjs
import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";
import sharp from "sharp";

const root = path.dirname(fileURLToPath(import.meta.url));
const blue = "#1E4FCC";
const navy = "#102A56";
const accent = "#4A7FE8";
const dark = "#0D1930";
const muted = "#627089";
const paper = "#F3F6FC";
const white = "#FFFFFF";
const concepts = [
  {
    id: "one-core", name: "ONE CORE", number: "01", meaning: "จุดร่วมของทั้งองค์กร",
    description: ["หน่วยงานที่ต่างกัน", "ทำงานบนข้อมูลและพื้นที่กลางเดียวกัน"],
    anatomy: ["สี่ส่วน = หน่วยงานที่มีหน้าที่ต่างกัน", "พื้นที่กลาง = งานและข้อมูลที่ใช้ร่วมกัน"],
    note: "เหมาะเป็นสัญลักษณ์หลักของ ERPX",
  },
  {
    id: "bridge", name: "BRIDGE", number: "02", meaning: "เชื่อมทีมให้งานไปต่อ",
    description: ["งานร่วมเป็นตัวเชื่อม", "ให้การส่งต่อระหว่างทีมมีความต่อเนื่อง"],
    anatomy: ["สองฝั่ง = ทีมที่ต้องรับส่งงาน", "ชิ้นกลาง = งานร่วมที่เชื่อมสองฝั่ง"],
    note: "เน้นความร่วมมือและความมั่นคง",
  },
  {
    id: "forward", name: "FORWARD", number: "03", meaning: "หลายส่วน มุ่งสู่ผลลัพธ์เดียว",
    description: ["คน การสนทนา และข้อมูล", "รวมทิศทางไปสู่การลงมือทำ"],
    anatomy: ["สามแนว = คน การคุย และข้อมูล", "ปลายเดียว = งานที่เดินไปสู่ความสำเร็จ"],
    note: "เน้นความคล่องตัวและการลงมือทำ",
  },
];
const xml = (value) => String(value).replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll('"', "&quot;");
const svg = (w, h, title, body) => `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}" role="img" aria-labelledby="title"><title id="title">${xml(title)}</title>${body}</svg>\n`;
const rect = (x, y, w, h, fill, r = 0, stroke = "none") => `<rect x="${x}" y="${y}" width="${w}" height="${h}" rx="${r}" fill="${fill}" stroke="${stroke}"/>`;
const text = (x, y, value, size = 18, color = navy, weight = 500, more = "") => `<text x="${x}" y="${y}" font-family="Avenir Next, Arial, Sukhumvit Set, sans-serif" font-size="${size}" font-weight="${weight}" fill="${color}" ${more}>${xml(value)}</text>`;
const thai = (x, y, value, size = 20, color = muted, weight = 500, more = "") => `<text x="${x}" y="${y}" font-family="Sukhumvit Set, Thonburi, sans-serif" font-size="${size}" font-weight="${weight}" fill="${color}" ${more}>${xml(value)}</text>`;
const label = (x, y, value, color = muted) => text(x, y, value, 12, color, 600, 'letter-spacing="1.6"');
const recolor = (body, color) => body.replace(/fill="#[0-9A-Fa-f]{6}"/g, `fill="${color}"`);
const wordmarkSource = await readFile(path.join(root, "wordmark.svg"), "utf8");
const wordmarkBody = wordmarkSource.match(/<g id="wordmark"[\s\S]*<\/g>/)[0].replace(' id="wordmark"', "");

function wordmark(x, y, width, color = navy) {
  return `<svg x="${x}" y="${y}" width="${width}" height="${width * 728 / 2511}" viewBox="0 0 2511 728">${recolor(wordmarkBody, color)}</svg>`;
}

function mark(concept, x, y, size, color) {
  return `<svg x="${x}" y="${y}" width="${size}" height="${size}" viewBox="0 0 512 512">${color ? recolor(concept.body, color) : concept.body}</svg>`;
}

function tileBody(concept) {
  return rect(0, 0, 512, 512, blue, 112) + recolor(concept.body, white);
}

function tile(concept, x, y, size) {
  return `<svg x="${x}" y="${y}" width="${size}" height="${size}" viewBox="0 0 512 512">${tileBody(concept)}</svg>`;
}

function pixels(concept, x, y, size) {
  return `<image x="${x}" y="${y}" width="${size}" height="${size}" href="data:image/png;base64,${concept.pngs.get(size).toString("base64")}"/>`;
}

function lockup(concept, x, y, width, color) {
  const body = mark(concept, 0, 0, 256, color) + wordmark(272, (256 - 548 * 728 / 2511) / 2, 548, color || navy);
  return `<svg x="${x}" y="${y}" width="${width}" height="${width * 256 / 860}" viewBox="0 0 860 256">${body}</svg>`;
}

function ico(pngs) {
  const frames = [16, 32, 48].map((size) => ({ size, bytes: pngs.get(size) }));
  const header = Buffer.alloc(6 + 16 * frames.length);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(frames.length, 4);
  let offset = header.length;
  frames.forEach(({ size, bytes }, i) => {
    const p = 6 + i * 16;
    header[p] = header[p + 1] = size;
    header.writeUInt16LE(1, p + 4);
    header.writeUInt16LE(32, p + 6);
    header.writeUInt32LE(bytes.length, p + 8);
    header.writeUInt32LE(offset, p + 12);
    offset += bytes.length;
  });
  return Buffer.concat([header, ...frames.map(({ bytes }) => bytes)]);
}

for (const concept of concepts) {
  const dir = path.join(root, concept.id);
  const source = await readFile(path.join(dir, "mark.svg"), "utf8");
  concept.body = source.match(/<g id="mark"[\s\S]*<\/g>/)[0].replace(' id="mark"', "");
  concept.pngs = new Map();
  await sharp(Buffer.from(source)).png().toFile(path.join(dir, "mark-512.png"));
  for (const [filename, color] of [["mark-mono", blue], ["mark-white", white]]) {
    const output = svg(512, 512, `ERPX ${concept.name} — monochrome mark`, recolor(concept.body, color));
    await writeFile(path.join(dir, `${filename}.svg`), output);
    await sharp(Buffer.from(output)).png().toFile(path.join(dir, `${filename}-512.png`));
  }
  await writeFile(path.join(dir, "icon.svg"), svg(512, 512, `ERPX ${concept.name} — app icon`, tileBody(concept)));
  for (const size of [16, 32, 48, 512]) {
    const output = svg(size, size, `ERPX ${concept.name}`, `<g transform="scale(${size / 512})">${tileBody(concept)}</g>`);
    const png = await sharp(Buffer.from(output)).png().toBuffer();
    concept.pngs.set(size, png);
    await writeFile(path.join(dir, `icon-${size}.png`), png);
  }
  await writeFile(path.join(dir, "favicon.ico"), ico(concept.pngs));
  for (const [filename, color] of [["logo", undefined], ["logo-white", white]]) {
    const output = svg(860, 256, `ERPX ${concept.name} — logo`, lockup(concept, 0, 0, 860, color));
    await writeFile(path.join(dir, `${filename}.svg`), output);
    await sharp(Buffer.from(output)).png().toFile(path.join(dir, `${filename}.png`));
  }
  concept.enlarged = await sharp(concept.pngs.get(16)).resize(144, 144, { kernel: "nearest" }).png().toBuffer();
}
await sharp(Buffer.from(wordmarkSource)).resize({ width: 1024 }).png().toFile(path.join(root, "wordmark.png"));

function comparison() {
  let body = rect(0, 0, 1600, 1216, paper);
  body += label(48, 56, "ERPX / ENTERPRISE BLUE / DESIGN STUDY 03", blue);
  body += thai(48, 125, "สัญลักษณ์ของงานที่เชื่อมถึงกัน", 46, navy, 600);
  body += thai(48, 169, "เริ่มจากความหมายของระบบ — องค์กรที่ประสานงาน ใช้ข้อมูลร่วมกัน และทำงานให้สำเร็จ", 20, muted);
  body += `<path d="M48 208H1552" stroke="#DCE4F1"/>`;
  concepts.forEach((concept, i) => {
    const x = 48 + i * 512;
    body += rect(x, 240, 480, 846, white, 24, i === 0 ? "#B8CAEE" : "#DCE4F1");
    body += label(x + 32, 282, concept.number, blue);
    if (i === 0) {
      body += rect(x + 342, 257, 106, 30, "#EBF1FF", 15);
      body += thai(x + 395, 278, "แนวทางแนะนำ", 13, blue, 600, 'text-anchor="middle"');
    }
    body += text(x + 32, 327, concept.name, 31, navy, 600, 'letter-spacing=".5"');
    body += thai(x + 32, 367, concept.meaning, 23, navy, 600);
    body += rect(x + 32, 392, 416, 254, "#F3F6FC", 18);
    body += mark(concept, x + 112, 389, 256);
    concept.anatomy.forEach((sentence, j) => { body += thai(x + 32, 690 + j * 29, sentence, 18, muted); });
    body += rect(x + 32, 756, 416, 100, dark, 16);
    body += lockup(concept, x + 120, 770, 240, white);
    body += label(x + 32, 898, "APP ICON / FAVICON / ACTUAL SIZE");
    [16, 32, 48].forEach((size, j) => {
      const cx = x + 92 + j * 148;
      body += pixels(concept, cx - size / 2, 943 - size / 2, size);
      body += text(cx, 991, `${size} px`, 13, muted, 400, 'text-anchor="middle"');
    });
    body += thai(x + 32, 1048, concept.note, 17, blue, 500);
  });
  body += `<path d="M48 1124H1552" stroke="#DCE4F1"/>`;
  [[navy, "NAVY  #102A56"], [blue, "BLUE  #1E4FCC"], [accent, "ACCENT  #4A7FE8"]].forEach(([color, name], i) => {
    const x = 48 + i * 298;
    body += rect(x, 1154, 28, 28, color, 8);
    body += text(x + 42, 1173, name, 12, muted, 600, 'letter-spacing=".5"');
  });
  body += text(1552, 1173, "WORK CONNECTED", 12, navy, 600, 'text-anchor="end" letter-spacing="1.8"');
  return svg(1600, 1216, "ERPX — meaningful enterprise blue icon concepts", body);
}

function inContext() {
  let body = rect(0, 0, 1600, 1120, paper);
  body += label(48, 54, "ERPX / ENTERPRISE BLUE IN CONTEXT", blue);
  body += thai(48, 118, "แบรนด์ในพื้นที่ทำงาน", 44, navy, 600);
  body += thai(48, 161, "ภาพจำลองสัญลักษณ์ใหม่บนแท็บเบราว์เซอร์และส่วนหัวของแอป", 20, muted);
  concepts.forEach((concept, i) => {
    const x = 48 + i * 512;
    body += text(x, 228, `${concept.number}  ${concept.name}`, 24, navy, 600, 'letter-spacing=".5"');
    [false, true].forEach((isDark, row) => {
      const y = 259 + row * 336;
      const base = isDark ? "#14243E" : white;
      const chrome = isDark ? "#24344F" : "#DFE6F2";
      const fg = isDark ? white : navy;
      body += rect(x, y, 480, 292, base, 18);
      body += rect(x, y, 480, 50, chrome, 18);
      body += rect(x, y + 24, 480, 26, chrome);
      body += rect(x + 12, y + 12, 296, 38, base, 10);
      body += rect(x + 12, y + 39, 296, 11, base);
      body += pixels(concept, x + 26, y + 23, 16);
      body += text(x + 52, y + 36, "ERPX · Work connected", 12, fg);
      body += text(x + 281, y + 36, "×", 16, isDark ? "#A7B6CF" : muted);
      body += text(x + 330, y + 36, "+", 18, fg);
      body += tile(concept, x + 30, y + 80, 33);
      body += wordmark(x + 77, y + 85, 77, fg);
      body += text(x + 30, y + 172, "Good work.", 33, fg, 500, 'letter-spacing="-1"');
      body += text(x + 30, y + 215, "Greater clarity.", 33, fg, 500, 'letter-spacing="-1"');
      body += text(x + 30, y + 257, "Your workspace. Your people. Connected.", 12, isDark ? "#A7B6CF" : muted);
    });
    body += rect(x, 935, 480, 86, white, 16, "#DCE4F1");
    body += tile(concept, x + 27, 962, 31);
    body += wordmark(x + 71, 967, 72, navy);
    body += text(x + 450, 983, "SIDEBAR / 31 PX", 11, muted, 500, 'text-anchor="end"');
    body += thai(x, 1065, concept.meaning, 20, blue, 600);
  });
  return svg(1600, 1120, "ERPX — enterprise blue branding in light and dark app mockups", body);
}

function sizeCheck() {
  let body = rect(0, 0, 1200, 736, paper);
  body += thai(36, 64, "สัญลักษณ์ยังชัด เมื่อเหลือเพียง 16 พิกเซล", 33, navy, 600);
  body += thai(36, 107, "ขนาดจริงบนพื้นสว่างและมืด · แถวล่างขยาย 16 px แบบเห็นพิกเซล", 18, muted);
  concepts.forEach((concept, i) => {
    const x = 36 + i * 388;
    body += text(x, 164, concept.name, 23, navy, 600);
    [false, true].forEach((isDark, row) => {
      const y = 192 + row * 128;
      body += rect(x, y, 352, 108, isDark ? dark : white, 14);
      [16, 32, 48].forEach((size, j) => {
        const cx = x + 62 + j * 110;
        body += pixels(concept, cx - size / 2, y + 41 - size / 2, size);
        body += text(cx, y + 87, `${size} px`, 12, isDark ? "#A7B6CF" : muted, 400, 'text-anchor="middle"');
      });
    });
    body += label(x, 473, "16 PX / 9× PIXEL VIEW");
    body += `<image x="${x}" y="494" width="144" height="144" href="data:image/png;base64,${concept.enlarged.toString("base64")}"/>`;
    body += rect(x + 188, 494, 144, 144, dark, 16);
    body += `<image x="${x + 188}" y="494" width="144" height="144" href="data:image/png;base64,${concept.enlarged.toString("base64")}"/>`;
  });
  body += thai(36, 696, "เปิดภาพที่ 100% เพื่อดูขนาดจริง · favicon ใช้รูปทรงสีขาวบนพื้นน้ำเงินเดียวกันทั้งสามแบบ", 17, muted);
  return svg(1200, 736, "ERPX — enterprise blue favicon size check", body);
}

for (const [filename, source] of [["comparison", comparison()], ["in-context", inContext()], ["size-check", sizeCheck()]]) {
  await writeFile(path.join(root, `${filename}.svg`), source);
  await sharp(Buffer.from(source)).png().toFile(path.join(root, `${filename}.png`));
}
console.log("Exported 3 enterprise blue identities and 3 review boards.");
