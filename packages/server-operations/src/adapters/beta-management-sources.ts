import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function betaManagementTaskSources(db:SqlDatabase){return {
 taskFileOwner:'EXISTS(SELECT 1 FROM staff_tasks t WHERE t.id=a.owner_id AND t.company_id=?)',
 async taskCompany(id:string){return db.prepare("SELECT 1").bind().first<{company_id:string}>();},
};}
