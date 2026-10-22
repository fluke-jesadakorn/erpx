import type {workforcePurchaseControls} from '@erpx/server-people_payroll/public/workforce-purchases';
export type WorkforcePurchase=NonNullable<Awaited<ReturnType<typeof workforcePurchaseControls>>>;
export type WorkforceKind='time'|'leave'|'overtime';
export function workforcePurpose(action:string){return ['create','save','submit','resubmit'].includes(action)?'newWork':'settleExisting';}
export function workforceActionAllowed(purchase:WorkforcePurchase|undefined,action:string){return !purchase||purchase[workforcePurpose(action)]===true;}
