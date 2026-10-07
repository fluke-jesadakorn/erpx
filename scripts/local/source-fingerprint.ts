import { createHash } from "node:crypto";
import { readFile, readdir } from "node:fs/promises";
import { join, relative } from "node:path";

const generatedSourceEntry = (name: string) => name.split(/[\\/]/).includes("__pycache__") ||
  /\.(?:pyc|pyo)$/.test(name) || name.split(/[\\/]/).at(-1) === ".DS_Store";

/** The same bytes identify the release, compiled worker, and engineering receipts. */
export async function runtimeSourceFingerprint(root: string, environment: NodeJS.ProcessEnv = process.env) {
  const hash = createHash("sha256").update("node24.20.0:bun1.2.17");
  for (const directory of ["src", "public", "scripts", "fixtures", "tests/helpers", "migrations/postgres"]) {
    const entries = await readdir(join(root, directory), { recursive: true, withFileTypes: true });
    const files = entries.filter(entry => entry.isFile()).map(entry => join(entry.parentPath, entry.name))
      .filter(file => !generatedSourceEntry(relative(root, file))).sort();
    for (const file of files) {
      const name = relative(root, file);
      if (name === "src/data/release-artifact.json") continue;
      hash.update(name).update("\0").update(await readFile(file)).update("\0");
    }
  }
  const configuration = ["bun.lock", "package.json", "next.config.ts", "tsconfig.json", "tsconfig.engineering-tests.json", "postcss.config.mjs"];
  for (const name of configuration) hash.update(name).update("\0").update(await readFile(join(root, name))).update("\0");
  const ops = ["ops/macos/install-attachment-runtime.py", "ops/macos/attachment-rollout.md",
    "ops/macos/erpxctl.py", "ops/macos/offsite.py", "ops/macos/recovery_status.py",
    ...(await readdir(join(root, "ops/macos/clamav"), { withFileTypes: true })).filter(entry => entry.isFile() && !generatedSourceEntry(entry.name))
      .map(entry => entry.name).sort().map(name => `ops/macos/clamav/${name}`)];
  for (const name of ops) hash.update(name).update("\0").update(await readFile(join(root, name))).update("\0");
  for (const key of Object.keys(environment).filter(key => key.startsWith("NEXT_PUBLIC_")).sort())
    hash.update(key).update("\0").update(environment[key] ?? "").update("\0");
  return hash.digest("hex");
}

/** A runtime match alone cannot attest to different assertions or a changed coverage register. */
export async function verificationSourceFingerprint(root: string, runtimeFingerprint: string) {
  const hash = createHash("sha256").update(runtimeFingerprint);
  // Includes suite-manifest.json and the standalone provider-rehearsals.json register.
  for (const directory of ["tests"]) {
    const entries = await readdir(join(root, directory), { recursive: true, withFileTypes: true });
    for (const file of entries.filter(entry => entry.isFile()).map(entry => join(entry.parentPath, entry.name))
      .filter(file => !generatedSourceEntry(relative(root, file))).sort())
      hash.update(relative(root, file)).update("\0").update(await readFile(file)).update("\0");
  }
  for (const name of ["docs/testing/engineering-scenarios.json", "docs/testing/legal-uat-engineering-cases.json",
    "docs/testing/engineering-gate-scenarios.json", "docs/testing/engineering-attachment-owner-cases.json", "docs/testing/uat-route-inventory.json", "vitest.config.mts", "vitest.unit.config.mts", "vitest.integration.config.mts", "playwright.config.ts", "eslint.config.mjs"])
    hash.update(name).update("\0").update(await readFile(join(root, name))).update("\0");
  for (const directory of [".github/workflows", ".github/actions"]) {
    const entries = await readdir(join(root, directory), { recursive: true, withFileTypes: true });
    for (const file of entries.filter(entry => entry.isFile()).map(entry => join(entry.parentPath, entry.name))
      .filter(file => !generatedSourceEntry(relative(root, file))).sort())
      hash.update(relative(root, file)).update("\0").update(await readFile(file)).update("\0");
  }
  return hash.digest("hex");
}
