import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { env, applyMigrations } from "./runtime-env";
import type { AccountAccessContext, WorkAssignment } from "../src/server/authorization";
import type { WorkspaceChoice } from "../src/lib/workspace-choice";
import { assignmentApplies } from "../src/lib/workspace-access";
import { selectWorkRole, readWorkSelection } from "../src/server/work-role-store";

const dependencies = vi.hoisted(() => ({
  session: vi.fn(), access: vi.fn(), choices: vi.fn(), narrowChoices: vi.fn(), manage: vi.fn(), platform: vi.fn(),
}));
vi.mock("../src/server/authorization", () => ({
  currentSession: dependencies.session, requireAccountAccess: dependencies.access,
  canEnterManageErpx: dependencies.manage, requirePlatformContext: dependencies.platform,
  assignmentApplies,
}));
vi.mock("../src/server/workspace-choices", () => ({ workspaceChoices: dependencies.choices, workspaceChoicesForAssignment: dependencies.narrowChoices }));
vi.mock("../src/server/runtime", () => ({ holdingResources: async () => ({ write: env.H001_DB }) }));
import { accountWorkContext } from "../src/server/account-work-context";

beforeAll(() => applyMigrations(env.H001_DB, env.STAFF_TEST_MIGRATIONS));
beforeEach(() => {
  vi.clearAllMocks();
  dependencies.manage.mockResolvedValue(false);
});

async function fixture(roles: ("holding_owner" | "employee")[]) {
  const companyId = crypto.randomUUID(), userId = crypto.randomUUID(), sessionId = crypto.randomUUID();
  await env.H001_DB.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Context company',?)")
    .bind(companyId, companyId, companyId).run();
  const assignments: WorkAssignment[] = [];
  for (const role of roles) {
    const id = crypto.randomUUID();
    await env.H001_DB.prepare("INSERT INTO assignments(id,user_id,role,company_id,modules_json,scope_kind,valid_from,valid_until) VALUES(?,?,?,?,'[]','holding',0,9999999999999)")
      .bind(id, userId, role, companyId).run();
    assignments.push({ id, role, companyId, branchId: null, modules: [], scopeKind: "holding", scopeNodeId: null, version: 1 });
  }
  const access: AccountAccessContext = {
    sessionId, userId, expiresAt: Date.now() + 60_000, name: "New account", email: "context@example.test",
    scope: { kind: "holding", holdingId: "h001", holdingSlug: "acme", holdingName: "Acme", companyId },
    assignments, requestHeaders: new Headers(),
  };
  const choices: WorkspaceChoice[] = [{ kind: "holding", holdingSlug: "acme", label: "Acme", companies: [
    { id: companyId, label: "Context company", destination: "/holdings/acme/home", branches: [], departments: [] },
  ] }];
  dependencies.session.mockResolvedValue({ session: { user: { id: userId, name: access.name, email: access.email } } });
  dependencies.access.mockResolvedValue(access);
  dependencies.choices.mockResolvedValue(choices);
  dependencies.narrowChoices.mockResolvedValue(choices);
  const choose = (assignment: WorkAssignment) => selectWorkRole(env.H001_DB, {
    sessionId, userId, companyId, assignmentId: assignment.id, assignmentVersion: 1, revision: 0, expiresAt: access.expiresAt,
  });
  return { access, assignments, choices, choose };
}

it("inspects the original company and returns the same full page with new authority", async () => {
  const f = await fixture(["holding_owner", "employee"]);
  await f.choose(f.assignments[1]);
  const from = "/holdings/acme/people?search=Mali&tab=organization#person-42";
  const state = await accountWorkContext({ from, companyId: f.access.scope.companyId });
  expect(dependencies.access).toHaveBeenCalledWith({ kind: "holding", holdingSlug: "acme" }, { companyId: f.access.scope.companyId });
  expect(state).toMatchObject({ from, status: "ready", canUseWorkspace: true, requiresWorkspaceSelection: false,
    user: { id: f.access.userId }, context: { userId: f.access.userId, companyId: f.access.scope.companyId, assignmentId: f.assignments[1].id, revision: 1 } });
  expect((await readWorkSelection(env.H001_DB, f.access.sessionId, f.access.userId, f.access.scope.companyId))?.revision).toBe(1);
});

it("asks for a role in the same workspace without choosing one among multiple grants", async () => {
  const f = await fixture(["holding_owner", "employee"]);
  const state = await accountWorkContext({ from: "/holdings/acme/people", companyId: f.access.scope.companyId });
  expect(state).toMatchObject({ status: "role-required", canUseWorkspace: false, requiresWorkspaceSelection: false, assignmentId: null });
  expect(await readWorkSelection(env.H001_DB, f.access.sessionId, f.access.userId, f.access.scope.companyId)).toBeNull();
});

it("offers workspace selection when the saved role belongs to another scope, preserving its revision", async () => {
  const f = await fixture(["employee"]), branchId = crypto.randomUUID();
  await env.H001_DB.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Branch',?,?,?)")
    .bind(branchId, branchId, f.access.scope.companyId, f.access.scope.companyId, branchId).run();
  await env.H001_DB.prepare("UPDATE assignments SET scope_kind='branch',scope_node_id=?,branch_id=? WHERE id=?")
    .bind(branchId, branchId, f.assignments[0].id).run();
  Object.assign(f.assignments[0], { scopeKind: "branch", scopeNodeId: branchId, branchId });
  const otherCompany = { id: "another-company", label: "Another company", destination: "/holdings/acme/home", branches: [], departments: [] };
  if (f.choices[0].kind === "holding") f.choices[0].companies.push(otherCompany);
  const branchChoice = { id: branchId, label: "Branch", branches: [{ kind: "branch", id: branchId, label: "Branch", destination: `/holdings/acme/branches/${branchId}/home`, departments: [] }], departments: [] };
  dependencies.narrowChoices.mockResolvedValue([{ kind: "holding", holdingSlug: "acme", label: "Acme", companies: [{ ...branchChoice, id: f.access.scope.companyId }] }]);
  await f.choose(f.assignments[0]);
  const state = await accountWorkContext({ from: "/holdings/acme/people?view=team#list", companyId: f.access.scope.companyId });
  expect(state).toMatchObject({ status: "workspace-required", requiresWorkspaceSelection: true, canUseWorkspace: false, context: null, revision: 1 });
  expect(state.current).toMatchObject({ kind: "holding", companyId: f.access.scope.companyId });
  expect(state.choices[0]).toMatchObject({ companies: [{ id: f.access.scope.companyId, branches: [{ id: branchId }] }, otherCompany] });
  expect(state.choices[0].kind === "holding" && state.choices[0].companies[0].destination).toBeUndefined();
  expect((await readWorkSelection(env.H001_DB, f.access.sessionId, f.access.userId, f.access.scope.companyId))?.assignment_id).toBe(f.assignments[0].id);
});

it("offers the new account's workspaces without selecting a company it cannot use at the original URL", async () => {
  const f = await fixture([]);
  const state = await accountWorkContext({ from: "/holdings/acme/people", companyId: f.access.scope.companyId });
  expect(state).toMatchObject({ status: "workspace-required", requiresWorkspaceSelection: true, canUseWorkspace: false, context: null });
  expect(state.choices).toEqual(f.choices);
  expect(await readWorkSelection(env.H001_DB, f.access.sessionId, f.access.userId, f.access.scope.companyId)).toBeNull();
});

it("keeps a recovery context when no work role is assigned", async () => {
  const f = await fixture([]);
  dependencies.choices.mockResolvedValue([]);
  const state = await accountWorkContext({ from: "/holdings/acme/people", companyId: f.access.scope.companyId });
  expect(state).toMatchObject({ status: "forbidden", requiresWorkspaceSelection: false, assignments: [], context: null, user: { id: f.access.userId } });
});

it("only initializes an unambiguous role that covers the current workspace", async () => {
  const f = await fixture(["employee"]);
  const state = await accountWorkContext({ from: "/holdings/acme/home", companyId: f.access.scope.companyId });
  expect(state).toMatchObject({ status: "ready", assignmentId: f.assignments[0].id, revision: 1 });
  const audit = await env.H001_DB.prepare("SELECT action FROM audit_events WHERE actor_id=?").bind(f.access.userId).all();
  expect(audit.results).toEqual([{ action: "work.role.select" }]);
});

it("uses the login error for an expired session", async () => {
  await fixture([]);
  dependencies.session.mockRejectedValue(new Error("UNAUTHENTICATED"));
  await expect(accountWorkContext({ from: "/holdings/acme/people" })).rejects.toThrow("UNAUTHENTICATED");
  expect(dependencies.access).not.toHaveBeenCalled();
});

it.each(["https://outside.test/holdings/acme/home", "//outside.test/holdings/acme/home", "/api/v1/holdings/acme/people", "/holdings/acme/%2foutside"])("rejects an unsafe page %s before reading context", async from => {
  await expect(accountWorkContext({ from })).rejects.toMatchObject({ code: "INVALID_WORKSPACE", status: 422 });
  expect(dependencies.session).not.toHaveBeenCalled();
});
