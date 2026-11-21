/**
 * FIXTURE-ONLY ORACLE — retained pre-cutover official law-source notice.
 *
 * Production notice generation now belongs to the native
 * `legal_law_source_notification_statement` port (server-collaboration), which
 * stages the exact INSERT in the caller's serializable unit. This copy seeds
 * the same row for the law-corpus integration suite without the TS writer.
 *
 * Tracked for removal by `retire.collaboration.legal-law-source-notifications`:
 * delete once the harness drives the native statement port directly.
 */
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';

const SQL = `INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
 SELECT ?,i.company_id,i.owner_id,'legal',i.id,i.version,'Official law source changed; review original, text, dates and Company applicability before use',?
 FROM legal_items i WHERE i.id=? AND ?=false
 AND EXISTS(SELECT 1 FROM legal_members m WHERE m.item_id=i.id AND m.user_id=i.owner_id) ON CONFLICT DO NOTHING`;

export function seedLawSourceNotice(
  db: SqlDatabase,
  _companyId: string,
  facts: { identifier: string; now: number; ruleId: string; unchanged: boolean },
): Promise<SqlStatement> {
  return Promise.resolve(db.prepare(SQL).bind(facts.identifier, facts.now, facts.ruleId, facts.unchanged));
}
