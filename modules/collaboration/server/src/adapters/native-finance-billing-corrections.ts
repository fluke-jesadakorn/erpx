import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance billing correction statements. */
export function nativeFinanceBillingCorrections(db: SqlDatabase, companyId: string) {
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    correctionInsert: (v: unknown[]) => call('billing_correction_insert', v),
    correctionReject: (v: unknown[]) => call('billing_correction_reject', v),
    recognitionReverse: (v: unknown[]) => call('billing_recognition_reverse', v),
    invoicePaid: (v: unknown[]) => call('billing_invoice_paid', v),
    correctionPost: (v: unknown[]) => call('billing_correction_post', v),
    refundObligation: (v: unknown[]) => call('billing_refund_obligation', v),
    invoiceCreatedBy: (v: unknown[]) => call('billing_invoice_created_by', v),
    runApprove: (v: unknown[]) => call('billing_run_approve', v),
  };
}
