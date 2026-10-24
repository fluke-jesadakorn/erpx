import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {sourceAccessSql} from '@erpx/server-collaboration/public/social-source-policy';
export function readablePmsHomePosts(db: SqlDatabase, actor: WorkContext, loadedIds: string[], now: number) {
    const write = db;
    return write.prepare(`SELECT p.id FROM social_posts p WHERE p.id IN (${loadedIds.map(() => "?").join(",")})
    AND p.company_id=? AND p.state='published' AND (?::text IS NULL OR p.branch_id IS NULL OR p.branch_id=?) AND ${sourceAccessSql("p", "?::text")}`)
        .bind(...loadedIds, actor.companyId, actor.branchId, actor.branchId, actor.userId, now, now, actor.assignmentId).all<{
        id: string;
    }>();
}
