import type {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignFlowPrimary,resolveFlow} from "@erpx/server-workflow/public/flow-routing";
import {captureSupplierBill,decideSupplierBill} from "@erpx/server-procurement/public/supplier-bill-matching";
import {invoiceFulfillment} from "@erpx/server-commerce/public/commerce";
import {postInvoice,postJournal} from "@erpx/server-finance/public/finance";

/** Seed distinct reviewed functional Inventory and goods-clearing references
 * before any receipt is created. A clearing/payable alias is not a valid AP
 * carrying fixture and must not be hidden by an invented FX adjustment. */
export async function prepareOriginalSupplyPostingFixture(database:PostgresDatabase,companyId:string,chartId:string){
 const inventory=`supply-addon-inventory-${crypto.randomUUID()}`,clearing=`supply-addon-clearing-${crypto.randomUUID()}`;
 await database.batch([
  database.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) SELECT ?,id,company_id,?,'Original Supply inventory','asset',base_currency,1,1 FROM finance_charts WHERE id=? AND company_id=?").bind(inventory,inventory,chartId,companyId),
  database.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) SELECT ?,id,company_id,?,'Original goods clearing','liability',base_currency,1,1 FROM finance_charts WHERE id=? AND company_id=?").bind(clearing,clearing,chartId,companyId),
  database.prepare("UPDATE commerce_settings SET inventory_account_id=?,clearing_account_id=? WHERE company_id=?").bind(inventory,clearing,companyId),
 ]);
}

/** Original two-unit/ten-baht receipt fixture, completed through independent
 * matching and posting. It leaves a genuine unpaid AP document for testing
 * an add-on withdrawal alongside unfinished work owned by base Supply. */
export async function openOriginalSupplyPayable(database:PostgresDatabase,actors:{owner:WorkContext;preparer:WorkContext;reviewer:WorkContext},fulfillmentId:string,periodId:string){
 const key=()=>crypto.randomUUID(),receipt=await database.prepare("SELECT receipt.journal_id,receipt.order_id FROM commerce_fulfillments receipt JOIN commerce_orders original ON original.id=receipt.order_id WHERE receipt.id=? AND original.company_id=?").bind(fulfillmentId,actors.owner.companyId).first<{journal_id:string;order_id:string}>();
 if(!receipt?.journal_id)throw Error("ORIGINAL_SUPPLY_RECEIPT_JOURNAL_REQUIRED");
 async function assign(kind:"order"|"invoice"|"journal",id:string,stepKey="draft",userId=actors.reviewer.userId){const flow=await resolveFlow(database,actors.owner,kind,id);await assignFlowPrimary(database,actors.owner,kind,id,{stepKey,userId,expectedVersion:flow.version,recordVersion:flow.recordVersion,reason:"Original independent reviewer for retained Supply payable",saveDefault:false},key());}
 const status=await database.prepare("SELECT status FROM finance_journals WHERE id=?").bind(receipt.journal_id).first<string>("status");
 if(status==="draft"){await assign("journal",receipt.journal_id);await postJournal(actors.reviewer,receipt.journal_id,key(),{expectedVersion:1});}
 const version=Number(await database.prepare("SELECT version FROM commerce_fulfillments WHERE id=?").bind(fulfillmentId).first("version")),lines=(await database.prepare("SELECT id FROM commerce_fulfillment_lines WHERE fulfillment_id=?").bind(fulfillmentId).all<{id:string}>()).results;
 if(lines.length!==1)throw Error("ORIGINAL_TWO_UNIT_RECEIPT_LINE_REQUIRED");
 const bill=await captureSupplierBill(actors.preparer,{fulfillmentId,expectedVersion:version,reference:`ORIGINAL-AP-${key()}`,invoiceDate:"2026-09-03",dueDate:"2026-09-10",evidence:"Actual two-unit supplier bill remains unpaid during separate addon withdrawal",lines:[{fulfillmentLineId:lines[0].id,quantity:"2",price:"10",taxBps:0}]},key());
 await decideSupplierBill(actors.reviewer,String(bill.id),{action:"approve",expectedVersion:1,reason:"Independent reviewer confirms original supplier receipt and bill"},key());
 await assign("order",receipt.order_id,"invoicing",actors.preparer.userId);
 const invoice=await invoiceFulfillment(actors.preparer,fulfillmentId,{expectedVersion:Number(await database.prepare("SELECT version FROM commerce_fulfillments WHERE id=?").bind(fulfillmentId).first("version")),date:"2026-09-03",dueDate:"2026-09-10",matchId:String(bill.id),matchVersion:2},key()),invoiceId=String(invoice.invoiceId);
 await assign("invoice",invoiceId);await postInvoice(actors.reviewer,invoiceId,{expectedVersion:1,periodId,entryDate:"2026-09-03"},key());
 return {billId:String(bill.id),invoiceId};
}
