export type SupplierBillPurchase={newWork:boolean;settleExisting:boolean;history:boolean;revision:string}|undefined;
export function validSupplierBillReceipt(path:string,value:unknown,key:string,input:{expectedVersion?:number;receipts?:unknown[];action?:string;matchingMode?:string},ownerId?:string):value is {id:string;version?:number;invoiceId?:string}{
 if(!value||typeof value!=="object"||Array.isArray(value)||!("id" in value)||typeof value.id!=="string"||!value.id)return false;
 const version="version" in value?value.version:undefined;
 if(path==="supplier-bills")return value.id===`bill-match-${key}`&&version===1&&"receiptCount" in value&&value.receiptCount===input.receipts?.length;
 if(path==="supplier-bills/policy")return value.id===ownerId&&version===Number(input.expectedVersion)+1&&"matchingMode" in value&&value.matchingMode===input.matchingMode;
 if(/^supplier-bills\/[^/]+$/.test(path))return value.id===path.split("/")[1]&&version===Number(input.expectedVersion)+1&&"status" in value&&value.status===(input.action==="approve"?"approved":"returned");
 return /^commerce\/fulfillments\/[^/]+\/invoice$/.test(path)&&value.id===ownerId&&"invoiceId" in value&&value.invoiceId===`invoice-${key}`;
}
