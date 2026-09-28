import { beforeEach, expect, it, vi } from "vitest";
import { ApiError } from "../src/server/http";
import { GET, POST } from "../src/app/api/v1/account/quick-access/[area]/route";

const state = vi.hoisted(() => ({
  userId: "account-user",
  choices: [{ kind: "holding" }],
  rejectSession: false,
  rejectRole: false,
  read: vi.fn(),
  save: vi.fn(),
}));

vi.mock("../src/server/authorization", () => ({
  currentSession: async () => {
    if (state.rejectSession) throw new Error("UNAUTHENTICATED");
    return { session: { user: { id: state.userId } } };
  },
}));
vi.mock("../src/server/workspace-choices", () => ({ workspaceChoices: async () => state.choices }));
vi.mock("../src/server/work-role-options", () => ({
  workRoleOptions: async () => {
    if (state.rejectRole) throw new ApiError(403, "FORBIDDEN", "Work role unavailable.");
    return { access: { userId: state.userId }, options: [{ id: "assignment" }] };
  },
}));
vi.mock("../src/server/quick-access-preferences", async (importOriginal) => ({
  ...await importOriginal<typeof import("../src/server/quick-access-preferences")>(),
  readQuickAccessPreferences: state.read,
  saveQuickAccessPreferences: state.save,
}));

const route = (area: string) => ({ params: Promise.resolve({ area }) });
const url = (area: string, from = "") => `http://localhost:3100/api/v1/account/quick-access/${area}${from ? `?from=${encodeURIComponent(from)}` : ""}`;

beforeEach(() => {
  state.userId = "account-user";
  state.choices = [{ kind: "holding" }];
  state.rejectSession = false;
  state.rejectRole = false;
  state.read.mockReset().mockResolvedValue({ pinnedIds: null, version: 0 });
  state.save.mockReset().mockResolvedValue({ pinnedIds: ["access"], version: 1 });
});

it("scopes work-role preferences to the authorized workspace from the return path", async () => {
  const from = "/holdings/acme/branches/east/home";
  const get = await GET(new Request(url("work-role", from)), route("work-role"));
  expect(get.status).toBe(200);
  expect(state.read).toHaveBeenCalledWith("account-user", "account-work-role", "/holdings/acme/branches/east");
  const response = await POST(new Request(url("work-role", from), {
    method: "POST",
    headers: { "Content-Type": "application/json", "Idempotency-Key": "account-save-001" },
    body: JSON.stringify({ pinnedIds: ["access"], expectedVersion: 0 }),
  }), route("work-role"));
  expect(response.status).toBe(200);
  expect(state.save).toHaveBeenCalledWith(expect.objectContaining({
    userId: "account-user",
    surface: "account-work-role",
    contextKey: "/holdings/acme/branches/east",
    availableIds: ["choose-role", "workspaces", "access", "security"],
    requestKey: "account-save-001",
  }));
});

it("never offers a workspace shortcut without an authorized workspace", async () => {
  state.choices = [];
  await POST(new Request(url("my-access"), {
    method: "POST",
    headers: { "Content-Type": "application/json", "Idempotency-Key": "account-save-002" },
    body: JSON.stringify({ pinnedIds: ["security"], expectedVersion: 0 }),
  }), route("my-access"));
  expect(state.save.mock.calls[0][0].availableIds).toEqual(["roles", "security"]);
});

it("rejects invalid contexts and unauthenticated preference reads", async () => {
  expect((await GET(new Request(url("unknown")), route("unknown"))).status).toBe(404);
  expect((await GET(new Request(url("work-role", "/platform/home")), route("work-role"))).status).toBe(422);
  state.rejectRole = true;
  expect((await GET(new Request(url("work-role", "/holdings/acme/home")), route("work-role"))).status).toBe(403);
  state.rejectSession = true;
  expect((await GET(new Request(url("security")), route("security"))).status).toBe(401);
  expect(state.read).not.toHaveBeenCalled();
});
