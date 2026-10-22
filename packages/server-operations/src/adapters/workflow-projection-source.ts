import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {ProjectionSource} from '@erpx/server-workflow/public/job-projector-contract';
export function taskProjectionSource(db:SqlDatabase,id:string){return db.prepare('SELECT state,version,company_id,branch_id,department_id FROM staff_tasks WHERE id=?').bind(id).first<ProjectionSource>();}
