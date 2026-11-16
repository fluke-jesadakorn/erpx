import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function stageTaskApprovalRevision(db:SqlDatabase,now:number,id:string){return db.prepare('SELECT 1').bind();}
