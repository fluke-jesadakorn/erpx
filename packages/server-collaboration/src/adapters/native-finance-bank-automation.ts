import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance bank automation statements. */
export function nativeFinanceBankAutomation(db: SqlDatabase, companyId: string) {
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    csvMappingInsert: (v: unknown[]) => call('bank_csv_mapping_insert', v),
    importFileInsert: (v: unknown[]) => call('bank_import_file_insert', v),
    matchGroupInsert: (v: unknown[]) => call('bank_match_group_insert', v),
    matchAllocationInsert: (v: unknown[]) => call('bank_match_allocation_insert', v),
    txnMatch: (v: unknown[]) => call('bank_txn_match', v),
    matchTargetInsert: (v: unknown[]) => call('bank_match_target_insert', v),
    txnUnmatch: (v: unknown[]) => call('bank_txn_unmatch', v),
    matchTargetDelete: (v: unknown[]) => call('bank_match_target_delete', v),
    matchGroupDecision: (v: unknown[]) => call('bank_match_group_decision', v),
  };
}
