import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { env, applyMigrations } from "../../support/runtime-env";
import { assignmentApplies, isRoleValidAtScope } from "@erpx/server-identity_access/public/rules/workspace-access";
const mocked = vi.hoisted(() => ({ session: vi.fn(), work: vi.fn(), platform: vi.fn(), scope: vi.fn() }));
vi.mock("@erpx/server-identity_access/public/authorization", () => ({ currentSession: mocked.session, requireWorkContext: mocked.work, requirePlatformContext: mocked.platform, resolveWorkspaceScope: mocked.scope, assignmentApplies, isRoleValidAtScope }));
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({ holdingResources: async () => ({ write: env.H001_DB }), runtimeEnv: async () => env }));
import { accessDiagnosis } from "@erpx/server-identity_access/public/access-diagnosis";
let company: string, branch: string, department: string, user: string;
let from: string;
const db = env.H001_DB;
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); await applyMigrations(env.CONTROL_DB, env.CONTROL_TEST_MIGRATIONS); });
beforeEach(async () => {
  vi.clearAllMocks();
  company = `case-${crypto.randomUUID()}`; branch = `${company}-branch`; department = `${company}-team`; user = `${company}-user`;
  from = `/holdings/access-test/branches/${branch}/departments/team/home`;
  mocked.session.mockResolvedValue({ session: { user: { id: user, email: "current@example.test" } } });
  mocked.work.mockRejectedValue(new Error("FORBIDDEN")); mocked.platform.mockRejectedValue(new Error("FORBIDDEN"));
  mocked.scope.mockResolvedValue({ kind: "department", holdingId: "h001", holdingSlug: "access-test", holdingName: "Test", companyId: company, branchId: branch, departmentId: department });
  await db.batch([
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Test',?)").bind(company, company, company),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Test branch',?,?,?)").bind(branch, branch, company, company, branch),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'department','team','Test team',?,?,?)").bind(department, branch, company, branch),
  ]);
  await env.CONTROL_DB.prepare("INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES('h001','access-test','Test','H001_DB','H001_READ_DB','H001_FILES','active') ON CONFLICT DO NOTHING").run();
});
async function grant(options: { from?: number; until?: number; revoked?: number | null; node?: string } = {}) {
  await db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,scope_kind,scope_node_id) VALUES(?,?,'employee','[\"people_payroll\"]',?,?,?,?,?,'department',?)")
    .bind(crypto.randomUUID(), user, company, branch, options.from ?? Date.now() - 1000, options.until ?? Date.now() + 100000, options.revoked ?? null, options.node ?? department).run();
}
it.each(["expired", "revoked", "scheduled"] as const)("reads %s from stored assignments without changing access", async reason => {
  const now = Date.now();
  await grant(reason === "expired" ? { from: now - 2000, until: now - 1000 } : reason === "revoked" ? { revoked: now } : { from: now + 1000 });
  const before = await db.prepare("SELECT * FROM assignments WHERE user_id=?").bind(user).all();
  expect(await accessDiagnosis(from)).toEqual({ problem: reason, email: "current@example.test" });
  expect((await db.prepare("SELECT * FROM assignments WHERE user_id=?").bind(user).all()).results).toEqual(before.results);
});
it("distinguishes absent access from access to a different workspace", async () => {
  expect((await accessDiagnosis(from)).problem).toBe("no_access");
  mocked.scope.mockResolvedValue({ kind: "holding", holdingId: "h001", companyId: company });
  await grant(); expect((await accessDiagnosis("/holdings/access-test/home")).problem).toBe("scope");
});
it("reports an inactive branch only to its own member", async () => {
  await db.prepare("UPDATE organization_nodes SET status='suspended' WHERE id=?").bind(branch).run();
  mocked.work.mockRejectedValue(new Error("WORKSPACE_NOT_FOUND"));
  expect((await accessDiagnosis(from)).problem).toBe("workspace");
  await grant();
  // A member can see that an ancestor of their own department is inactive.
  expect((await accessDiagnosis(from)).problem).toBe("suspended");
  await db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,scope_kind,scope_node_id) VALUES(?,?,'employee','[]',?,?,?,?,'branch',?)").bind(crypto.randomUUID(), user, company, branch, Date.now()-1000, Date.now()+100000, branch).run();
  expect((await accessDiagnosis(from)).problem).toBe("suspended");
});
it.each([["COMPANY_REQUIRED", "company"], ["INVALID_ORGANIZATION_LINEAGE", "setup"], ["HOLDING_NOT_FOUND", "workspace"]])("explains %s without exposing organization details", async (error, expected) => {
  mocked.work.mockRejectedValue(new Error(error));
  expect((await accessDiagnosis(from)).problem).toBe(expected);
});
it("rechecks restored access and keeps feature permission hints separate", async () => {
  mocked.work.mockResolvedValue({});
  expect((await accessDiagnosis(from)).problem).toBe("available");
  expect((await accessDiagnosis(from, "capability")).problem).toBe("capability");
});
it("does not disclose an account or workspace when signed out", async () => {
  mocked.session.mockRejectedValue(new Error("UNAUTHENTICATED"));
  expect(await accessDiagnosis(from)).toEqual({ problem: "sign_in" });
  expect(mocked.work).not.toHaveBeenCalled();
});
it("keeps unexpected service failures out of the permission/not-found cases", async () => {
  const error = new Error("private database detail"); mocked.work.mockRejectedValue(error);
  await expect(accessDiagnosis(from)).rejects.toBe(error);
});
