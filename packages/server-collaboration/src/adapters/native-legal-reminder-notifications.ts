import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { nativeHoldingNotificationStatement } from './native-system-notification';

/** Native-owned legal reminder notification statements. */
export async function legalFeatureNotices(db: SqlDatabase, holdingId: string, table: string, runtimePredicate: string): Promise<void> {
  const stmt = await nativeHoldingNotificationStatement(db, holdingId,
    { userId: '', recordType: 'legal', recordId: '', recordVersion: 1, message: '', kind: 'legal_feature', table_name: table, runtime_predicate: runtimePredicate }, '', Date.now());
  await stmt.run();
}
export async function legalSlaNotices(db: SqlDatabase, holdingId: string, runtimePredicate: string, now: number): Promise<void> {
  const stmt = await nativeHoldingNotificationStatement(db, holdingId,
    { userId: '', recordType: 'legal', recordId: '', recordVersion: 1, message: '', kind: 'legal_sla', runtime_predicate: runtimePredicate }, '', now);
  await stmt.run();
}
export async function legalEscalationNotices(db: SqlDatabase, holdingId: string, runtimePredicate: string): Promise<void> {
  const stmt = await nativeHoldingNotificationStatement(db, holdingId,
    { userId: '', recordType: 'legal', recordId: '', recordVersion: 1, message: '', kind: 'legal_escalation', runtime_predicate: runtimePredicate }, '', Date.now());
  await stmt.run();
}
/** Full legal-deadline CTE: caller supplies the fixed `WITH due/added` prefix. */
export async function legalDeadlineNotice(db: SqlDatabase, holdingId: string, ctePrefix: string): Promise<void> {
  const stmt = await nativeHoldingNotificationStatement(db, holdingId,
    { userId: '', recordType: 'legal', recordId: '', recordVersion: 1, message: '', kind: 'legal_deadline', cte_prefix: ctePrefix }, '', Date.now());
  await stmt.run();
}
