import 'server-only';
import type{SqlDatabase}from '@erpx/server-runtime/public/runtime/types';
import type{PersistedInvoiceLine}from '@erpx/server-finance/domain/invoice-line-revision';
export async function currentInvoiceLines(db: SqlDatabase, companyId: string, invoiceId: string, revision: number) {
    return (await db.prepare("SELECT * FROM finance_invoice_line_versions WHERE invoice_id=? AND company_id=? AND source_version=? ORDER BY ordinal").bind(invoiceId, companyId, revision).all<PersistedInvoiceLine>()).results;
}
