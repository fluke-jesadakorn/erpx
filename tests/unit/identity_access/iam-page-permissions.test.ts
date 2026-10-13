import { beforeEach, expect, it, vi } from "vitest";
import type { PermissionActor } from "../../../src/lib/identity_access/iam";

const mocks = vi.hoisted(() => ({
  forbidden: vi.fn((): never => { throw new Error("page-forbidden"); }),
}));
vi.mock("next/navigation", () => mocks);

import { requirePermission } from "../../../src/server/modules/identity_access/public/iam-permissions";
import { ApiError, errorResponse } from "../../../src/server/modules/runtime/adapters/http";

beforeEach(() => vi.clearAllMocks());

const deniedActor: PermissionActor = { role: "employee", scope: { kind: "holding" }, permissions: [] };

it("renders the forbidden boundary for permission loss in a server page", () => {
  const actor = { ...deniedActor, pageRequest: true as const };
  expect(() => requirePermission(actor, "finance.read")).toThrow("page-forbidden");
  expect(mocks.forbidden).toHaveBeenCalledOnce();
});

it("retains structured JSON permission errors for API requests", async () => {
  let denied: unknown;
  try { requirePermission(deniedActor, "finance.read"); }
  catch (error) { denied = error; }
  expect(denied).toBeInstanceOf(ApiError);
  const response = errorResponse(denied);
  expect(response.status).toBe(403);
  expect(await response.json()).toEqual({ error: { code: "PERMISSION_REQUIRED", message: "Your selected role does not grant this permission." } });
  expect(mocks.forbidden).not.toHaveBeenCalled();
});

it("returns permitted page actors unchanged", () => {
  const actor = { ...deniedActor, permissions: ["finance.read"], pageRequest: true as const };
  expect(requirePermission(actor, "finance.read")).toBe(actor);
  expect(mocks.forbidden).not.toHaveBeenCalled();
});
