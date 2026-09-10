// Rebuild the draft exports from the three editable icon.svg masters.
// Run from the repo root: bun Designed/branding/erpx-icons/generate.mjs
import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";
import sharp from "sharp";

const root = path.dirname(fileURLToPath(import.meta.url));
const green = "#176B50";
const ink = "#172D25";
const muted = "#64746E";
const paper = "#F6F8F6";
const dark = "#141C17";
const sizes = [16, 32, 48, 512];
const concepts = [
  {
    id: "e-connect", name: "E Connect", number: "01",
    description: ["คน งาน และข้อมูล", "เชื่อมกันด้วยแกนเดียว"],
    note: "เรียบ ชัดเจน เหมาะกับ favicon",
  },
  {
    id: "e-modules", name: "E Modules", number: "02",
    description: ["สามส่วนประกอบเป็นหนึ่ง", "สะท้อนโมดูลที่ทำงานร่วมกัน"],
    note: "เป็นระบบ เห็นจังหวะของแต่ละส่วน",
  },
  {
    id: "e-flow", name: "E Flow", number: "03",
    description: ["เส้นเดียวเชื่อมต่ออย่างลื่นไหล", "สื่อถึงการส่งต่องานและความร่วมมือ"],
    note: "นุ่มนวล เป็นมิตร มีความเคลื่อนไหว",
  },
];

const xml = (value) => String(value).replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll('"', "&quot;");
const text = (x, y, value, size = 18, color = ink, weight = 400, more = "") =>
  `<text x="${x}" y="${y}" font-family="Arial, Sukhumvit Set, Thonburi, sans-serif" font-size="${size}" font-weight="${weight}" fill="${color}" ${more}>${xml(value)}</text>`;
const thai = (x, y, value, size = 20, color = ink, weight = 500, more = "") =>
  `<text x="${x}" y="${y}" font-family="Sukhumvit Set, Thonburi, sans-serif" font-size="${size}" font-weight="${weight}" fill="${color}" ${more}>${xml(value)}</text>`;
const rect = (x, y, width, height, fill, radius = 0, stroke = "none") =>
  `<rect x="${x}" y="${y}" width="${width}" height="${height}" rx="${radius}" fill="${fill}" stroke="${stroke}"/>`;
const line = (x1, y1, x2, y2, color = "#E2E8E4") =>
  `<path d="M${x1} ${y1}L${x2} ${y2}" stroke="${color}"/>`;
const label = (x, y, value, color = muted) => text(x, y, value, 12, color, 600, 'letter-spacing="1.7"');
const svg = (width, height, title, body) => `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" role="img" aria-labelledby="title"><title id="title">${xml(title)}</title>${body}</svg>\n`;

function placed(concept, x, y, size, mode = "tile") {
  const body = mode === "tile" ? concept.body : concept.mark.replaceAll("#FFFFFF", mode === "white" ? "#FFFFFF" : green);
  return `<svg x="${x}" y="${y}" width="${size}" height="${size}" viewBox="0 0 512 512">${body}</svg>`;
}

function pixels(concept, x, y, size) {
  return `<image x="${x}" y="${y}" width="${size}" height="${size}" href="data:image/png;base64,${concept.pngs.get(size).toString("base64")}"/>`;
}

function wordmark(x, y, size = 28, color = ink) {
  // Match the lowercase wording, weight and tight spacing of the current UI.
  return text(x, y, "erpx.", size, color, 700, 'letter-spacing="-1"');
}

async function png(svgText, size) {
  const sized = svgText.replace('width="512" height="512"', `width="${size}" height="${size}"`);
  return sharp(Buffer.from(sized)).png().toBuffer();
}

// Standard 32-bit DIB frames, with an AND mask, for broad ICO compatibility.
// Each size is rendered independently from SVG rather than a larger bitmap.
async function ico(pngs) {
  const frames = [];
  for (const size of [16, 32, 48]) {
    const rgba = await sharp(pngs.get(size)).ensureAlpha().raw().toBuffer();
    const stride = Math.ceil(size / 32) * 4;
    const dib = Buffer.alloc(40 + size * size * 4 + stride * size);
    dib.writeUInt32LE(40, 0);
    dib.writeInt32LE(size, 4);
    dib.writeInt32LE(size * 2, 8);
    dib.writeUInt16LE(1, 12);
    dib.writeUInt16LE(32, 14);
    dib.writeUInt32LE(size * size * 4 + stride * size, 20);
    for (let y = 0; y < size; y++) {
      for (let x = 0; x < size; x++) {
        const source = (y * size + x) * 4;
        const bottomUp = size - 1 - y;
        const target = 40 + (bottomUp * size + x) * 4;
        dib[target] = rgba[source + 2];
        dib[target + 1] = rgba[source + 1];
        dib[target + 2] = rgba[source];
        dib[target + 3] = rgba[source + 3];
        if (rgba[source + 3] === 0) {
          dib[40 + size * size * 4 + bottomUp * stride + Math.floor(x / 8)] |= 0x80 >> (x % 8);
        }
      }
    }
    frames.push({ size, dib });
  }
  const directory = Buffer.alloc(6 + 16 * frames.length);
  directory.writeUInt16LE(1, 2);
  directory.writeUInt16LE(frames.length, 4);
  let offset = directory.length;
  frames.forEach(({ size, dib }, i) => {
    const entry = 6 + i * 16;
    directory[entry] = size;
    directory[entry + 1] = size;
    directory.writeUInt16LE(1, entry + 4);
    directory.writeUInt16LE(32, entry + 6);
    directory.writeUInt32LE(dib.length, entry + 8);
    directory.writeUInt32LE(offset, entry + 12);
    offset += dib.length;
  });
  return Buffer.concat([directory, ...frames.map((frame) => frame.dib)]);
}

for (const concept of concepts) {
  const directory = path.join(root, concept.id);
  const source = await readFile(path.join(directory, "icon.svg"), "utf8");
  concept.body = source.match(/<rect[\s\S]*?(?=<\/svg>)/)[0].replace('id="mark"', "");
  concept.mark = source.match(/<g id="mark"[\s\S]*?<\/g>/)[0].replace(' id="mark"', "");
  concept.pngs = new Map();
  for (const size of sizes) {
    const rendered = await png(source, size);
    concept.pngs.set(size, rendered);
    await writeFile(path.join(directory, `icon-${size}.png`), rendered);
  }
  for (const [filename, color] of [["mark", green], ["mark-white", "#FFFFFF"]]) {
    const mark = svg(512, 512, `ERPX — ${concept.name} — monochrome`, concept.mark.replaceAll("#FFFFFF", color));
    await writeFile(path.join(directory, `${filename}.svg`), mark);
    await writeFile(path.join(directory, `${filename}-512.png`), await png(mark, 512));
  }
  await writeFile(path.join(directory, "favicon.ico"), await ico(concept.pngs));
}

function comparison() {
  let body = rect(0, 0, 1600, 1152, paper);
  body += label(48, 56, "ERPX / IDENTITY EXPLORATION", green);
  body += thai(48, 123, "งานเชื่อมกัน แบรนด์เดียวกัน", 46, ink, 600);
  body += thai(48, 167, "สามแนวทางของ E เชื่อมต่อ — เรียบสงบ ชัดเจน และพร้อมอยู่ข้างทุกวันทำงาน", 20, muted);
  body += rect(1364, 62, 188, 42, "#E9F3EC", 21);
  body += text(1458, 88, "03 CONCEPTS", 12, green, 700, 'text-anchor="middle" letter-spacing="1.4"');
  body += line(48, 202, 1552, 202);
  concepts.forEach((concept, i) => {
    const x = 48 + i * 512;
    body += rect(x, 234, 480, 798, "#FFFFFF", 24, i === 0 ? "#B9D4C5" : "#E2E8E4");
    body += label(x + 32, 276, concept.number, green);
    if (i === 0) {
      body += rect(x + 274, 254, 174, 30, "#E9F3EC", 15);
      body += thai(x + 361, 275, "แนะนำสำหรับ favicon", 14, green, 600, 'text-anchor="middle"');
    }
    body += text(x + 32, 321, concept.name, 34, ink, 600, 'letter-spacing="-1"');
    concept.description.forEach((value, j) => { body += thai(x + 32, 356 + j * 28, value, 20, muted); });
    body += rect(x + 32, 410, 416, 292, "#F0F4F1", 18);
    body += placed(concept, x + 148, 440, 184);
    body += wordmark(x + 240, 674, 30).replace('<text ', '<text text-anchor="middle" ');
    body += label(x + 32, 734, "ONE COLOR / TRANSPARENT MARK");
    body += rect(x + 32, 752, 196, 94, "#F0F4F1", 14);
    body += rect(x + 244, 752, 204, 94, dark, 14);
    body += placed(concept, x + 87, 756, 86, "green");
    body += placed(concept, x + 303, 756, 86, "white");
    body += label(x + 32, 882, "FAVICON / ACTUAL SIZE");
    for (const [j, size] of [16, 32, 48].entries()) {
      const cx = x + 92 + j * 148;
      body += pixels(concept, cx - size / 2, 920 - size / 2, size);
      body += text(cx, 966, `${size} px`, 13, muted, 400, 'text-anchor="middle"');
    }
    body += thai(x + 32, 1005, concept.note, 16, green);
  });
  body += line(48, 1067, 1552, 1067);
  body += thai(48, 1108, "เขียว ERPX #176B50 · สัญลักษณ์สีขาว · รูปทรงมุมมน", 17, muted);
  body += text(1552, 1107, "DESIGN STUDY 01 / 09 SEP 2026", 12, muted, 500, 'text-anchor="end" letter-spacing="1.2"');
  return svg(1600, 1152, "ERPX — เปรียบเทียบไอคอน E Connect, E Modules และ E Flow", body);
}

function browserChrome(concept, x, y, isDark) {
  const base = isDark ? "#1B261F" : "#FFFFFF";
  const chrome = isDark ? "#303E36" : "#E2E8E4";
  const textColor = isDark ? "#DBE4DF" : "#26362F";
  let body = rect(x, y, 480, 106, chrome, 16);
  body += rect(x, y + 20, 480, 86, chrome);
  body += rect(x + 12, y + 12, 298, 36, base, 10);
  body += rect(x + 12, y + 36, 298, 12, base);
  body += pixels(concept, x + 26, y + 22, 16);
  body += text(x + 52, y + 35, "ERPX · Work connected", 13, textColor);
  body += text(x + 282, y + 35, "×", 17, isDark ? "#A0B1A7" : muted);
  body += text(x + 328, y + 35, "+", 20, textColor);
  body += rect(x, y + 48, 480, 58, base);
  body += `<path d="M${x + 27} ${y + 68}l-7 7 7 7M${x + 20} ${y + 75}h16" fill="none" stroke="${textColor}" stroke-width="1.5"/>`;
  body += rect(x + 52, y + 60, 408, 31, isDark ? dark : paper, 15);
  body += text(x + 70, y + 80, "erpx / workspace", 12, isDark ? "#A0B1A7" : muted);
  return body;
}

function contexts() {
  let body = rect(0, 0, 1600, 1432, paper);
  body += label(48, 55, "ERPX / IN CONTEXT", green);
  body += thai(48, 117, "จากแท็บเล็ก ๆ สู่พื้นที่ทำงาน", 43, ink, 600);
  body += thai(48, 159, "ภาพจำลองตำแหน่งโลโก้จากหน้าล็อกอินและ sidebar ปัจจุบัน", 20, muted);
  body += line(48, 195, 1552, 195);
  concepts.forEach((concept, i) => {
    const x = 48 + i * 512;
    body += text(x, 243, `${concept.number}   ${concept.name}`, 26, ink, 600, 'letter-spacing="-.6"');
    body += label(x, 284, "BROWSER TAB / LIGHT / 16 PX");
    body += browserChrome(concept, x, 304, false);
    body += rect(x, 410, 480, 217, "#E8EFE7", 16);
    body += rect(x, 410, 480, 20, "#E8EFE7");
    body += placed(concept, x + 32, 440, 33);
    body += wordmark(x + 75, 465, 24, "#234532");
    body += text(x + 32, 527, "Good work.", 33, "#234532", 500, 'letter-spacing="-1.4"');
    body += text(x + 32, 568, "Greater clarity.", 33, "#234532", 500, 'letter-spacing="-1.4"');
    body += text(x + 32, 602, "Your workspace. Your people. Connected.", 12, "#57705E");
    body += label(x, 677, "BROWSER TAB / DARK / 16 PX");
    body += browserChrome(concept, x, 698, true);
    body += rect(x, 804, 480, 190, dark, 16);
    body += rect(x, 804, 480, 20, dark);
    body += placed(concept, x + 32, 836, 33);
    body += wordmark(x + 75, 861, 24, "#F0F5F1");
    body += text(x + 32, 920, "Work connected.", 32, "#F0F5F1", 500, 'letter-spacing="-1.1"');
    body += thai(x + 32, 960, "ชัดเจนบนพื้นเข้ม ด้วยสัญลักษณ์สีขาว", 17, "#A0B1A7");
    body += label(x, 1042, "SIDEBAR BRAND / LIGHT + DARK / 31 PX");
    body += rect(x, 1064, 480, 88, "#FFFFFF", 14, "#E2E8E4");
    body += placed(concept, x + 28, 1092, 31);
    body += wordmark(x + 69, 1115, 20);
    body += text(x + 442, 1113, "⌘ K", 12, muted, 400, 'text-anchor="end"');
    body += rect(x, 1168, 480, 88, "#1B261F", 14, "#303E36");
    body += placed(concept, x + 28, 1196, 31);
    body += wordmark(x + 69, 1219, 20, "#F0F5F1");
    body += text(x + 442, 1217, "⌘ K", 12, "#A0B1A7", 400, 'text-anchor="end"');
    body += thai(x, 1302, concept.note, 17, green);
  });
  body += line(48, 1338, 1552, 1338);
  body += thai(48, 1381, "favicon 16 px · ล็อกอิน 33 px · sidebar 31 px · ใช้ขนาดจริงในไฟล์ภาพ", 17, muted);
  return svg(1600, 1432, "ERPX — ภาพจำลองไอคอนบนแท็บเบราว์เซอร์ หน้าล็อกอิน และ sidebar", body);
}

function sizeCheck() {
  let body = rect(0, 0, 1200, 764, paper);
  body += text(36, 56, "Small size, clear identity.", 34, ink, 600, 'letter-spacing="-1"');
  body += thai(36, 95, "แถวบนเป็นขนาดจริง · แถวล่างขยาย favicon 16 px แบบเห็นพิกเซล", 20, muted);
  concepts.forEach((concept, i) => {
    const x = 36 + i * 388;
    body += text(x, 160, concept.name, 24, ink, 600);
    [false, true].forEach((isDark, row) => {
      const y = 186 + row * 130;
      body += rect(x, y, 352, 108, isDark ? dark : "#FFFFFF", 14, isDark ? "none" : "#E2E8E4");
      [16, 32, 48].forEach((size, j) => {
        const cx = x + 64 + j * 110;
        body += pixels(concept, cx - size / 2, y + 40 - size / 2, size);
        body += text(cx, y + 87, `${size} px`, 12, isDark ? "#A0B1A7" : muted, 400, 'text-anchor="middle"');
      });
    });
    body += label(x, 469, "16 PX / 10× PIXEL VIEW");
    body += `<image x="${x}" y="490" width="160" height="160" style="image-rendering:pixelated" href="data:image/png;base64,${concept.enlarged.toString("base64")}"/>`;
    body += rect(x + 188, 490, 164, 160, dark, 14);
    body += `<image x="${x + 190}" y="490" width="160" height="160" href="data:image/png;base64,${concept.enlarged.toString("base64")}"/>`;
    body += thai(x, 700, concept.note, 16, green);
  });
  return svg(1200, 764, "ERPX — ตรวจ favicon ขนาดจริงและภาพขยายพิกเซล", body);
}

for (const concept of concepts) {
  concept.enlarged = await sharp(concept.pngs.get(16)).resize(160, 160, { kernel: "nearest" }).png().toBuffer();
}
for (const [filename, source] of [["comparison", comparison()], ["in-context", contexts()], ["size-check", sizeCheck()]]) {
  await writeFile(path.join(root, `${filename}.svg`), source);
  await sharp(Buffer.from(source)).png().toFile(path.join(root, `${filename}.png`));
}
console.log("Exported 3 icon families, 3 ICO files and 3 review boards.");
