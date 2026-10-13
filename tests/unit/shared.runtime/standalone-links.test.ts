import { afterEach, expect, it } from "vitest";
import { cp, mkdir, mkdtemp, readFile, readlink, realpath, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { relocateStandaloneLinks } from "../../../scripts/local/standalone-links";

const directories: string[] = [];
afterEach(async () => { for (const directory of directories.splice(0)) await rm(directory, { recursive: true, force: true }); });

it("preserves traced dependency aliases and their canonical identity after deleting the build tree", async () => {
  const root = await mkdtemp(join(tmpdir(), "erpx-standalone-links-")); directories.push(root);
  const standalone = join(root, "build/standalone"), release = join(root, "release");
  await mkdir(join(standalone, "node_modules/pg"), { recursive: true });
  await mkdir(join(standalone, ".next/node_modules"), { recursive: true });
  await writeFile(join(standalone, "node_modules/pg/index.js"), "module.exports = { package: 'pg' };\n");
  await symlink(join(standalone, "node_modules/pg"), join(standalone, ".next/node_modules/pg-hash"));
  await cp(standalone, release, { recursive: true });
  expect(await relocateStandaloneLinks(standalone, release)).toBe(1);
  expect(await readlink(join(release, ".next/node_modules/pg-hash"))).toBe("../../node_modules/pg");
  await rm(join(root, "build"), { recursive: true });
  expect(await realpath(join(release, ".next/node_modules/pg-hash"))).toBe(await realpath(join(release, "node_modules/pg")));
  expect(await readFile(join(release, ".next/node_modules/pg-hash/index.js"), "utf8")).toBe("module.exports = { package: 'pg' };\n");
  expect(await relocateStandaloneLinks(standalone, release)).toBe(1);
});

it("refuses a standalone dependency that still requires a host path outside the release", async () => {
  const root = await mkdtemp(join(tmpdir(), "erpx-standalone-external-")); directories.push(root);
  const standalone = join(root, "build/standalone"), release = join(root, "release");
  await mkdir(standalone, { recursive: true });
  await mkdir(release, { recursive: true });
  await writeFile(join(root, "host.js"), "module.exports = {};\n");
  await symlink(join(root, "host.js"), join(release, "dependency.js"));
  await expect(relocateStandaloneLinks(standalone, release)).rejects.toThrow("STANDALONE_EXTERNAL_LINK");
  expect(await readlink(join(release, "dependency.js"))).toBe(join(root, "host.js"));
});
