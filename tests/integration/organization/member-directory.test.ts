import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import { companyMembers } from "../../../src/server/modules/organization/public/member-directory";
import { memberProfileDetails } from "../../../src/server/modules/organization/public/member-profile-view";

vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({ runtimeEnv: async () => env, holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }) }));
beforeAll(async () => { await applyMigrations(env.CONTROL_DB, env.CONTROL_TEST_MIGRATIONS); await applyMigrations(env.H001_DB, env.STAFF_TEST_MIGRATIONS); });
let fixture: Awaited<ReturnType<typeof seedExpenseCompany>>;
beforeEach(async () => {
  fixture = await seedExpenseCompany(env.H001_DB, env.H001_FILES);
  for (const actor of [fixture.owner, fixture.claimant]) await env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,"createdAt","updatedAt") VALUES(?,?,?,?,?)').bind(actor.userId, actor.name, `${actor.userId}@example.test`, Date.now(), Date.now()).run();
});

it("keeps colleague contact private for staff while exposing own contact and verified Person links", async () => {
  const rows = await companyMembers(fixture.claimant);
  expect(rows.find(row => row.id === fixture.claimant.userId)?.email).toBe(`${fixture.claimant.userId}@example.test`);
  expect(rows.find(row => row.id === fixture.owner.userId)?.email).toBe("");
  expect(rows.find(row => row.id === fixture.claimant.userId)?.personId).toBeTruthy();
});

it("removes revoked accounts for ordinary staff and retains HR historical access", async () => {
  await env.H001_DB.prepare("UPDATE assignments SET revoked_at=? WHERE company_id=? AND user_id=?").bind(Date.now(), fixture.company, fixture.owner.userId).run();
  expect((await companyMembers(fixture.claimant)).some(row => row.id === fixture.owner.userId)).toBe(false);
  const hr = await fixture.member("directory-hr", "hr_admin");
  expect((await companyMembers(hr)).some(row => row.id === fixture.owner.userId)).toBe(true);
});

it("uses reviewed identity links before email and allows email fallback only in beta HR", async () => {
  const hr = await fixture.member("directory-hr", "hr_admin");
  const person = await env.H001_DB.prepare("SELECT id FROM hr_people WHERE company_id=? AND user_id=?").bind(fixture.company, fixture.claimant.userId).first<string>("id");
  if (!person) throw new Error("DIRECTORY_PERSON_FIXTURE_REQUIRED");
  await env.H001_DB.prepare("UPDATE hr_people SET user_id=NULL,email=? WHERE id=?").bind(`${fixture.claimant.userId}@example.test`, person).run();
  expect((await companyMembers(hr)).find(row => row.id === fixture.claimant.userId)?.personId).toBeNull();
  expect((await companyMembers({ ...hr, beta: true })).find(row => row.id === fixture.claimant.userId)?.personId).toBe(person);
  await env.H001_DB.prepare("INSERT INTO iam_members(company_id,user_id,principal_id,person_id) VALUES(?,?,?,?)").bind(fixture.company, fixture.claimant.userId, `local:${fixture.claimant.userId}`, person).run();
  expect((await companyMembers(hr)).find(row => row.id === fixture.claimant.userId)?.personId).toBe(person);
});

it("reads only the authorized member's level history through the Organization panel owner and preserves newest-first ordering", async () => {
  const actor = fixture.claimant, personId = `person-${actor.userId}`, engagementId = `eng-${actor.userId}`;
  const latest = `member-history-latest-${crypto.randomUUID()}`, earlier = `member-history-earlier-${crypto.randomUUID()}`, unrelated = `member-history-unrelated-${crypto.randomUUID()}`;
  for (const [id, engagement, level, reason, at] of [[earlier, engagementId, 1, "Earlier level", 100], [latest, engagementId, 5, "Latest level", 200], [unrelated, `eng-${fixture.owner.userId}`, 10, "Private unrelated history", 300]] as const)
    await env.H001_DB.prepare("INSERT INTO staff_level_history(id,engagement_id,level_id,actor_id,reason,command_id,created_at) VALUES(?,?,?,?,?,?,?)")
      .bind(id, engagement, `${fixture.company}-level-${level}`, fixture.owner.userId, reason, id, at).run();
  const panel = await memberProfileDetails(actor, { id: actor.userId, name: actor.name, email: `${actor.userId}@example.test`, personId });
  expect(panel.person).toMatchObject({ id: personId, engagement_id: engagementId });
  expect(panel.history.map(row => ({ id: row.id, level: row.level_name, reason: row.reason }))).toEqual([
    { id: latest, level: "Level 5", reason: "Latest level" }, { id: earlier, level: "Level 1", reason: "Earlier level" },
  ]);
  expect(panel.manage).toBe(false); expect(panel.nodes).toEqual([]); expect(panel.organization).toBeUndefined();
  expect(panel.canManageAccess).toBe(false); expect(panel.canRevokeCompany).toBe(false);
  const other = await seedExpenseCompany(env.H001_DB, env.H001_FILES);
  await expect(memberProfileDetails(actor, { id: other.owner.userId, name: other.owner.name, email: "", personId: `person-${other.owner.userId}` }))
    .rejects.toMatchObject({ status: 404, code: "PERSON_NOT_FOUND" });
});
