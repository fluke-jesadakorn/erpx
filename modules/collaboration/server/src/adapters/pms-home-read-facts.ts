import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import {sourceAccessSql} from '@erpx/collaboration-server/public/social-source-policy';
import {socialPostIdHead} from './read-tail-sql';
export function readablePmsHomePosts(db: SqlDatabase, actor: WorkContext, loadedIds: string[], now: number) {
    const write = db;
    return write.prepare(`${socialPostIdHead(`p.id IN (${loadedIds.map(() => "?").join(",")})
    AND p.company_id=? AND p.state='published' AND (?::text IS NULL OR p.branch_id IS NULL OR p.branch_id=?) AND ${sourceAccessSql("p", "?::text")}`)}`)
        .bind(...loadedIds, actor.companyId, actor.branchId, actor.branchId, actor.userId, now, now, actor.assignmentId).all<{
        id: string;
    }>();
}
