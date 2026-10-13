import { cp, mkdir } from "node:fs/promises";
import { join } from "node:path";
const version = "6.3.289",
  source = join(import.meta.dir, "../../node_modules/pdfjs-dist"),
  target = join(import.meta.dir, `../../public/vendor/pdfjs-${version}`);
await mkdir(target, { recursive: true });
for (const name of ["wasm", "cmaps", "standard_fonts", "LICENSE"])
  await cp(join(source, name), join(target, name), { recursive: true });
await cp(
  join(source, "build/pdf.worker.min.mjs"),
  join(target, "pdf.worker.min.mjs"),
);
