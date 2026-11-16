import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function betaManagementTaskSources(db:SqlDatabase){return {
 taskFileOwner:'EXISTS(SELECT 1 FROM staff_tasks t WHERE t.id=a.owner_id AND t.company_id=?)',
 async taskCompany(id:string){return db.prepare('SELECT company_id FROM staff_tasks WHERE id=?').bind(id).first<{company_id:string}>();},
};}
