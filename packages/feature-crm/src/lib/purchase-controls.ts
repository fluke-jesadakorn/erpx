export type CRMPurchaseControls={newWork:boolean;settleExisting:boolean;history:boolean;revision:string};
export type CRMReferencePurchases={marketing:CRMPurchaseControls;hotel:CRMPurchaseControls};
export function crmCommandAllowed(purchase:CRMPurchaseControls|undefined,input:unknown,references?:CRMReferencePurchases){
 if(!input||typeof input!=='object'||Array.isArray(input))return false;const command=input as Record<string,unknown>,action=command.action,identity=command.domain==='identity';
 let purpose:'newWork'|'settleExisting';
 if(identity){if(['create-account','create-contact','link-guest'].includes(String(action))||action==='guest-sharing'&&command.state==='granted')purpose='newWork';else if(action==='link-finance'||action==='guest-sharing'&&command.state==='revoked')purpose='settleExisting';else return false;}
 else if(action===undefined||action==='qualify'||action==='reopen')purpose='newWork';
 else if(['disqualify','reassign','link-quote','negotiate','win','win-pms','lose','complete','cancel'].includes(String(action)))purpose='settleExisting';else return false;
 if(purchase?.[purpose]===false)return false;
 if((action==='link-guest'||action==='guest-sharing'||action==='win-pms'||command.sourceKind==='pms')&&references?.hotel[purpose]===false)return false;
 if((command.campaignId||command.sourceKind==='form')&&references?.marketing[purpose]===false)return false;
 return true;
}
export function marketingCommandAllowed(purchase:CRMPurchaseControls|undefined,input:unknown,handoff?:CRMPurchaseControls){
 if(!input||typeof input!=='object'||Array.isArray(input))return false;const command=input as Record<string,unknown>,action=command.action;
 const purpose=action===undefined||action==='create'||action==='activate'?'newWork':action==='retire'||action==='reject'||action==='record-engagement'||action==='handoff'||action==='link-cost'?'settleExisting':null;
 return !!purpose&&purchase?.[purpose]!==false&&(action!=='handoff'||handoff?.settleExisting!==false);
}
