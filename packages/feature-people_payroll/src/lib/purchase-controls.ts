export type PeoplePurchaseAccess={newWork:boolean;settleExisting:boolean;history:boolean;revision:string};
export type PeoplePurchaseControls={directory:PeoplePurchaseAccess;lifecycle:PeoplePurchaseAccess;identity:PeoplePurchaseAccess};
export function peopleLifecyclePurchased(controls:PeoplePurchaseControls|undefined,kind:string){return !controls||(['offboarding','suspend'].includes(kind)?controls.lifecycle.settleExisting:controls.lifecycle.newWork);}
