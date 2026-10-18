export type QualityPurchaseControls={newWork:boolean;settleExisting:boolean;history:boolean;revision:string};
/** Presentation hints only. Actual stored protective sources select their
 * original capability inside the owner transaction. */
export function qualityCommandAllowed(purchase:QualityPurchaseControls|undefined,path:string,input?:unknown){
 if(!purchase)return true;
 if(/^\/specifications\/[^/]+$/.test(path)&&input&&typeof input==='object'&&'action' in input)return input.action==='cancel'?purchase.settleExisting:input.action==='activate'&&purchase.newWork;
 if(path===""||/^\/specifications(?:\/[^/]+)?$/.test(path)||/^\/specifications\/[^/]+\/activate$/.test(path))return purchase.newWork;
 if(/^\/specifications\/[^/]+\/cancel$/.test(path)||/^\/[^/]+\/(inspect|release|disposition|cancel|acknowledge)$/.test(path))return purchase.settleExisting;
 return false;
}
