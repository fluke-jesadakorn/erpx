import type { SqlDatabase } from "../../runtime/public/runtime/types";
import { matchedBillLines, type BillRow } from "../domain/supplier-bills";
import { decimalToMinor } from "@/lib/finance-money";

export function documentPostingPreviewFacts(db: SqlDatabase) {
  return {
    async matchedInvoiceLines(companyId: string, id: string) {
      const bill = await db.prepare("SELECT * FROM procurement_bill_matches WHERE invoice_id=? AND company_id=? AND status='invoiced'").bind(id, companyId).first<BillRow>();
      return bill ? matchedBillLines(bill).map(line => ({ id: line.accountId, debit: decimalToMinor(line.debit), credit: decimalToMinor(line.credit) })) : null;
    },
  };
}
