import { afterEach, describe, expect, it } from "vitest";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createHash } from "node:crypto";
import { attachmentsOf, collectInvestorReport, generateInvestorReport, type InvestorManifest } from "../../scripts/testing/investor-report";
import { investorCases, investorGroups, investorJourneys, investorShots } from "../../scripts/testing/investor-selection";
const directories: string[] = [];
afterEach(async () => { await Promise.all(directories.splice(0).map(directory => rm(directory, { recursive: true, force: true }))); });
async function fixture() {
  const directory = await mkdtemp(join(tmpdir(), "erpx-investor-")); directories.push(directory);
  await mkdir(join(directory, "images"));
  const png = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jP1sAAAAASUVORK5CYII=", "base64");
  const manifest: InvestorManifest = { version: 1, createdAt: "2026-10-08T00:00:00Z", runtimeFingerprint: "runtime", verificationFingerprint: "verification", complete: true, problems: [], cases: investorCases.map(item => ({ ...item, passed: true })), images: investorShots.map(shot => ({ id: shot.id, suite: investorJourneys.find(item => item.id === shot.journey)!.suite, test: investorJourneys.find(item => item.id === shot.journey)!.title, project: shot.project, image: `images/investor-${shot.id}-${shot.project}.png`, sha256: createHash("sha256").update(png).digest("hex"), capturedAt: "2026-10-08T00:00:00Z", route: "/holdings/simulation/home", viewport: { width: 1440, height: 900 } })) };
  await Promise.all(manifest.images.map(image => writeFile(join(directory, image.image), png)));
  await writeFile(join(directory, "manifest.json"), JSON.stringify(manifest));
  return { directory, manifest };
}
describe("investor evidence packaging", () => {
  it("balances the story and regenerates an offline gallery from 30 intact checkpoints", async () => {
    for (const group of investorGroups) expect(investorShots.filter(shot => shot.group === group.id && shot.project === "desktop")).toHaveLength(8);
    expect(investorShots.filter(shot => shot.project === "mobile")).toHaveLength(6);
    const { directory } = await fixture();
    expect(await generateInvestorReport(directory)).toBe(true);
    const html = await readFile(join(directory, "index.html"), "utf8");
    expect(html).toContain("30 checkpoint images");
    expect(html).toContain("Visual review is pending");
    expect(html).not.toMatch(/<script[^>]+src=|<link[^>]+href=|fetch\(/);
    expect(await readFile(join(directory, "report.html"), "utf8")).toContain("deterministic responses");
  });
  it("marks a missing or altered image incomplete instead of silently retaining a previous presentation", async () => {
    const { directory, manifest } = await fixture();
    await rm(join(directory, manifest.images[0].image));
    await writeFile(join(directory, manifest.images[1].image), "altered");
    expect(await generateInvestorReport(directory)).toBe(false);
    const html = await readFile(join(directory, "index.html"), "utf8");
    expect(html).toContain("Incomplete capture");
    expect(html).toContain("Checkpoint unavailable");
  });
  it("rejects skipped, failed and duplicate case results even when attachments exist", () => {
    const result = (status: string) => ({ suites: [{ specs: [{ title: "journey", tests: [{ projectName: "desktop", expectedStatus: "passed", status: "expected", results: [{ status, attachments: [] }] }] }] }] });
    for (const status of ["skipped", "failed", "timedOut"]) expect(() => attachmentsOf(result(status), "journey", "desktop")).toThrow("EXACT_PASSED_RESULT");
    const passed = result("passed");
    expect(attachmentsOf(passed, "journey", "desktop")).toEqual([]);
    expect(() => attachmentsOf({ suites: [...passed.suites, ...passed.suites] }, "journey", "desktop")).toThrow("EXACT_PASSED_RESULT");
  });
  it("keeps unavailable cases incomplete and rejects a review from another revision", async () => {
    const { directory } = await fixture();
    const manifest = await collectInvestorReport(join(directory, "missing-runner-evidence"), directory);
    expect(manifest.complete).toBe(false);
    expect(manifest.images).toHaveLength(0);
    expect(manifest.cases.every(item => !item.passed)).toBe(true);
    expect(await generateInvestorReport(directory)).toBe(false);
    await writeFile(join(directory, "review.json"), JSON.stringify({ runtimeFingerprint: "other", verificationFingerprint: "other", reviewedImages: [], strengths: [], findings: [] }));
    await expect(generateInvestorReport(directory)).rejects.toThrow("REVIEW_REVISION_MISMATCH");
  });
});
