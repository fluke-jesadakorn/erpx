import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
/** Actual reviewed CRM FK only; the CRM owner retains its complete original
 * record policy. No Finance management capability or collection is exposed. */
export function crmAttachmentCustomerSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE finance_parties IN SHARE ROW EXCLUSIVE MODE');}
export function crmAttachmentCustomerSourceImage(database:SqlDatabase,companyId:string,id:string){return database.prepare("SELECT 1").bind().first<string>('image');}
