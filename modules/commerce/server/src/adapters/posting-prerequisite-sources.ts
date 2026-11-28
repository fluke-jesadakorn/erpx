import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {nativeCommerceReadFirst,RD_POSTING_PREREQUISITE_STATE} from './native-commerce-read';
/** Actual initial setup state, under the existing complete Commerce source
 * leases. It authorizes neither a package nor a Finance command. */
export async function commercePostingPrerequisiteState(database:SqlDatabase,companyId:string){
 const row=await nativeCommerceReadFirst<{configured:boolean;history:boolean}>(RD_POSTING_PREREQUISITE_STATE,[companyId,companyId,companyId,companyId,companyId],database);
 if(!row)throw Error('Commerce initial posting state is unavailable.');return row;
}
