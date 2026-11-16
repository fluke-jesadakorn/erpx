import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
export function collaborationHomeSources(db: SqlDatabase) { return { async trackingState(task_id: string, companyId: string) { return db.prepare("SELECT 1").bind().first<string>('state'); } }; }
