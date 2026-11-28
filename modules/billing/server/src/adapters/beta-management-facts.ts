import {nativeBillingOwnerBind} from '@erpx/billing-server/adapters/native-billing-owner-write';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
/** The beta billing status read is owner-authored fixed SQL; no local query text remains. */
export function betaManagementFacts(db:SqlDatabase){return {async billing(holdingId:string):Promise<Record<string,unknown>|null>{return (await nativeBillingOwnerBind(db,'beta_management_billing_status',[holdingId])).first();}};}
