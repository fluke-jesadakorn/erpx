import { mkdir, writeFile } from "node:fs/promises";
import { beforeAll, expect, it, vi } from "vitest";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import { holdingPeopleDirectory } from "@erpx/server-organization/public/holding-people-directory";
import { peopleExplorerDetail } from "@erpx/server-people_payroll/public/people-explorer-detail";
import { measureReads } from "@erpx/server-runtime/adapters/runtime/read-metrics";
import { applyMigrations, env } from "../../support/runtime-env";

vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  holdingBindingBySlug: async (slug: string) => {
    if (slug !== "test") throw new Error("HOLDING_NOT_FOUND");
    return { id: "h001", slug, displayName: "Test Holding" };
  },
  holdingResources: async (holdingId: string) => {
    if (holdingId !== "h001") throw new Error("HOLDING_NOT_FOUND");
    return { write: env.H001_DB, files: env.H001_FILES };
  },
  runtimeEnv: async () => env,
}));

beforeAll(async () => { await applyMigrations(env.H001_DB, env.STAFF_TEST_MIGRATIONS); });

it("halves ordinary People profile SQL without changing its response", async () => {
  const fixture = await seedExpenseCompany(env.H001_DB, env.H001_FILES);
  const selection = { kind: "person" as const, id: `eng-${fixture.head.userId}` };
  const samples = [];
  for (let round = 0; round < 20; round++) {
    // The previous read resolved a single target by loading the entire Holding.
    const baseline = await measureReads(async () => {
      const directory = await holdingPeopleDirectory(fixture.claimant);
      expect(directory.people.some(person => person.engagementId === selection.id)).toBe(true);
      return {
        selection, tab: "Profile", tabs: ["Profile"], privatePerson: null, organization: null,
        groups: [], editorNodes: [], staffOptions: null, lifecycle: null, history: [], branding: null, authority: null,
      };
    });
    const measured = await measureReads(() => peopleExplorerDetail(fixture.claimant, selection, "Profile"));
    expect(measured.result).toEqual(baseline.result);
    expect(measured.metrics.queries).toBeLessThanOrEqual(baseline.metrics.queries / 2);
    samples.push({ round,
      baseline: { ...baseline.metrics, bytes: Buffer.byteLength(JSON.stringify(baseline.result)) },
      targeted: { ...measured.metrics, bytes: Buffer.byteLength(JSON.stringify(measured.result)) },
    });
  }
  await mkdir(".scratch/performance", { recursive: true });
  const evidence = JSON.stringify({ fixture: "seedExpenseCompany", samples }, null, 2);
  const observedAt = new Date().toISOString().replaceAll(":", "-");
  await writeFile(`.scratch/performance/people-profile-comparison-${observedAt}.json`, evidence);
  await writeFile(".scratch/performance/people-profile-comparison.json", evidence);
  expect(samples.every(sample => sample.baseline.queries === 6 && sample.targeted.queries === 2)).toBe(true);
});
