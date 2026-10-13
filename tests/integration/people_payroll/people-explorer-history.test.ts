import { beforeAll, expect, it, vi } from "vitest";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import { GET } from "../../../src/app/api/v1/holdings/[holdingSlug]/people/explorer-detail/route";
import { env, applyMigrations } from "../../support/runtime-env";

const state = vi.hoisted(() => ({ actor: null as WorkContext | null }));
vi.mock("../../../src/server/modules/identity_access/public/authorization", () => ({ requireWorkContext: async () => state.actor }));
vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({
  holdingBindingBySlug: async () => ({ id: "h001", slug: "test", displayName: "Test Holding" }),
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
beforeAll(async () => { await applyMigrations(env.H001_DB, env.STAFF_TEST_MIGRATIONS); });

it("retains organization Changes filters, pagination, and linked records in lazy People details", async () => {
  const fixture = await seedExpenseCompany(env.H001_DB, env.H001_FILES);
  state.actor = fixture.owner;
  const linked = `history-linked-${crypto.randomUUID()}`;
  for (let index = 0; index < 24; index++) {
    const id = index === 23 ? linked : `history-${crypto.randomUUID()}`;
    await env.H001_DB.prepare(`INSERT INTO organization_node_changes
      (id,company_id,action,node_kind,slug,display_name,state,requested_by,reason,created_at,updated_at)
      VALUES(?,?,'create','branch',?,?,?,?,?,?,?)`)
      .bind(id, fixture.company, id, index === 23 ? "Linked closed branch" : "Matching branch", index === 23 ? "cancelled" : "draft", fixture.owner.userId, "History context regression", index + 1, index + 1).run();
  }
  async function read(filters: Record<string, string>) {
    const query = new URLSearchParams({ kind: "company", id: fixture.company, tab: "Changes", ...filters });
    const response = await GET(new Request(`http://localhost:3100/api/v1/holdings/test/people/explorer-detail?${query}`), { params: Promise.resolve({ holdingSlug: "test" }) });
    return { response, payload: await response.json() };
  }
  const filtered = await read({ changeState: "draft", changeQuery: "Matching", changePage: "2" });
  expect(filtered.response.status).toBe(200);
  expect(filtered.response.headers.get("Cache-Control")).toBe("private, no-store");
  expect(filtered.payload.data.organization.changeQueue).toMatchObject({ state: "draft", query: "Matching", page: 2, pages: 2, total: 23 });
  expect(filtered.payload.data.organization.nodeChanges).toHaveLength(3);
  expect(filtered.payload.data.organization.nodeChanges.every((row: { display_name: string }) => row.display_name === "Matching branch")).toBe(true);

  const selected = await read({ changeId: linked, changeState: "submitted", changeQuery: "unrelated", changePage: "2" });
  expect(selected.response.status).toBe(200);
  expect(selected.payload.data.organization.changeQueue).toMatchObject({ recordId: linked, state: "all", query: "", total: 1 });
  expect(selected.payload.data.organization.nodeChanges.map((row: { id: string }) => row.id)).toEqual([linked]);

  state.actor = fixture.claimant;
  const denied = await read({ changeId: linked });
  expect(denied.response.status).toBe(403);
  expect(denied.payload.error.code).toBe("EXPLORER_TAB_FORBIDDEN");
  expect(JSON.stringify(denied.payload)).not.toContain("Linked closed branch");
});
