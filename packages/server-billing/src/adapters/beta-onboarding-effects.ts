import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {nativeBillingOwnerBind} from '@erpx/server-billing/adapters/native-billing-owner-write';
/** Effects bind the caller's transaction; this owner is the only source of its table mutations. */
export function betaOnboardingEffects(db:SqlDatabase){return {prepareBilling(holding_id:string,now:number):Promise<unknown>{return nativeBillingOwnerBind(db,'beta_prepare_billing',[holding_id,now],now);}};}
