import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
/** Named Finance source facts for the full Inventory landed-cost owner. The
 * caller proves the actual Commerce receipt FK and original actor policy. */
export async function financeLandedCostReceiptSource(database:SqlDatabase,input:{companyId:string;receiptId:string;journalId:string}){
 return database.prepare("SELECT id,chart_id,currency,status FROM finance_journals WHERE id=? AND company_id=? AND source_type='goods_receipt' AND source_id=?").bind(input.journalId,input.companyId,input.receiptId).first<{id:string;chart_id:string;currency:string;status:string}>();
}
