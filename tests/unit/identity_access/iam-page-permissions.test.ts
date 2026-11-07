import { beforeEach, expect, it, vi } from "vitest";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import type { PermissionActor } from "@erpx/feature-identity_access/lib/iam";

const mocks = vi.hoisted(() => ({
  forbidden: vi.fn((): never => { throw new Error("page-forbidden"); }),
}));
vi.mock("next/navigation", () => mocks);

import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { ApiError, errorResponse } from "@erpx/server-runtime/adapters/http";

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

import legacyPermissionCases from "../../../fixtures/identity_access/permissions/legacy-explicit-permissions.v1.json";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import legacyWorkAdmission from "../../../fixtures/identity_access/permissions/legacy-work-admission.v1.json";
import { effectiveWorkModules } from "@erpx/server-identity_access/domain/work-modules";
it("keeps generated browser role modules compatible with retained source and verifies oracle provenance", () => {
  for(const source of legacyWorkAdmission.sources){
    const bytes=execFileSync('git',['show',`${source.revision}:${source.path}`]);
    expect(createHash('sha256').update(bytes).digest('hex'),source.path).toBe(source.sha256);
  }
  for(const row of legacyWorkAdmission.cases)expect(effectiveWorkModules(row.role,row.assigned,row.permissions,row.beta),JSON.stringify(row)).toEqual(row.expected);
});
it("keeps generated browser permission projections compatible with the recorded explicit IAM matrix", () => {
  for (const row of legacyPermissionCases.cases) {
    const actor: PermissionActor = { role: "holding_owner", scope: { kind: row.scope }, permissions: row.granted ? [row.permission] : [], ...(row.identity ? { userId: "synthetic-actor" } : {}) };
    expect(hasPermission(actor, row.permission), JSON.stringify(row)).toBe(row.expected);
  }
});
