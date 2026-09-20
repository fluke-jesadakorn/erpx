import "server-only";
import { z } from "zod";
import type { SqlDatabase } from "../runtime/types";
import type { WorkContext } from "../authorization";
import { requirePermission } from "../iam-permissions";
import { visibility } from "./access";
export async function legalKnowledge(
  db: SqlDatabase,
  a: WorkContext,
  query: unknown,
) {
  requirePermission(a, "legal.read");
  const q = z.string().trim().min(2).max(200).parse(query),
    v = visibility(a);
  const rows = (
    await db
      .prepare(
        `SELECT i.id item_id,i.title,d.id document_id,d.name,d.version,d.checksum,substring(d.body from greatest(1,strpos(lower(d.body),lower(?))-120) for 500) excerpt FROM legal_documents d JOIN legal_items i ON i.id=d.item_id WHERE ${v.sql} AND strpos(lower(d.body),lower(?))>0 AND NOT EXISTS(SELECT 1 FROM legal_disposals x WHERE x.document_id=d.id) ORDER BY i.updated_at DESC,d.version DESC LIMIT 50`,
      )
      .bind(q, ...v.values, q)
      .all()
  ).results;
  if (rows.length) await db.batch(rows.map((row) => db
    .prepare("INSERT INTO legal_access_events VALUES(?,?,?,?,?,?)")
    .bind(crypto.randomUUID(), row.item_id, row.document_id, a.userId, "knowledge.read", Date.now())));
  return {
    query: q,
    sources: rows,
    notice: rows.length
      ? "Review the cited immutable version before relying on its text."
      : "No accessible source matched. No legal conclusion can be inferred from missing evidence.",
  };
}
