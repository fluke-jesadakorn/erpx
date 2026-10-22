/** One destination catalogue for Home, Work cards and New Work. */
export const selfServiceActions = [
 {id:"shifts",label:"My shifts",description:"Your roster and shift changes",path:"/people/roster?view=mine"},
 {id:"leave",label:"My leave",description:"Balances, time off and history",path:"/leave"},
 {id:"overtime",label:"Overtime",description:"Plan extra work or claim hours",path:"/overtime"},
 {id:"purchase_request",label:"Purchase request",description:"Request what you need for work",path:"/requests/new?kind=purchase"},
] as const;
export function selfServiceHref(base:string,kind:string) {
 const action=selfServiceActions.find(a=>a.id===kind);
 return action?`${base}${action.path}`:null;
}
