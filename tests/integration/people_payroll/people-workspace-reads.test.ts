import { mkdir, writeFile } from "node:fs/promises";
import { beforeAll, expect, it, vi } from "vitest";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { archivedPeopleDirectory, holdingPeopleDirectory } from "@erpx/server-organization/public/holding-people-directory";
import { organizationAccessData } from "@erpx/server-organization/public/organization-access";
import { peopleWorkspaceData } from "@erpx/server-people_payroll/public/people-workspace";
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
}));
beforeAll(async () => { await applyMigrations(env.H001_DB, env.STAFF_TEST_MIGRATIONS); });

async function previousWorkspaceRead(actor: WorkContext) {
  const [directory, archivedPeople] = await Promise.all([
    holdingPeopleDirectory(actor), archivedPeopleDirectory(actor),
  ]);
  const accessOrganization = await organizationAccessData(env.H001_DB, actor);
  return { directory, archivedPeople, accessOrganization };
}

it("loads archived profiles only when HR opens Archived and keeps the full current graph", async () => {
  const fixture = await seedExpenseCompany(env.H001_DB, env.H001_FILES);
  const archivedId = `${fixture.company}-archived-profile`;
  await env.H001_DB.prepare(`INSERT INTO hr_people
    (id,company_id,legal_name,country_code,status,created_by,created_at,updated_at)
    VALUES(?,?,?,'TH','archived','fixture',1,1)`).bind(archivedId, fixture.company, "Archived colleague").run();
  const baseline = await measureReads(() => previousWorkspaceRead(fixture.owner));
  const current = await measureReads(() => peopleWorkspaceData(fixture.owner));
  const archived = await measureReads(() => peopleWorkspaceData(fixture.owner, { status: "archived" }));
  expect(current.result.directory).toEqual(baseline.result.directory);
  expect(current.result.accessOrganization).toEqual(baseline.result.accessOrganization);
  expect(current.result.archivedPeople).toEqual([]);
  expect(current.metrics.queries).toBe(baseline.metrics.queries - 2);
  expect(archived.result.archivedPeople).toEqual(baseline.result.archivedPeople);
  expect(archived.result.archivedPeople.map(person => person.personId)).toContain(archivedId);
  expect(archived.result.accessOrganization).toEqual(current.result.accessOrganization);
  expect((await peopleWorkspaceData(fixture.claimant, { status: "archived" })).archivedPeople).toEqual([]);
  const measured = { fixture: "seedExpenseCompany + 1 archived profile", baseline: baseline.metrics,
    current: current.metrics, archived: archived.metrics,
    sectionBytes: {
      baseline: Buffer.byteLength(JSON.stringify(baseline.result)),
      current: Buffer.byteLength(JSON.stringify({ directory: current.result.directory,
        archivedPeople: current.result.archivedPeople, accessOrganization: current.result.accessOrganization })),
      archived: Buffer.byteLength(JSON.stringify({ directory: archived.result.directory,
        archivedPeople: archived.result.archivedPeople, accessOrganization: archived.result.accessOrganization })),
    },
  };
  expect(measured.sectionBytes.current).toBeLessThan(measured.sectionBytes.baseline);
  await mkdir(".scratch/performance", { recursive: true });
  await writeFile(".scratch/performance/people-workspace-sections.json", JSON.stringify(measured, null, 2));
});

it("skips the private arrangement graph for scoped HR and rechecks a revoked assignment", async () => {
  const fixture = await seedExpenseCompany(env.H001_DB, env.H001_FILES);
  const scoped = { ...fixture.owner, branchId: fixture.branch, departmentId: fixture.department,
    scope: { ...fixture.owner.scope, kind: "department" } } as WorkContext;
  const before = await measureReads(() => previousWorkspaceRead(scoped));
  const after = await measureReads(() => peopleWorkspaceData(scoped));
  expect(after.result.directory).toEqual(before.result.directory);
  expect(after.result.accessOrganization).toBeNull();
  expect(after.result.canManageStructure).toBe(false);
  expect(after.metrics.queries).toBeLessThanOrEqual(before.metrics.queries / 2);
  const scopedMeasurement = { baseline: before.metrics, current: after.metrics, sectionBytes: {
    baseline: Buffer.byteLength(JSON.stringify(before.result)),
    current: Buffer.byteLength(JSON.stringify({ directory: after.result.directory,
      archivedPeople: after.result.archivedPeople, accessOrganization: after.result.accessOrganization })),
  } };
  await mkdir(".scratch/performance", { recursive: true });
  await writeFile(".scratch/performance/people-workspace-scoped-sections.json", JSON.stringify(scopedMeasurement, null, 2));
  await env.H001_DB.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(), scoped.assignmentId).run();
  await expect(peopleWorkspaceData(scoped)).rejects.toMatchObject({ status: 403, code: "HOLDING_DIRECTORY_FORBIDDEN" });
});
