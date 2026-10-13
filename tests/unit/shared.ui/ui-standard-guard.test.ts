import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";

const roots: string[] = [];
const fingerprint = (text: string) => createHash("sha256").update(text).digest("hex");

/** Run the real guard in an isolated repository, without mutating live sources or inventories. */
function fixture({
  content = 'import { Button as Action } from "@/components/ui/button"; export const View = () => <Action appearance="standard">Open</Action>;',
  frozenChanged = false,
  boundary = false,
  stale = false,
  complete = false,
} = {}) {
  const root = mkdtempSync(join(tmpdir(), "erpx-ui-guard-"));
  roots.push(root);
  for (const directory of ["scripts/ui", "docs/ux-ui", "src", "node_modules"]) mkdirSync(join(root, directory), { recursive: true });
  copyFileSync(resolve("scripts/ui/check-standard.mjs"), join(root, "scripts/ui/check-standard.mjs"));
  symlinkSync(resolve("node_modules/typescript"), join(root, "node_modules/typescript"), "dir");
  writeFileSync(join(root, "src/view.tsx"), content);
  writeFileSync(join(root, "src/specialized.tsx"), frozenChanged ? "changed layout" : "retained layout");
  writeFileSync(join(root, "scripts/ui/frozen-boundaries.json"), JSON.stringify([
    { source: "src/specialized.tsx", fingerprint: fingerprint("retained layout"), reason: "frozen-specialized" },
  ]));
  writeFileSync(join(root, "docs/ux-ui/standard-coverage.json"), JSON.stringify({
    boundaryConsumers: boundary ? [{ owner: "src/specialized.tsx", component: "src/view.tsx" }] : [],
    routes: [{ verification: "pending" }],
    sources: [{ source: "src/view.tsx", scope: "standardize", presentation: "shared-opt-in-present", fingerprint: fingerprint(stale ? "old source" : content), verification: "pending" }],
  }));
  const result = spawnSync(process.execPath, [join(root, "scripts/ui/check-standard.mjs"), ...(complete ? ["--complete"] : [])], { encoding: "utf8" });
  if (result.error) throw result.error;
  return { status: result.status, output: result.stdout + result.stderr };
}

afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

describe("shared UI migration guard", () => {
  it("accepts an explicit aliased control and an unchanged specialized source", () => {
    const result = fixture();
    expect(result.status).toBe(0);
    expect(result.output).toContain("Migrated caller presentation guard passed.");
  });

  it("rejects missing appearance on an aliased imported control", () => {
    const result = fixture({ content: 'import { Button as Action } from "@/components/ui/button"; export const View = () => <Action>Open</Action>;' });
    expect(result.status).toBe(1);
    expect(result.output).toContain("Action lacks explicit appearance");
  });

  it("rejects native tables and legacy presentation classes", () => {
    const result = fixture({ content: 'export const View = () => <div className="form-actions"><table /></div>;' });
    expect(result.status).toBe(1);
    expect(result.output).toContain("native table bypasses shared presentation");
    expect(result.output).toContain("legacy presentation class");
  });

  it("rejects a changed frozen source instead of refreshing its baseline", () => {
    const result = fixture({ frozenChanged: true });
    expect(result.status).toBe(1);
    expect(result.output).toContain("src/specialized.tsx: frozen boundary changed");
  });

  it("rejects an ordinary component reached across a specialized boundary", () => {
    const result = fixture({ boundary: true });
    expect(result.status).toBe(1);
    expect(result.output).toContain("src/specialized.tsx: src/view.tsx needs a legacy boundary adapter");
  });

  it("rejects stale source inventories", () => {
    const result = fixture({ stale: true });
    expect(result.status).toBe(1);
    expect(result.output).toContain("inventory is stale; regenerate coverage");
  });

  it("keeps full browser/state acceptance separate from the source guard", () => {
    const result = fixture({ complete: true });
    expect(result.status).toBe(1);
    expect(result.output).toContain("component/page sources lack current state acceptance");
    expect(result.output).toContain("routes lack current browser acceptance");
  });
});
