import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import { archivedPeopleDirectory, holdingDirectorySelection, holdingPeopleDirectory } from "../../../src/server/modules/organization/public/holding-people-directory";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import { applyMigrations, env } from "../../support/runtime-env";

vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({
  holdingBindingBySlug: async (slug: string) => {
    if (slug === "test") return { id: "h001", slug, displayName: "Test Holding" };
    if (slug === "other") return { id: "h002", slug, displayName: "Other Holding" };
    throw new Error("HOLDING_NOT_FOUND");
  },
  holdingResources: async (holdingId: string) => {
    if (holdingId !== "h001") throw new Error("HOLDING_NOT_FOUND");
    return { write: env.H001_DB, files: env.H001_FILES };
  },
}));

const db = env.H001_DB;
let fixture: Awaited<ReturnType<typeof seedExpenseCompany>>;
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => { fixture = await seedExpenseCompany(db, env.H001_FILES); });

it("resolves the same active targets as the public Holding directory", async () => {
  const other = await seedExpenseCompany(db, env.H001_FILES);
  const positionId = `${fixture.company}-selected-position`;
  await db.prepare(`INSERT INTO organization_positions
    (id,company_id,branch_id,department_id,title,level_id,created_by,created_at,updated_at)
    VALUES(?,?,?,?,?,?,?,1,1)`)
    .bind(positionId, fixture.company, fixture.branch, fixture.department,
      "Selected position", `${fixture.company}-level-1`, "fixture").run();
  const directory = await holdingPeopleDirectory(fixture.claimant);
  for (const company of directory.companies) {
    for (const kind of ["company", "unassigned"] as const) {
      expect(await holdingDirectorySelection(fixture.claimant, { kind, id: company.id })).toEqual({ companyId: company.id });
    }
  }
  for (const node of directory.nodes.filter(node => node.kind !== "company")) {
    const kind = node.kind === "headquarters" ? "branch" : node.kind as "branch" | "department" | "team";
    expect(await holdingDirectorySelection(fixture.claimant, { kind, id: node.id })).toEqual({ companyId: node.companyId });
  }
  for (const item of directory.positions) {
    expect(await holdingDirectorySelection(fixture.claimant, { kind: "position", id: item.id })).toEqual({ companyId: item.companyId });
  }
  for (const item of directory.levels) {
    expect(await holdingDirectorySelection(fixture.claimant, { kind: "level", id: item.id })).toEqual({ companyId: item.companyId });
  }
  for (const person of directory.people) {
    expect(await holdingDirectorySelection(fixture.claimant, { kind: "person", id: person.engagementId })).toEqual({
      companyId: person.companyId,
      person: { id: person.engagementId, personId: person.personId, engagementId: person.engagementId, companyId: person.companyId },
    });
  }
  expect(await holdingDirectorySelection(fixture.claimant, { kind: "department", id: fixture.branch })).toBeNull();
  expect(await holdingDirectorySelection(fixture.claimant, { kind: "person", id: `person-${fixture.head.userId}` })).toBeNull();
  await db.prepare("UPDATE organization_nodes SET status='suspended' WHERE id=?").bind(other.company).run();
  expect(await holdingDirectorySelection(fixture.claimant, { kind: "person", id: `eng-${other.head.userId}` })).toBeNull();
});

it("keeps archived selections inside Company HR placement scope", async () => {
  const other = await seedExpenseCompany(db, env.H001_FILES);
  const personId = `person-${fixture.head.userId}`;
  const engagementId = `eng-${fixture.head.userId}`;
  for (const id of [personId, `person-${other.head.userId}`]) {
    await db.prepare("UPDATE hr_people SET status='archived' WHERE id=?").bind(id).run();
    await db.prepare("UPDATE hr_engagements SET status='ended',end_date='2020-12-31' WHERE person_id=?").bind(id).run();
  }
  const selection = { kind: "person" as const, id: engagementId };
  expect(await holdingDirectorySelection(fixture.owner, selection)).toBeNull();
  expect(await holdingDirectorySelection(fixture.claimant, selection, true)).toBeNull();
  const archived = await archivedPeopleDirectory(fixture.owner);
  expect(archived.some(person => person.engagementId === engagementId)).toBe(true);
  expect(await holdingDirectorySelection(fixture.owner, selection, true)).toEqual({ companyId: fixture.company,
    person: { id: engagementId, personId, engagementId, companyId: fixture.company },
  });
  expect(await holdingDirectorySelection(fixture.owner, { kind: "person", id: `eng-${other.head.userId}` }, true)).toBeNull();
  const scoped = { ...fixture.owner, branchId: fixture.branch, departmentId: fixture.department } as WorkContext;
  expect(await holdingDirectorySelection(scoped, selection, true)).not.toBeNull();
  expect(await holdingDirectorySelection({ ...scoped, departmentId: "other-department" }, selection, true)).toBeNull();
  await db.prepare(`INSERT INTO hr_engagements
    (id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at)
    SELECT ?,person_id,company_id,branch_id,department_id,worker_type,employment_type,'onboarding','2099-01-01',job_title,currency,created_by,created_at,updated_at
    FROM hr_engagements WHERE id=?`).bind(`${engagementId}-onboarding`, engagementId).run();
  expect(await holdingDirectorySelection(fixture.owner, selection, true)).toBeNull();
});

it("rechecks Holding, selected Company, role and assignment revocation for every target read", async () => {
  const selection = { kind: "person" as const, id: `eng-${fixture.head.userId}` };
  await expect(holdingDirectorySelection({ ...fixture.claimant, holdingSlug: "other" }, selection))
    .rejects.toMatchObject({ status: 403, code: "HOLDING_SCOPE_REQUIRED" });
  await expect(holdingDirectorySelection({ ...fixture.claimant, companyId: "other-company" }, selection))
    .rejects.toMatchObject({ status: 403, code: "HOLDING_SCOPE_REQUIRED" });
  await expect(holdingDirectorySelection({ ...fixture.claimant, role: "owner" }, selection))
    .rejects.toMatchObject({ status: 403, code: "HOLDING_DIRECTORY_FORBIDDEN" });
  expect(await holdingDirectorySelection(fixture.claimant, selection)).not.toBeNull();
  await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(), fixture.claimant.assignmentId).run();
  await expect(holdingDirectorySelection(fixture.claimant, selection))
    .rejects.toMatchObject({ status: 403, code: "HOLDING_DIRECTORY_FORBIDDEN" });
});
