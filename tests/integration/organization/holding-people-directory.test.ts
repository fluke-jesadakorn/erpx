import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { env, applyMigrations } from "../../support/runtime-env";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import { archivedPeopleDirectory, holdingPeopleDirectory } from "../../../src/server/modules/organization/public/holding-people-directory";

vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({
  holdingBindingBySlug: async (slug: string) => {
    if (slug === "test") return { id: "h001", slug, displayName: "Test Holding" };
    if (slug === "other") return { id: "h002", slug, displayName: "Other Holding" };
    throw new Error("HOLDING_NOT_FOUND");
  },
  holdingResources: async (holdingId: string) => {
    if (holdingId === "h001") return { write: env.H001_DB };
    if (holdingId === "h002") return { write: env.H002_DB };
    throw new Error("HOLDING_NOT_FOUND");
  },
}));

const db = env.H001_DB;
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });

let first: Awaited<ReturnType<typeof seedExpenseCompany>>;
let second: Awaited<ReturnType<typeof seedExpenseCompany>>;
beforeEach(async () => {
  first = await seedExpenseCompany(db, env.H001_FILES);
  second = await seedExpenseCompany(db, env.H001_FILES);
});

it("returns active people across Companies with only explicit work contacts", async () => {
  const actor = first.owner;
  const personId = `person-${first.claimant.userId}`;
  const engagementId = `eng-${first.claimant.userId}`;
  const positionId = `${first.company}-directory-position`;
  await db.prepare(`INSERT INTO organization_positions
    (id,company_id,branch_id,department_id,title,level_id,created_by,created_at,updated_at)
    VALUES(?,?,?,?,?,?,?,1,1)`)
    .bind(positionId, first.company, first.branch, first.department,
      "Service coordinator", `${first.company}-level-1`, "fixture").run();
  await db.prepare("UPDATE hr_engagements SET position_id=? WHERE id=?")
    .bind(positionId, engagementId).run();
  await db.prepare(`UPDATE hr_people SET email=?,phone=?,work_email=?,work_phone=? WHERE id=?`)
    .bind("private@example.test", "private-number", "work@example.test", "+66 2 123 4567", personId).run();
  await db.prepare(`INSERT INTO hr_engagements
    (id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,
     job_title,currency,created_by,created_at,updated_at)
    VALUES(?,?,?,?,?,'contractor','contractor','active','2020-01-01','Second assignment','THB','fixture',1,1)`)
    .bind(`${engagementId}-second`, personId, first.company, first.branch, first.department).run();
  const archivedPerson = `person-${first.head.userId}`;
  await db.prepare("UPDATE hr_people SET status='archived' WHERE id=?").bind(archivedPerson).run();
  await db.prepare("UPDATE hr_engagements SET status='ended',end_date='2020-12-31' WHERE person_id=?")
    .bind(`person-${first.tie.userId}`).run();
  const accountOnly = await first.member("account-only", "employee", 1, first.department, false);

  const directory = await holdingPeopleDirectory(actor);
  expect(directory.companies.map(company => company.id)).toContain(first.company);
  expect(directory.companies.map(company => company.id)).toContain(second.company);
  expect(directory.nodes.some(node => node.companyId === second.company && node.kind === "branch")).toBe(true);
  expect(directory.positions.find(position => position.id === positionId)).toMatchObject({
    companyId: first.company, title: "Service coordinator", levelId: `${first.company}-level-1`,
  });
  expect(directory.levels.some(level => level.id === `${first.company}-level-1`)).toBe(true);
  expect(directory.people.some(person => person.companyId === second.company)).toBe(true);
  expect(directory.people.filter(person => person.personId === personId).map(person => person.engagementId))
    .toEqual([engagementId]);
  expect(directory.people.find(person => person.engagementId === engagementId)).toMatchObject({
    legalName: "claimant", positionId, jobTitle: "Service coordinator",
    workEmail: "work@example.test", workPhone: "+66 2 123 4567",
  });
  expect(directory.people.some(person => person.personId === archivedPerson)).toBe(false);
  expect(directory.people.some(person => person.personId === `person-${first.tie.userId}`)).toBe(false);
  expect(directory.people.some(person => person.personId === `person-${accountOnly.userId}`)).toBe(false);
  expect(Object.keys(directory.people[0]).sort()).toEqual([
    "branchId", "companyId", "departmentId", "engagementId", "id", "jobTitle", "legalName",
    "personId", "positionId", "userId", "workEmail", "workPhone",
  ].sort());
  const payload = JSON.stringify(directory);
  expect(payload).not.toContain(accountOnly.userId);
  expect(payload).not.toContain("private@example.test");
  expect(payload).not.toContain("private-number");
  expect(payload).not.toContain("base_amount_minor");
});

it("rejects a Holding slug mismatch and a revoked assignment before reading people", async () => {
  await expect(holdingPeopleDirectory({ ...first.owner, holdingSlug: "other" }))
    .rejects.toMatchObject({ code: "HOLDING_SCOPE_REQUIRED", status: 403 });
  await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(), first.owner.assignmentId).run();
  await expect(holdingPeopleDirectory(first.owner))
    .rejects.toMatchObject({ code: "HOLDING_DIRECTORY_FORBIDDEN", status: 403 });
});

it("uses the selected Holding database and rejects an assignment copied from another Holding", async () => {
  await applyMigrations(env.H002_DB, env.STAFF_TEST_MIGRATIONS);
  const other = await seedExpenseCompany(env.H002_DB, env.H002_FILES);
  await env.H002_DB.prepare("UPDATE hr_people SET work_email=? WHERE id=?")
    .bind("other-holding@example.test", `person-${other.claimant.userId}`).run();
  const otherActor = {
    ...other.owner,
    holdingId: "h002",
    holdingSlug: "other",
    scope: { ...other.owner.scope, holdingId: "h002", holdingSlug: "other" },
  };
  const ownDirectory = await holdingPeopleDirectory(first.owner);
  const otherDirectory = await holdingPeopleDirectory(otherActor);
  expect(ownDirectory.companies.some(company => company.id === other.company)).toBe(false);
  expect(otherDirectory.companies.map(company => company.id)).toEqual([other.company]);
  expect(JSON.stringify(ownDirectory)).not.toContain("other-holding@example.test");
  expect(otherDirectory.people.some(person => person.workEmail === "other-holding@example.test")).toBe(true);

  const forged = {
    ...first.owner,
    holdingId: "h002",
    holdingSlug: "other",
    scope: { ...first.owner.scope, holdingId: "h002", holdingSlug: "other" },
  };
  await expect(holdingPeopleDirectory(forged))
    .rejects.toMatchObject({ code: "HOLDING_DIRECTORY_FORBIDDEN", status: 403 });
});

it("limits archived and ended profiles to HR's selected Company and Department", async () => {
  const archivedId = `person-${first.head.userId}`;
  const endedId = `person-${first.tie.userId}`;
  const otherDepartmentId = `person-${first.reviewer.userId}`;
  const otherCompanyId = `person-${second.owner.userId}`;
  const noEngagementId = `${first.company}-archived-without-engagement`;
  await db.batch([
    db.prepare("UPDATE hr_engagements SET status='ended',end_date='2020-12-31' WHERE person_id=ANY(?::text[])")
      .bind(`{${[archivedId, endedId, otherDepartmentId, otherCompanyId].join(",")}}`),
    db.prepare("UPDATE hr_people SET status='archived' WHERE id=ANY(?::text[])")
      .bind(`{${[archivedId, otherDepartmentId, otherCompanyId].join(",")}}`),
    db.prepare("UPDATE hr_people SET email='private@example.test',phone='private-number',work_email='hr@example.test' WHERE id=?")
      .bind(archivedId),
    db.prepare(`INSERT INTO hr_people(id,company_id,legal_name,country_code,status,created_by,created_at,updated_at)
      VALUES(?,?,?,'TH','archived','fixture',1,1)`)
      .bind(noEngagementId, first.company, "Archived draft"),
  ]);

  await expect(archivedPeopleDirectory(first.claimant))
    .rejects.toMatchObject({ code: "PERMISSION_REQUIRED", status: 403 });

  const companyRows = await archivedPeopleDirectory(first.owner);
  expect(companyRows.map(row => row.personId)).toEqual(expect.arrayContaining([
    archivedId, endedId, otherDepartmentId, noEngagementId,
  ]));
  expect(companyRows.some(row => row.personId === otherCompanyId)).toBe(false);
  expect(companyRows.some(row => row.personId === `person-${first.claimant.userId}`)).toBe(false);
  expect(companyRows.find(row => row.personId === noEngagementId)).toMatchObject({
    id: `archived:${noEngagementId}`, engagementId: "", branchId: null, departmentId: null,
  });

  const departmentActor = {
    ...first.owner,
    branchId: first.branch,
    departmentId: first.department,
    scope: {
      kind: "department" as const,
      holdingId: first.owner.holdingId,
      holdingSlug: first.owner.holdingSlug,
      holdingName: first.owner.holdingName,
      companyId: first.company,
      branchId: first.branch,
      departmentId: first.department,
      departmentSlug: "test-department",
      departmentName: "Test Department",
    },
  };
  const departmentRows = await archivedPeopleDirectory(departmentActor);
  expect(departmentRows.map(row => row.personId).sort()).toEqual([archivedId, endedId].sort());
  expect(departmentRows.find(row => row.personId === archivedId)).toMatchObject({
    workEmail: "hr@example.test", workPhone: null,
  });
  const payload = JSON.stringify(departmentRows);
  expect(payload).not.toContain("private@example.test");
  expect(payload).not.toContain("private-number");
});
