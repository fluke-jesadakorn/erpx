import sharp from "sharp";
import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const icon = await readFile(join(root, "src/app/icon.svg"));

for (const size of [192, 512]) {
  await sharp(icon)
    .resize(size, size)
    .png()
    .toFile(join(root, `public/icon-${size}.png`));
}
await sharp(icon)
  .resize(410, 410)
  .flatten({ background: "#2F69F5" })
  .extend({
    top: 51,
    bottom: 51,
    left: 51,
    right: 51,
    background: "#2F69F5",
  })
  .png()
  .toFile(join(root, "public/icon-maskable-512.png"));
await sharp(icon).resize(180, 180).png().toFile(join(root, "src/app/apple-icon.png"));

const faviconPngs = await Promise.all([16, 32, 48].map((size) => sharp(icon).resize(size, size).png().toBuffer()));
const header = Buffer.alloc(6 + faviconPngs.length * 16);
header.writeUInt16LE(0, 0);
header.writeUInt16LE(1, 2);
header.writeUInt16LE(faviconPngs.length, 4);
let offset = header.length;
faviconPngs.forEach((png, index) => {
  const entry = 6 + index * 16;
  const size = [16, 32, 48][index];
  header[entry] = size;
  header[entry + 1] = size;
  header.writeUInt16LE(1, entry + 4);
  header.writeUInt16LE(32, entry + 6);
  header.writeUInt32LE(png.length, entry + 8);
  header.writeUInt32LE(offset, entry + 12);
  offset += png.length;
});
await writeFile(join(root, "src/app/favicon.ico"), Buffer.concat([header, ...faviconPngs]));
