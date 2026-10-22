export type LandedCostPurchaseControls={newWork:boolean;settleExisting:boolean;history:boolean;revision:string};
export function landedCostCommandAllowed(purchase:LandedCostPurchaseControls|undefined,path:string){if(!purchase)return true;if(path==="")return purchase.newWork;return /^\/[^/]+\/(approve|reject|cancel|post)$/.test(path)&&purchase.settleExisting;}
