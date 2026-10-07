import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {ProjectionSource} from '../../workflow/public/job-projector-contract';
export function taskProjectionSource(db:SqlDatabase,id:string){return db.prepare('SELECT state,version,company_id,branch_id,department_id FROM staff_tasks WHERE id=?').bind(id).first<ProjectionSource>();}
