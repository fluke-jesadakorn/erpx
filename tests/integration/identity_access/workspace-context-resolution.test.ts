import { beforeAll, expect, it, vi } from "vitest";
import { env, applyMigrations } from "../../support/runtime-env";
import { rememberWorkspace } from "../../../src/server/modules/work_projection/public/workspace-preferences";

const request = vi.hoisted(() => ({ headers: new Headers(), session: vi.fn() }));
vi.mock("next/headers", () => ({ headers: async () => request.headers }));
vi.mock("../../../src/server/modules/identity_access/public/auth", () => ({ createAuth: async () => ({ api: { getSession: request.session } }) }));
vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({
  runtimeEnv: async () => env,
  holdingResources: async () => ({ write: env.H001_DB }),
  holdingBindingBySlug: async (slug: string) => ({ id: "h001", slug, displayName: "Acme" }),
}));
import { resolveWorkspaceScope } from "../../../src/server/modules/identity_access/public/authorization";

beforeAll(async () => {
  await Promise.all([
    applyMigrations(env.H001_DB, env.STAFF_TEST_MIGRATIONS),
    applyMigrations(env.CONTROL_DB, env.CONTROL_TEST_MIGRATIONS),
  ]);
});

async function fixture() {
  const userId = crypto.randomUUID(), original = crypto.randomUUID(), preferred = crypto.randomUUID();
  for (const id of [original, preferred]) {
    await env.H001_DB.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Context company',?)")
      .bind(id, id, id).run();
  }
  await env.H001_DB.prepare("INSERT INTO assignments(id,user_id,role,company_id,modules_json,scope_kind,valid_from,valid_until) VALUES(?,?,'holding_owner',?,'[]','holding',0,9999999999999)")
    .bind(crypto.randomUUID(), userId, preferred).run();
  request.session.mockResolvedValue({ user: { id: userId }, session: { id: crypto.randomUUID(), expiresAt: new Date(Date.now() + 60_000) } });
  request.headers = new Headers({ host: "localhost:3100" });
  return { userId, original, preferred };
}

it("keeps the browser's original company when the new account remembers another company", async () => {
  const f = await fixture();
  await rememberWorkspace(env.CONTROL_DB, f.userId, "/holdings/acme/home", { holdingId: "h001", companyId: f.preferred });
  request.headers.set("cookie", `erpx_company_h001=${f.original}`);
  expect(await resolveWorkspaceScope({ kind: "holding", holdingSlug: "acme" })).toMatchObject({ companyId: f.original });
});

it("inspects the explicit original company even when cookies and stored preferences point elsewhere", async () => {
  const f = await fixture();
  await rememberWorkspace(env.CONTROL_DB, f.userId, "/holdings/acme/home", { holdingId: "h001", companyId: f.preferred });
  request.headers.set("cookie", `erpx_company_h001=${f.preferred}`);
  request.headers.set("x-erpx-context-company", f.preferred);
  expect(await resolveWorkspaceScope({ kind: "holding", holdingSlug: "acme" }, { companyId: f.original }))
    .toMatchObject({ companyId: f.original });
});

it("requires a workspace choice instead of replacing an unavailable browser company", async () => {
  await fixture();
  request.headers.set("cookie", `erpx_company_h001=${crypto.randomUUID()}`);
  await expect(resolveWorkspaceScope({ kind: "holding", holdingSlug: "acme" })).rejects.toThrow("COMPANY_REQUIRED");
});

it("uses account memory only when this browser has no company choice", async () => {
  const f = await fixture();
  await rememberWorkspace(env.CONTROL_DB, f.userId, "/holdings/acme/home", { holdingId: "h001", companyId: f.preferred });
  expect(await resolveWorkspaceScope({ kind: "holding", holdingSlug: "acme" })).toMatchObject({ companyId: f.preferred });
});

it("requires an explicit company choice when a new browser has multiple eligible companies", async () => {
  const f = await fixture();
  await env.H001_DB.prepare("INSERT INTO assignments(id,user_id,role,company_id,modules_json,scope_kind,valid_from,valid_until) VALUES(?,?,'holding_owner',?,'[]','holding',0,9999999999999)")
    .bind(crypto.randomUUID(), f.userId, f.original).run();
  await expect(resolveWorkspaceScope({ kind: "holding", holdingSlug: "acme" })).rejects.toThrow("COMPANY_REQUIRED");
});

it("cannot resolve a branch from a different explicit original company", async () => {
  const f = await fixture(), branch = crypto.randomUUID();
  await env.H001_DB.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Branch',?,?,?)")
    .bind(branch, branch, f.preferred, f.preferred, branch).run();
  await expect(resolveWorkspaceScope({ kind: "branch", holdingSlug: "acme", branchSlug: branch }, { companyId: f.original }))
    .rejects.toThrow("WORKSPACE_NOT_FOUND");
  expect(await resolveWorkspaceScope({ kind: "branch", holdingSlug: "acme", branchSlug: branch }, { companyId: f.preferred }))
    .toMatchObject({ companyId: f.preferred, branchId: branch });
});
