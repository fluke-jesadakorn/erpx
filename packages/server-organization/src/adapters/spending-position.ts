import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {nativeOrgOne} from './native-organization-owner-write';

/** Native-owned active-spending-position fact. The authoritative SQL and
 * Company scope live in the Organization owner crate; this adapter sends only
 * the typed kind and the caller's positional values and keeps the boolean
 * reduction. */
export async function activeSpendingPosition(db:SqlDatabase,companyId:string,id:string){
  return !!(await nativeOrgOne(db,companyId,'spending_active_position',[id,companyId]));
}
