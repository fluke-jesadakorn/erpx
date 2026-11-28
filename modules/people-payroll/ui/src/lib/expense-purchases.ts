export type ExpensePurchaseControls={newWork:boolean;settleExisting:boolean;history:boolean;revision:string;contextKey:string};
/** Presentation only. Every original native owner repeats its actual record,
 * role, purpose and fresh purchase check inside the Holding transaction. */
export function expenseCommandPurpose(path:string):'new-work'|'settle-existing'{return path===''||path==='/'||/^\/[^/]+$/.test(path)||/\/(submit|ai)$/.test(path)?'new-work':'settle-existing';}
export function expenseActionAllowed(controls:ExpensePurchaseControls|undefined,purpose:'new-work'|'settle-existing'|'history'){return !controls||(purpose==='new-work'?controls.newWork:purpose==='settle-existing'?controls.settleExisting:controls.history);}
