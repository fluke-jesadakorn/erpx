import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance instrument statements. */
export function nativeFinanceInstruments(db: SqlDatabase, companyId: string) {
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    editionInsert: (v: unknown[]) => call('instrument_edition_insert', v),
    editionSupersede: (v: unknown[]) => call('instrument_edition_supersede', v),
    bankMatch: (v: unknown[]) => call('instrument_bank_match', v),
    editionDecision: (v: unknown[]) => call('instrument_edition_decision', v),
    runInsert: (v: unknown[]) => call('instrument_run_insert', v),
    bankMatch2: (v: unknown[]) => call('instrument_bank_match2', v),
    editionAgree: (v: unknown[]) => call('instrument_edition_agree', v),
    runDecision: (v: unknown[]) => call('instrument_run_decision', v),
    bankUnmatch: (v: unknown[]) => call('instrument_bank_unmatch', v),
    runReverse: (v: unknown[]) => call('instrument_run_reverse', v),
    editionActivate: (v: unknown[]) => call('instrument_edition_activate', v),
  };
}
