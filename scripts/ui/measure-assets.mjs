import { readdir, readFile } from "node:fs/promises";
import { resolve, join, extname, relative } from "node:path";
import { gzipSync, brotliCompressSync } from "node:zlib";
const root = resolve(process.argv[2] ?? ".");
const directory = join(root, ".next/static");
const entries = await readdir(directory, { recursive: true, withFileTypes: true });
const result = { build: relative(process.cwd(), root), totals: {} };
for (const extension of [".css", ".js", ".woff2", ".ttf"]) {
  const paths = entries
    .filter((entry) => entry.isFile() && extname(entry.name) === extension)
    .map((entry) => join(entry.parentPath, entry.name));
  if (extension === ".ttf" || extension === ".woff2") {
    const fonts = await readdir(join(root, "public/fonts"), { withFileTypes: true });
    paths.push(
      ...fonts
        .filter((entry) => entry.isFile() && extname(entry.name) === extension)
        .map((entry) => join(root, "public/fonts", entry.name)),
    );
  }
  const totals = { files: paths.length, raw: 0, gzip: 0, brotli: 0 };
  for (const path of paths) {
    const content = await readFile(path);
    totals.raw += content.length;
    totals.gzip += gzipSync(content).length;
    totals.brotli += brotliCompressSync(content).length;
  }
  result.totals[extension] = totals;
}
console.log(JSON.stringify(result, null, 2));
