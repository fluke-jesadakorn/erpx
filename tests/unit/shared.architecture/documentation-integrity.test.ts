import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { checkDocumentationLinks, trackedTemporaryFiles } from "../../../scripts/docs/documentation-integrity";

const roots: string[] = [];
const fixture = () => {
  const root = mkdtempSync(join(tmpdir(), "erpx-docs-"));
  roots.push(root);
  return { root, put: (file: string, text: string) => {
    mkdirSync(dirname(join(root, file)), { recursive: true });
    writeFileSync(join(root, file), text);
  } };
};
afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });

describe("documentation integrity", () => {
  it("checks arbitrary root Markdown and the current archive index without rewriting historical evidence", () => {
    const { root, put } = fixture();
    put("NEW-GUIDE.md", "[missing](missing.md)\n[anchor](README.md#absent)\n");
    put("README.md", "# Recovery\n");
    put("docs/archive/README.md", "[recovery](../../CUTOVER.md)\n");
    const evidence = "[old source](../../src/retired.rs)\n";
    for (const file of ["docs/archive/2026-09-02/README.md", "docs/testing/evidence/receipt.md", "docs/testing/protected.md"])
      put(file, evidence);
    const result = checkDocumentationLinks(root, new Set(["docs/testing/protected.md"]));
    expect(result.errors).toEqual([
      "NEW-GUIDE.md: missing local link missing.md",
      "NEW-GUIDE.md: missing anchor README.md#absent",
      "docs/archive/README.md: missing local link ../../CUTOVER.md",
    ]);
    put("NEW-GUIDE.md", "[recovery](README.md#recovery)\n");
    put("docs/archive/README.md", "[recovery](../../README.md#recovery)\n");
    expect(checkDocumentationLinks(root, new Set(["docs/testing/protected.md"])).errors).toEqual([]);
    for (const file of ["docs/archive/2026-09-02/README.md", "docs/testing/evidence/receipt.md", "docs/testing/protected.md"])
      expect(readFileSync(join(root, file), "utf8")).toBe(evidence);
  });

  it("retains historical-source exceptions and still rejects missing current sources", () => {
    const { root, put } = fixture();
    put("docs/testing/dated.md", "[source](../../src/old.ts#historical)\n");
    put("docs/current.md", "[source](../src/missing.ts)\n");
    const result = checkDocumentationLinks(root, new Set(), (document, file) =>
      document === "docs/testing/dated.md" && file === "src/old.ts");
    expect(result.errors).toEqual(["docs/current.md: missing local link ../src/missing.ts"]);
  });

  it("reports tracked temporary files while allowing ignored private files and unstaged deletions", () => {
    const { root, put } = fixture();
    execFileSync("git", ["init", "--quiet"], { cwd: root });
    put(".gitignore", ".scratch/\n.tmp/\n");
    put(".scratch/closed/map.md", "old plan\n");
    put(".tmp/old.md", "temporary\n");
    put(".scratch/dev/private.txt", "fixture private file\n");
    execFileSync("git", ["add", "--force", ".scratch/closed/map.md", ".tmp/old.md"], { cwd: root });
    expect(trackedTemporaryFiles(root)).toEqual([".scratch/closed/map.md", ".tmp/old.md"]);
    rmSync(join(root, ".scratch/closed/map.md"));
    rmSync(join(root, ".tmp/old.md"));
    expect(trackedTemporaryFiles(root)).toEqual([]);
    expect(readFileSync(join(root, ".scratch/dev/private.txt"), "utf8")).toBe("fixture private file\n");
  });
});
