import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance tax register statements. */
export function nativeFinanceTaxRegister(db: SqlDatabase, companyId: string) {
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    taxEntryInsert: (v: unknown[]) => call('tax_entry_insert', v),
    packageEvent: (v: unknown[]) => call('tax_package_event', v),
    taxEntryDecision: (v: unknown[]) => call('tax_entry_decision', v),
    vatCorrectionInsert: (v: unknown[]) => call('vat_correction_insert', v),
    taxEntryCorrectionDecision: (v: unknown[]) => call('tax_entry_correction_decision', v),
    vatCorrectionDecision: (v: unknown[]) => call('vat_correction_decision', v),
    calcRunInsert: (v: unknown[]) => call('calc_run_insert', v),
    statutoryPackageInsert: (v: unknown[]) => call('statutory_package_insert', v),
    statutoryPackageApprove: (v: unknown[]) => call('statutory_package_approve', v),
    calcRunApprove: (v: unknown[]) => call('calc_run_approve', v),
    statutoryPackageExport: (v: unknown[]) => call('statutory_package_export', v),
    statutoryPackageFile: (v: unknown[]) => call('statutory_package_file', v),
    statutoryPackageSupersede: (v: unknown[]) => call('statutory_package_supersede', v),
  };
}
