import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance withholding statements. */
export function nativeFinanceWithholding(db: SqlDatabase, companyId: string) {
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    consumed: (v: unknown[]) => call('wh_withholding_consumed', v),
    reversed: (v: unknown[]) => call('wh_assessment_reversed', v),
    thaiDocumentCancel: (v: unknown[]) => call('wh_thai_document_cancel', v),
    assessmentInsert: (v: unknown[]) => call('wh_assessment_insert', v),
    incomeLineInsert: (v: unknown[]) => call('wh_income_line_insert', v),
    paymentWithholding: (v: unknown[]) => call('wh_payment_withholding', v),
    assessmentDecision: (v: unknown[]) => call('wh_assessment_decision', v),
  };
}
