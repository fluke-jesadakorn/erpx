import { workspaceCopyInput } from './workspace-copy-input';
import { createHash } from "node:crypto";
import { readFile, readdir, access, stat } from "node:fs/promises";
import { join, relative } from "node:path";

const generatedSourceEntry = (name: string) => name.split(/[\\/]/).includes("__pycache__") ||
  /\.(?:pyc|pyo)$/.test(name) || name.split(/[\\/]/).at(-1) === ".DS_Store";

/** The same bytes identify the release, compiled worker, and engineering receipts. */
export async function runtimeSourceFingerprint(root: string, environment: NodeJS.ProcessEnv = process.env) {
  const hash = createHash("sha256").update("erpx-workspace-runtime/v2:node26.11.1:bun1.4.3");
  for (const directory of ["src", "apps", "packages", "public", "scripts/local", "scripts/assets", "migrations/postgres"]) {
    try { await access(join(root,directory)); } catch { continue; }
    const pending = [{ path: join(root, directory), links: 0 }], inputs: string[] = [];
    for (let index = 0; index < pending.length; index++) {
      const current = pending[index];
      for (const entry of await readdir(current.path, { withFileTypes: true })) {
        const path = join(current.path, entry.name);
        if (!workspaceCopyInput(root, path) || entry.name === "node_modules" || entry.name === "__pycache__") continue;
        if (entry.isDirectory()) pending.push({ path, links: current.links });
        else if (entry.isFile()) inputs.push(path);
        else if (entry.isSymbolicLink()) {
          // Recursive readdir includes children of app public-directory links.
          // Retain their lexical input names while pruning generated trees.
          let target;
          try { target = await stat(path); } catch (error) {
            if ((error as NodeJS.ErrnoException).code === "ENOENT") continue;
            throw error;
          }
          if (target.isDirectory()) {
            if (current.links >= 40) throw new Error("RUNTIME_SOURCE_SYMLINK_LOOP");
            pending.push({ path, links: current.links + 1 });
          }
        }
      }
    }
    const files = inputs
      .filter(file => !generatedSourceEntry(relative(root, file)) && !relative(root, file).split(/[\\/]/).includes("node_modules") && workspaceCopyInput(root,file) && !file.endsWith("/next-env.d.ts") && !file.endsWith("/AGENTS.md")).sort();
    for (const file of files) {
      const name = relative(root, file);
      if (name === "packages/catalogue/src/release-artifact.json" || /^apps\/[^/]+\/turbo\.json$/.test(name)) continue;
      hash.update(name).update("\0").update(await readFile(file)).update("\0");
    }
  }
  const configuration = ["bun.lock", "package.json", "next.config.ts", "tsconfig.base.json", "postcss.config.mjs"];
  for (const name of ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'tests/config/tsconfig.frontend.json', 'LICENSE', 'NOTICE', 'LICENSING.md', 'CONTRIBUTING.md', 'SECURITY.md', 'CODE_OF_CONDUCT.md', 'GOVERNANCE.md']) {
    try { await access(join(root, name)); configuration.push(name); } catch { /* Historical TypeScript-only candidates have no Cargo workspace. */ }
  }
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
  for (const directory of ["tests", "fixtures", "scripts/testing", "scripts/architecture", "scripts/performance"]) {
    try { await access(join(root,directory)); } catch { continue; }
    const entries = await readdir(join(root, directory), { recursive: true, withFileTypes: true });
    for (const file of entries.filter(entry => entry.isFile()).map(entry => join(entry.parentPath, entry.name))
      .filter(file => !generatedSourceEntry(relative(root, file)) && !relative(root, file).split(/[\\/]/).some(part => ["node_modules", ".next", ".build", ".cache", ".turbo"].includes(part)) && !file.endsWith("/next-env.d.ts") && !file.endsWith("/AGENTS.md")).sort())
      hash.update(relative(root, file)).update("\0").update(await readFile(file)).update("\0");
  }
  for (const name of ["tests/catalogue/engineering-scenarios.json", "tests/catalogue/legal-uat-engineering-cases.json",
    "tests/catalogue/engineering-gate-scenarios.json", "tests/catalogue/engineering-attachment-owner-cases.json", "tests/catalogue/uat-route-inventory.json", "tests/config/vitest.config.mts", "tests/config/vitest.unit.config.mts", "tests/config/vitest.integration.config.mts", "tests/config/playwright.config.ts", "eslint.config.mjs"])
    hash.update(name).update("\0").update(await readFile(join(root, name))).update("\0");
  for (const directory of [".github/workflows", ".github/actions"]) {
    try { await access(join(root,directory)); } catch { continue; }
    const entries = await readdir(join(root, directory), { recursive: true, withFileTypes: true });
    for (const file of entries.filter(entry => entry.isFile()).map(entry => join(entry.parentPath, entry.name))
      .filter(file => !generatedSourceEntry(relative(root, file)) && !relative(root, file).split(/[\\/]/).some(part => ["node_modules", ".next", ".build", ".cache", ".turbo"].includes(part)) && !file.endsWith("/next-env.d.ts") && !file.endsWith("/AGENTS.md")).sort())
      hash.update(relative(root, file)).update("\0").update(await readFile(file)).update("\0");
  }
  return hash.digest("hex");
}

/** Fixture revisions invalidate template preparation without changing app artifacts. */
export async function fixtureSourceFingerprint(root: string) {
  const hash = createHash("sha256").update("erpx-workspace-fixtures/v1");
  for (const directory of ["fixtures", "tests/support", "scripts/seed", "migrations/postgres"]) {
    const entries = await readdir(join(root,directory),{recursive:true,withFileTypes:true});
    for (const file of entries.filter(entry=>entry.isFile()).map(entry=>join(entry.parentPath,entry.name)).sort())
      hash.update(relative(root,file)).update("\0").update(await readFile(file)).update("\0");
  }
  return hash.digest("hex");
}
