import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {nativeSupportReadAll,SUPPORT_RD_ATTACHMENT_MIGRATION_SCOPE} from './native-support-read';

type Scope = {company_id:string;branch_id:string|null;department_id:string|null;version:number};

/** Owner-native read: the statement is planned by the Support native read
 * port (`RD_ATTACHMENT_MIGRATION_SCOPE`) and executed on the caller's open
 * unit. This module carries no SQL for this read. */
export function legacyAttachmentScopesupport(db:SqlDatabase,kind:string,id:string):Promise<Scope|null>{
 if(kind!=='support_ticket')return Promise.resolve(null);
 return nativeSupportReadAll<Scope>(SUPPORT_RD_ATTACHMENT_MIGRATION_SCOPE,[id],db).then(result => result.results[0]??null);
}
