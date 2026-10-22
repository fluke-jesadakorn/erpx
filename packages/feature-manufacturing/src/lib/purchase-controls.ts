export type ManufacturingPurchaseControls={newWork:boolean;settleExisting:boolean;history:boolean;revision:string};
export function manufacturingCommandAllowed(purchase:ManufacturingPurchaseControls|undefined,input:unknown){
 if(!input||typeof input!=="object"||Array.isArray(input))return false;
 const command=input as Record<string,unknown>,entity=command.entity,action=command.action;
 let purpose:"newWork"|"settleExisting";
 if(entity==="resource-calendar")purpose="newWork";
 else if(entity==="resource")purpose=command.id&&command.status==="archived"?"settleExisting":"newWork";
 else if(entity==="specification"||entity==="accounting") {if(action===undefined||action==="approve")purpose="newWork";else if(action==="reject"||action==="retire")purpose="settleExisting";else return false;}
 else if(entity==="scenario"){if(action===undefined)purpose="newWork";else if(action==="cancel")purpose="settleExisting";else return false;}
 else if(entity==="recommendation"){if(action==="approve")purpose="newWork";else if(action==="reject")purpose="settleExisting";else return false;}
 else if(entity==="order"){if(action==="reserve")purpose="newWork";else if(["issue","report","pass-qc","fail-qc","rework","scrap","complete-operation","cancel"].includes(String(action)))purpose="settleExisting";else return false;}
 else return false;
 return purchase?.[purpose]!==false;
}
