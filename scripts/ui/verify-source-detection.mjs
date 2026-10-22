import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { cp, mkdir, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import postcss from "postcss";
import tailwind from "@tailwindcss/postcss";

const root = resolve(process.argv[2] ?? ".");
const scratch = resolve(".scratch");
await mkdir(scratch, { recursive: true });
const isolated = await mkdtemp(join(scratch, "ui-source-check-"));
async function compile(directory) {
  const from = join(directory, "packages/workplace/src/routes/globals.css");
  const result = await postcss([
    tailwind({ base: directory, optimize: { minify: true }, transformAssetUrls: false }),
  ]).process(await readFile(from, "utf8"), { from, map: false });
  return result.css;
}
try {
  await cp(join(root, "src"), join(isolated, "src"), { recursive: true });
  await symlink(join(root, "node_modules"), join(isolated, "node_modules"), "dir");
  // A release copy has no Git ignore context. Non-source artifacts must still
  // never contribute utility classes, even when they are present in the copy.
  for (const directory of ["tests", "docs", "vendor"]) {
    await mkdir(join(isolated, directory), { recursive: true });
    await writeFile(
      join(isolated, directory, "source-check.tsx"),
      'const probe = "w-[13703px]";\n',
    );
  }
  const [workspaceCss, isolatedCss] = await Promise.all([compile(root), compile(isolated)]);
  assert.equal(
    isolatedCss,
    workspaceCss,
    "Isolated and workspace CSS must match for the same src.",
  );
  assert.equal(isolatedCss.includes("13703px"), false, "Files outside src must not add utilities.");
  const sha256 = createHash("sha256").update(isolatedCss).digest("hex");
  console.log(
    JSON.stringify({
      identical: true,
      excludesNonSourceArtifacts: true,
      bytes: Buffer.byteLength(isolatedCss),
      sha256,
    }),
  );
} finally {
  await rm(isolated, { recursive: true, force: true });
}
