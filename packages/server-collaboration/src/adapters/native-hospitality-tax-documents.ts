import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance hospitality tax-document statements. */
export function hospitalityTaxDocuments(db: SqlDatabase, companyId: string) {
  const call = (kind: string, key: string, values: unknown[], column?: string): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values), table_name: column }, key, 0);
  return {
    record: (f: {
      documentId: string; companyId: string; invoiceId: string; sourceType: string; issuer: string;
      externalReference: string; issuedOn: string; currency: string; totalMinor: number; taxMinor: number;
      documentTotalMinor: number; documentTaxMinor: number; userId: string; reason: string; createdAt: number;
    }) => call('hospitality_tax_doc_record', '',
      [f.documentId, f.companyId, f.invoiceId, f.sourceType, f.issuer, f.externalReference, f.issuedOn,
       f.currency, f.totalMinor, f.taxMinor, f.documentTotalMinor, f.documentTaxMinor, f.userId, f.reason, f.createdAt, f.createdAt]),
    decide: (f: {
      state: string; next: number; userId: string; reason: string; now: number; docId: string;
      companyId: string; version: number; action: 'verify' | 'void';
    }) => call('hospitality_tax_doc_decide', '',
      [f.state, f.next, f.userId, f.reason, f.now, f.docId, f.companyId, f.version],
      f.action === 'verify' ? 'verified_by' : 'voided_by'),
  };
}
