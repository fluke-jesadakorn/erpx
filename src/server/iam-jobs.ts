import "server-only";
import type { PermissionActor } from "@/lib/iam";
import type { SqlDatabase } from "./runtime/types";
import { loadIamAuthority } from "./iam-store";
import { ApiError } from "./http";
type JobActor = PermissionActor & {
  userId: string;
  assignmentId: string;
  companyId: string;
  authority?: { sessionId: string; assignmentVersion: number };
};
/** Capture current authorization before queue insertion; never reconstruct it in a worker. */
export async function captureJobAuthority<T extends JobActor>(
  db: SqlDatabase,
  actor: T,
): Promise<T> {
  const grant = await db
    .prepare(
      "SELECT version FROM assignments WHERE id=? AND user_id=? AND company_id=? AND iam_assignment_active(id,?)",
    )
    .bind(actor.assignmentId, actor.userId, actor.companyId, Date.now())
    .first<{ version: number }>();
  if (!grant)
    throw new ApiError(
      403,
      "ASSIGNMENT_UNAVAILABLE",
      "The authorizing assignment is unavailable.",
    );
  return {
    ...actor,
    iam: {
      ...(actor.iam ??
        (await loadIamAuthority(
          db,
          actor.companyId,
          actor.userId,
          actor.role,
          actor.authority?.sessionId,
        ))),
      queuedAssignmentVersion:
        actor.authority?.assignmentVersion ?? grant.version,
    },
  };
}
export function requireJobAuthority(actor: JobActor | null) {
  if (!actor?.iam?.queuedAssignmentVersion)
    throw new ApiError(
      409,
      "IAM_JOB_STALE",
      "This job predates the current access policy. Review and submit it again.",
    );
}
