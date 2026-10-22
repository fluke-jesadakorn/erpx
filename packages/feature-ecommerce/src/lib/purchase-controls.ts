export type EcommercePurchaseControls={newWork:boolean;settleExisting:boolean;history:boolean;revision:string;canReview:boolean};
export function ecommerceCommandAllowed(purchase:EcommercePurchaseControls|undefined,input:unknown){
 if(!purchase)return true;if(!input||typeof input!=='object')return false;const value=input as Record<string,unknown>,review=value.entity==='channel'||value.entity==='listing';
 if(value.action){const active=['activate','publish','approve-order'].includes(String(value.action));return (active?purchase.newWork:purchase.settleExisting)&&(!(review||value.action==='approve-order')||purchase.canReview);}
 return purchase.newWork;
}
