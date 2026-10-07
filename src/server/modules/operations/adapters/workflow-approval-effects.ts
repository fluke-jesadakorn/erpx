import type {SqlDatabase} from '../../runtime/public/runtime/types';
export function stageTaskApprovalRevision(db:SqlDatabase,now:number,id:string){return db.prepare('UPDATE staff_tasks SET version=version+1,updated_at=? WHERE id=?').bind(now,id);}
