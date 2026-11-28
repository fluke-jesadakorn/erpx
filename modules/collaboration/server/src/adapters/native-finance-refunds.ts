import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance refund statements. */
export function nativeFinanceRefunds(db: SqlDatabase, companyId: string) {
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    obligationApprove: (v: unknown[]) => call('refund_obligation_approve', v),
    instructionInsert: (v: unknown[]) => call('refund_instruction_insert', v),
    obligationBump: (v: unknown[]) => call('refund_obligation_bump', v),
    instructionExport: (v: unknown[]) => call('refund_instruction_export', v),
    instructionResolve: (v: unknown[]) => call('refund_instruction_resolve', v),
    obligationBump2: (v: unknown[]) => call('refund_obligation_bump2', v),
    paymentInsert: (v: unknown[]) => call('refund_payment_insert', v),
    bankMatch: (v: unknown[]) => call('refund_bank_match', v),
    instructionConfirm: (v: unknown[]) => call('refund_instruction_confirm', v),
    obligationSettle: (v: unknown[]) => call('refund_obligation_settle', v),
  };
}
