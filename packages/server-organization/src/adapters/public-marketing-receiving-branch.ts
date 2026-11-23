import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {nativeOrgOne} from './native-organization-owner-write';

/** Native-owned public-form receiving-branch read. The authoritative SQL and
 * owner-local table live in the Organization owner crate; this adapter sends
 * only the typed kind and the caller's Company value. */
export function publicMarketingReceivingBranch(db:SqlDatabase,companyId:string){
  return nativeOrgOne<{id:string}>(db,companyId,'public_marketing_receiving_branch',[companyId]);
}
