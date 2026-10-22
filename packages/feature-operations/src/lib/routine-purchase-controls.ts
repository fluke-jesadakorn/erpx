import type {RoutinePurchase,RoutinePurchases} from '@erpx/server-operations/public/rules/routine-purchase-controls';
/** Moving or cloning a real source intersects its original owner and target.
 * These controls never replace the server's persisted-source classification. */
export function routineSourcePurchase(purchases:RoutinePurchases|undefined,targetProjectId:string|null,sourceProjectId?:string|null):RoutinePurchase|undefined{
 if(!purchases)return;const values=[targetProjectId?purchases.projects:purchases.work,...(sourceProjectId===undefined?[]:[sourceProjectId?purchases.projects:purchases.work])];return {newWork:values.every(value=>value.newWork),settleExisting:values.every(value=>value.settleExisting),history:values.every(value=>value.history),revision:values.map(value=>value.revision).join(':')};
}
