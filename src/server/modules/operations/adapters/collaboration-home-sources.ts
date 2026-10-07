import type { SqlDatabase } from '../../runtime/public/runtime/types';
export function collaborationHomeSources(db: SqlDatabase) { return { async trackingState(task_id: string, companyId: string) { return db.prepare('SELECT state FROM staff_tasks WHERE id=? AND company_id=?').bind(task_id, companyId).first<string>('state'); } }; }
