export type ServicePaymentPurchase={settleExisting:boolean;history:boolean};
export const controlledPaymentAllowed=(banking:boolean,source?:ServicePaymentPurchase)=>banking||source?.settleExisting===true;
export function serviceBankImportAcknowledged(value:unknown,key:string,paymentId:string){
 if(!value||typeof value!=="object")return false;
 const data=value as {id?:unknown;settlementPaymentId?:unknown;imported?:unknown};
 return data.id===`statement-${key}`&&data.settlementPaymentId===paymentId&&data.imported===1;
}
export function controlledPaymentCreationAcknowledged(value:unknown,invoiceId:string,correctionPaymentId?:string){
 if(!value||typeof value!=="object")return false;
 const data=value as {id?:unknown;status?:unknown;version?:unknown;settlementInvoiceId?:unknown;correctionPaymentId?:unknown};
 return typeof data.id==="string"&&data.id.length>0&&data.status==="draft"&&data.version===1&&data.settlementInvoiceId===invoiceId&&data.correctionPaymentId===correctionPaymentId;
}
export function paymentReversalAcknowledged(value:unknown,paymentId:string){
 if(!value||typeof value!=="object")return false;
 const data=value as {id?:unknown;status?:unknown;reversalJournalId?:unknown};
 return data.id===paymentId&&data.status==="voided"&&typeof data.reversalJournalId==="string"&&data.reversalJournalId.length>0;
}
