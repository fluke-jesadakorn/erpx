import "server-only";
import type { PermissionActor } from "@/lib/iam";
import type { WorkContext } from "./authorization";
import {
  currentSession,
  requirePlatformContext,
  requireWorkContext,
} from "./authorization";
import { holdingResources, runtimeEnv } from "./runtime";
import { PostgresDatabase } from "./runtime/postgres";
import { ApiError } from "./http";
import { loadIamAuthority } from "./iam-store";
export type IamActor = PermissionActor & {
  userId: string;
  assignmentId: string;
  companyId: string;
  sessionId: string;
  holdingId: string | null;
  scopeKind: string;
  scopeNodeId: string | null;
  branchId: string | null;
  departmentId: string | null;
  platform: boolean;
  authority?: {
    revision: number;
    assignmentVersion: number;
    sessionId: string;
  };
};
export async function iamContext(holdingSlug: string | null) {
  const { session } = await currentSession();
  if (holdingSlug) {
    const a = await requireWorkContext(holdingSlug),
      { write } = await holdingResources(a.holdingId);
    return {
      db: write as PostgresDatabase,
      actor: {
        ...a,
        sessionId: session.session.id,
        holdingId: a.holdingId,
        scopeKind: a.scope.kind,
        scopeNodeId: a.departmentId ?? a.branchId,
        platform: false,
      } satisfies IamActor,
    };
  }
  const a = await requirePlatformContext(),
    db = (await runtimeEnv()).CONTROL_DB as PostgresDatabase;
  return {
    db,
    actor: {
      ...a,
      iam: await loadIamAuthority(
        db,
        "platform",
        a.userId,
        a.role,
        session.session.id,
      ),
      sessionId: session.session.id,
      holdingId: null,
      companyId: "platform",
      scopeKind: "platform",
      scopeNodeId: null,
      branchId: null,
      departmentId: null,
      platform: true,
    } satisfies IamActor,
  };
}
export function iamPermission(actor: IamActor, id: string) {
  return actor.platform ? `platform.${id}` : id;
}
export function assertCompanyScope(actor: IamActor) {
  if (!["holding", "platform"].includes(actor.scopeKind))
    throw new ApiError(
      403,
      "COMPANY_SCOPE_REQUIRED",
      "Choose an authorized company scope for this operation.",
    );
}

/** Adapter preserves the already resolved session and scope; it never selects another role. */
export function iamActorFromWork(a: WorkContext): IamActor {
  if (!a.authority)
    throw new ApiError(
      403,
      "FRESH_MFA_REQUIRED",
      "Sign in and verify MFA before changing access.",
    );
  return {
    ...a,
    sessionId: a.authority.sessionId,
    holdingId: a.holdingId,
    scopeKind: a.scope.kind,
    scopeNodeId: a.departmentId ?? a.branchId,
    platform: false,
  };
}
