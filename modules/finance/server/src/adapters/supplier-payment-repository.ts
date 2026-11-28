import {supplierPaymentSourceOptions} from "@erpx/finance-server/adapters/supplier-payment-purchases";
import {nativeFinanceReadAll,nativeFinanceReadFirst,FINANCE_RD_SUPPLIER_BATCH_ROW,FINANCE_RD_SUPPLIER_INSTRUCTION_ROW,FINANCE_RD_SUPPLIER_BATCH_LIST,FINANCE_RD_SUPPLIER_BATCH_WORK_LIST,FINANCE_RD_SUPPLIER_BATCH_ITEMS,FINANCE_RD_SUPPLIER_BATCH_INVOICES,FINANCE_RD_SUPPLIER_BATCH_INVOICE,FINANCE_RD_SUPPLIER_BATCH_ACCOUNTS,FINANCE_RD_SUPPLIER_BATCH_PERIODS,FINANCE_RD_SUPPLIER_BATCH_BANK} from "./native-finance-read";
import "server-only";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {hasPermission} from "@erpx/identity-access-ui/lib/permissions";
import type {prepareJournal} from '@erpx/finance-server/public/journal-planning';
import type {prepareAttachmentBinding} from "@erpx/files-server/public/attachments/service";
import type {SupplierPaymentRow as Row} from "@erpx/procurement-server/public/supplier-payment-contract";
export type SupplierPaymentEvidence=Awaited<ReturnType<typeof prepareAttachmentBinding>>;
export type SupplierPaymentJournal=Awaited<ReturnType<typeof prepareJournal>>;
/** Bounded read port consumed by the procurement supplier-payment application.
 *  Every statement is planned by the native Finance owner and executed on the
 *  caller's already-open unit. No TypeScript SQL remains: the purchase source
 *  fence, the invoiceId picker and the canPay bank gate are composition inputs. */
export interface SupplierPaymentReadPort{
  batch(id:string):Promise<Row|null>;
  item(id:string):Promise<Row|null>;
  list(forWork:boolean):Promise<Row[]>;
  workspace(id:string|undefined,batch:Row|null):Promise<{items:Row[];invoices:Row[];accounts:Row[];periods:Row[];bank:Row[]}>;
}
export function supplierPaymentRepository(write:SqlDatabase,actor:WorkContext,options:{invoiceId?:string}={}):SupplierPaymentReadPort{return {
  batch:(id:string)=>nativeFinanceReadFirst<Row>(FINANCE_RD_SUPPLIER_BATCH_ROW,[id,actor.companyId],write),
  item:(id:string)=>nativeFinanceReadFirst<Row>(FINANCE_RD_SUPPLIER_INSTRUCTION_ROW,[id,actor.companyId],write),
  async list(forWork:boolean){
   const source=await supplierPaymentSourceOptions(write,actor);
   return (await nativeFinanceReadAll<Row>(forWork?FINANCE_RD_SUPPLIER_BATCH_WORK_LIST:FINANCE_RD_SUPPLIER_BATCH_LIST,[actor.companyId,source.sourceOnly],write)).results;
  },async workspace(id:string|undefined,batch:Row|null){
   const source=await supplierPaymentSourceOptions(write,actor);
   const [items,invoices,accounts,periods,bank]=await Promise.all([
    id?nativeFinanceReadAll<Row>(FINANCE_RD_SUPPLIER_BATCH_ITEMS,[id],write):Promise.resolve({results:[] as Row[]}),
    !id?nativeFinanceReadAll<Row>(options.invoiceId?FINANCE_RD_SUPPLIER_BATCH_INVOICE:FINANCE_RD_SUPPLIER_BATCH_INVOICES,options.invoiceId?[actor.companyId,source.sourceOnly,options.invoiceId]:[actor.companyId,source.sourceOnly],write):Promise.resolve({results:[] as Row[]}),
    nativeFinanceReadAll<Row>(FINANCE_RD_SUPPLIER_BATCH_ACCOUNTS,[actor.companyId],write),
    nativeFinanceReadAll<Row>(FINANCE_RD_SUPPLIER_BATCH_PERIODS,[actor.companyId],write),
    batch&&hasPermission(actor,'finance.pay')?nativeFinanceReadAll<Row>(FINANCE_RD_SUPPLIER_BATCH_BANK,[actor.companyId,batch.bank_account_id,source.sourceOnly,batch.currency,batch.planned_date,batch.id],write):Promise.resolve({results:[] as Row[]}),
   ]);
   return {items:items.results,invoices:invoices.results,accounts:accounts.results,periods:periods.results,bank:bank.results};
  }
  };}
