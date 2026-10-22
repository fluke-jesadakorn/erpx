import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {Row} from "@erpx/server-commerce/public/commerce-contracts";
export function CommerceProcurementFacts(db:SqlDatabase){return {
readProcurementBillMatchLines1:(facts:{invoice_id:string | number | null;id:string | number | null;})=>db.prepare("SELECT l.subtotal_minor,l.tax_minor FROM procurement_bill_match_lines l JOIN procurement_bill_matches m ON m.id=l.match_id WHERE m.invoice_id=? AND m.status='posted' AND l.fulfillment_line_id=?").bind(facts.invoice_id,facts.id).first<Row>(),
readProcurementBillMatches1:(facts:{invoice_id:string | number | null;companyId:string;})=>db.prepare("SELECT * FROM procurement_bill_matches WHERE invoice_id=? AND company_id=? AND status='posted'").bind(facts.invoice_id,facts.companyId).first<Row>()
};}
