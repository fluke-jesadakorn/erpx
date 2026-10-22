import {minorToDecimal} from "@erpx/feature-finance/lib/finance-money";
import {controlledPaymentCreationAcknowledged} from "@erpx/feature-finance/lib/service-payment-controls";

/** Minimal facts for an already-authorized original supplier payment. */
export type SupplierPaymentCorrectionSource={paymentId:string;paymentNumber:string;amountMinor:number;bankAccountId:string;bankAccountLabel:string;bankReference:string};
export type SupplierInvoiceCorrections={capability:"finance.inventory-valuation";settleExisting:boolean;history:boolean;revision:string;sources:SupplierPaymentCorrectionSource[];prepared?:{paymentId:string;paymentNumber:string;status:string;originalPaymentId:string}[]};
export type SupplierCorrectionInput={partyId:string;kind:"payment";paymentNumber:string;paymentDate:string;currency:string;amount:string;bankAccount:string;settlementInvoiceId:string;correctionPaymentId:string};
export function supplierCorrectionSource(sources:readonly SupplierPaymentCorrectionSource[],id:string){const matches=sources.filter(source=>source.paymentId===id);return matches.length===1?matches[0]:undefined;}
export function supplierCorrectionInput(source:SupplierPaymentCorrectionSource,invoice:{id:string;partyId:string;currency:string},fields:{paymentNumber:string;paymentDate:string}):SupplierCorrectionInput{return {...fields,partyId:invoice.partyId,kind:"payment",currency:invoice.currency,amount:minorToDecimal(source.amountMinor),bankAccount:source.bankAccountId,settlementInvoiceId:invoice.id,correctionPaymentId:source.paymentId};}
export async function sendSupplierCorrection(fetcher:(url:string,init:RequestInit)=>Promise<Response>,url:string,attempt:{input:SupplierCorrectionInput;body:string;key:string}){
 const response=await fetcher(url,{method:"POST",headers:{"Content-Type":"application/json","Idempotency-Key":attempt.key},body:attempt.body}),result=await response.json() as {data?:{id?:string;amount?:unknown;outstanding?:unknown};error?:{message?:string}};
 if(!response.ok||!controlledPaymentCreationAcknowledged(result.data,attempt.input.settlementInvoiceId,attempt.input.correctionPaymentId)||result.data?.amount!==attempt.input.amount||result.data?.outstanding!==attempt.input.amount)throw Error(result.error?.message??"The supplier correction was not confirmed. Retry the captured request safely.");
 return result.data!.id!;
}
