export type TransferPurchaseControls={newWork:boolean;settleExisting:boolean;history:boolean;lossSettleExisting:boolean;revision:string};
/** UI hints only; the original owner rechecks the actual source, permissions,
 * warehouse policies and purchased capabilities inside its transaction. */
export function transferCommandAllowed(purchase:TransferPurchaseControls|undefined,path:string,input?:unknown){
 if(!purchase)return true;
 if(/^\/stock-transfers(?:\/[^/]+)?$/.test(path)||/^\/stock-transfers\/[^/]+\/submit$/.test(path)||/^\/stock-transit\/[^/]+\/dispatch$/.test(path))return purchase.newWork;
 if(/^\/stock-transfers\/[^/]+\/(post|reject|cancel)$/.test(path)||/^\/stock-transit\/[^/]+\/(receive|loss|cancel)$/.test(path))return purchase.settleExisting;
 if(/^\/stock-transit\/[^/]+\/decide-loss$/.test(path))return purchase.settleExisting&&!!input&&typeof input==='object'&&'decision' in input&&(input.decision==='reject'||input.decision==='approve'&&purchase.lossSettleExisting);
 return false;
}
