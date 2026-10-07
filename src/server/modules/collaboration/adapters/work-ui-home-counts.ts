import type {SqlDatabase} from "../../runtime/public/runtime/types";
import type {WorkContext} from "../../identity_access/public/authorization";
import {sourceAccessSql} from "../public/social-source";
import {homeReadCursorProjection} from "../../work_projection/public/home-cursor-projection";
export async function frontUnreadHomeCount(db:SqlDatabase, actor:WorkContext, base:string, now:number) {
  // Match the unfiltered Home feed's audience; count all posts, including older pages.
  const row = await db.prepare(`SELECT count(*) AS count FROM social_posts p
    WHERE p.company_id=? AND p.state='published' AND p.linked_module IS NULL
    AND ${sourceAccessSql()}
    AND (?::text IS NULL OR p.branch_id=? OR p.branch_id IS NULL)
    AND p.author_id<>? AND p.created_at<=?
    AND p.created_at>${homeReadCursorProjection}`)
    .bind(actor.companyId, actor.userId, now, now, actor.branchId, actor.branchId,
      actor.userId, now, actor.companyId, actor.userId, base)
    .first<{ count: number }>();
  return Number(row?.count ?? 0);
}
