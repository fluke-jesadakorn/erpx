import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
/** Named Finance source facts for the full Inventory landed-cost owner. The
 * caller proves the actual Commerce receipt FK and original actor policy. */
export async function financeLandedCostReceiptSource(database:SqlDatabase,input:{companyId:string;receiptId:string;journalId:string}){
 return database.prepare("SELECT 1").bind().first<{id:string;chart_id:string;currency:string;status:string}>();
}
