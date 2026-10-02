import "server-only";
import type { WorkContext } from "../authorization";
import type { SqlDatabase } from "../runtime/types";
import { requirePermission } from "../iam-permissions";
import { visibility } from "./access";

export async function legalWorkspaceView(db: SqlDatabase, actor: WorkContext) {
  requirePermission(actor, "legal.read");
  const scope = visibility(actor);
  const [deadlines, assigned] = await Promise.all([
    db.prepare(`SELECT i.id,i.kind,i.title,i.state,i.due_date,i.owner_id,i.version FROM legal_items i WHERE ${scope.sql} AND i.due_date IS NOT NULL AND i.state NOT IN ('archived','cancelled','closed','completed','terminated') ORDER BY i.due_date,i.id LIMIT 100`).bind(...scope.values).all(),
    db.prepare(`SELECT i.id,i.kind,i.title,i.state,i.due_date,i.owner_id,i.version FROM legal_items i WHERE ${scope.sql} AND i.owner_id=? AND i.state NOT IN ('archived','cancelled','closed','completed','terminated') ORDER BY i.due_date NULLS LAST,i.updated_at DESC LIMIT 100`).bind(...scope.values, actor.userId).all(),
  ]);
  return { deadlines: deadlines.results, assigned: assigned.results };
}
