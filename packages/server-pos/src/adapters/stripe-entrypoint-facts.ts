import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {nativePosConnectionsStatement, nativePosStripeEntrypointJobsStatement} from './native-pos-core-statement';
export function stripeEntrypointFacts(db:SqlDatabase){return {
 /** Served by the native POS owner statement registry
  * (`stripe_readposconnections_526`); no local SQL mirror remains. */
 connection:(companyId:string)=>nativePosConnectionsStatement(db,companyId),
 /** Served by the native POS owner statement registry
  * (`stripe_readposjobsentrypoint_529`); no local SQL mirror remains. */
 jobs:(companyId:string)=>nativePosStripeEntrypointJobsStatement(db,companyId)
};}
