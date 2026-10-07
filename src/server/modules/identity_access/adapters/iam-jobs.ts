import { createCaptureJobAuthority, createRequireJobAuthority, type JobActor } from '../application/jobs';
import "server-only";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { loadIamAuthority } from "@/server/modules/identity_access/public/iam-store";
import { ApiError } from "@/server/modules/runtime/public/http";
/** Capture current authorization before queue insertion; never reconstruct it in a worker. */
export async function captureJobAuthority<T extends JobActor>(db: SqlDatabase, actor: T): Promise<T> {
    return createCaptureJobAuthority({ grant: async (actor, now) => await db
            .prepare("SELECT version FROM assignments WHERE id=? AND user_id=? AND company_id=? AND iam_assignment_active(id,?)")
            .bind(actor.assignmentId, actor.userId, actor.companyId, now)
            .first<{
            version: number;
        }>(), authority: async (actor) => loadIamAuthority(db, actor.companyId, actor.userId, actor.role, actor.authority?.sessionId), now: Date.now, error: (...args) => new ApiError(...args) })(actor);
}
export const requireJobAuthority = createRequireJobAuthority({ error: (...args) => new ApiError(...args) });
