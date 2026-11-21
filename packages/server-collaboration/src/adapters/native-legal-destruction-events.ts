import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned legal destruction source/ledger/event statements. */
export function legalDestructionStatements(db: SqlDatabase) {
  return {
    retireSource(facts: { now: number; id: string; companyId: string }): Promise<SqlStatement> {
      return nativeSystemNotificationStatement(db, facts.companyId,
        { userId: '', recordType: '', recordId: facts.id, recordVersion: 1, message: '', kind: 'legal_retire_source' }, '', facts.now);
    },
    retireLedger(facts: { now: number; id: string; companyId: string }): Promise<SqlStatement> {
      return nativeSystemNotificationStatement(db, facts.companyId,
        { userId: '', recordType: '', recordId: facts.id, recordVersion: 1, message: '', kind: 'legal_retire_ledger' }, '', facts.now);
    },
    recordRetirement(facts: { fact0: string; companyId: string; id: string; action: string; fact4: string; now: number }): Promise<SqlStatement> {
      return nativeSystemNotificationStatement(db, facts.companyId,
        { userId: '', recordType: facts.action, recordId: facts.id, recordVersion: 1, message: facts.fact4, kind: 'legal_retire_event' }, facts.fact0, facts.now);
    },
  };
}
