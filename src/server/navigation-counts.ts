import "server-only";
import { publicScope, type WorkContext } from "./authorization";
import { workspaceBase } from "@/lib/workspace";
import type { SqlDatabase } from "./runtime/types";
import { sourceAccessSql } from "./social-source";

export async function markHomeRead(db: SqlDatabase, actor: WorkContext, now = Date.now()) {
  await db.prepare(`INSERT INTO home_read_cursors(company_id,user_id,workspace_base,read_through)
    VALUES(?,?,?,?) ON CONFLICT(company_id,user_id,workspace_base)
    DO UPDATE SET read_through=GREATEST(home_read_cursors.read_through,excluded.read_through)`)
    .bind(actor.companyId, actor.userId, workspaceBase(publicScope(actor)), now).run();
}

export async function unreadHomeCount(db: SqlDatabase, actor: WorkContext, now = Date.now()) {
  // Match the unfiltered Home feed's audience; count all posts, including older pages.
  const row = await db.prepare(`SELECT count(*) AS count FROM social_posts p
    WHERE p.company_id=? AND p.state='published' AND p.linked_module IS NULL
    AND ${sourceAccessSql()}
    AND (?::text IS NULL OR p.branch_id=? OR p.branch_id IS NULL)
    AND p.author_id<>? AND p.created_at<=?
    AND p.created_at>COALESCE((SELECT read_through FROM home_read_cursors
      WHERE company_id=? AND user_id=? AND workspace_base=?),0)`)
    .bind(actor.companyId, actor.userId, now, now, actor.branchId, actor.branchId,
      actor.userId, now, actor.companyId, actor.userId, workspaceBase(publicScope(actor)))
    .first<{ count: number }>();
  return Number(row?.count ?? 0);
}
