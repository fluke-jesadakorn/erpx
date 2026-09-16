import "server-only";
import type { SqlDatabase } from "./runtime/types";
/** All reviewed identities of these people, including superseded principals. */
export async function personIdentityIds(
  db: SqlDatabase,
  companyId: string,
  ids: readonly string[],
) {
  if (!ids.length) return [];
  const rows = (
    await db
      .prepare(
        `SELECT DISTINCT linked.user_id FROM iam_members original JOIN iam_members linked ON linked.company_id=original.company_id AND linked.person_id=original.person_id WHERE original.company_id=? AND original.user_id IN (${ids.map(() => "?").join(",")})`,
      )
      .bind(companyId, ...ids)
      .all<{ user_id: string }>()
  ).results;
  return [...new Set([...ids, ...rows.map((r) => r.user_id)])];
}
/** Only a reviewed supersession with one active successor changes a saved participant. */
export async function currentPersonPrincipals(
  db: SqlDatabase,
  companyId: string,
  ids: readonly string[],
) {
  if (!ids.length) return new Map<string, string>();
  const rows = (
    await db
      .prepare(
        `SELECT original.user_id original_id,linked.user_id current_id FROM iam_members original JOIN iam_members linked ON linked.company_id=original.company_id AND linked.person_id=original.person_id AND linked.status='active' WHERE original.company_id=? AND original.status='superseded' AND original.user_id IN (${ids.map(() => "?").join(",")})`,
      )
      .bind(companyId, ...ids)
      .all<{ original_id: string; current_id: string }>()
  ).results;
  const map = new Map(ids.map((id) => [id, id]));
  for (const id of ids) {
    const matches = rows.filter((r) => r.original_id === id);
    if (matches.length === 1) map.set(id, matches[0].current_id);
  }
  return map;
}
