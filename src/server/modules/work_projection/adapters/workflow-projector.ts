import type {SqlDatabase,SqlStatement} from '../../runtime/public/runtime/types';
import type {EventMessage,ProjectionWork} from '../../workflow/public/job-projector-contract';
export async function commitWorkflowProjection(read:SqlDatabase,message:EventMessage,payload:EventMessage['payload'],version:number,work:ProjectionWork|null,times:{receiptAt:number;workAt:number;checkpointAt:number;updatedAt:number}){
 const statements:SqlStatement[]=[read.prepare('INSERT INTO projection_receipts(event_id,aggregate_id,aggregate_version,applied_at) VALUES(?,?,?,?) ON CONFLICT DO NOTHING').bind(message.id,message.aggregateId,message.aggregateVersion,times.receiptAt)];
 if(work)statements.push(read.prepare(`INSERT INTO work_items(id,source_type,source_id,source_version,title,subtitle,queue,company_id,branch_id,department_id,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)
      ON CONFLICT(source_type,source_id) DO UPDATE SET source_version=excluded.source_version,subtitle=excluded.subtitle,
        queue=excluded.queue,company_id=excluded.company_id,branch_id=excluded.branch_id,department_id=excluded.department_id,updated_at=excluded.updated_at
      WHERE work_items.source_version<excluded.source_version`).bind(`work_${message.aggregateId}`,work.sourceType,message.aggregateId,version,work.title,payload.state,work.queue,payload.companyId,payload.branchId,payload.departmentId,times.workAt));
 statements.push(read.prepare(`INSERT INTO projection_checkpoints(projector,last_event_id,last_event_at,updated_at) VALUES('work-items',?,?,?)
      ON CONFLICT(projector) DO UPDATE SET last_event_id=excluded.last_event_id,last_event_at=excluded.last_event_at,updated_at=excluded.updated_at`).bind(message.id,times.checkpointAt,times.updatedAt));
 await read.batch(statements);
}
