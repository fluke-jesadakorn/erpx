import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import type {EventMessage,ProjectionWork} from '@erpx/workflow-server/public/job-projector-contract';
import {nativeOwnerWrite} from './native-owner-write';
import {noopSql} from './read-tail-sql';
/** Commit one projected workflow event. All three writes now execute in the
 * native Work axum command with receipt/audit/outbox in the same Rust unit;
 * no SQL and no fallback remain in this projector. */
export async function commitWorkflowProjection(read:SqlDatabase,message:EventMessage,payload:EventMessage['payload'],version:number,work:ProjectionWork|null,times:{receiptAt:number;workAt:number;checkpointAt:number;updatedAt:number}){
 const noop=()=>read.prepare(noopSql);
 await nativeOwnerWrite('wp_projection_receipt',[message.id,message.aggregateId,message.aggregateVersion,times.receiptAt],`wp-projection:${message.id}`,{now:times.receiptAt},noop);
 if(work)await nativeOwnerWrite('wp_work_item_upsert',[
   `work_${message.aggregateId}`,work.sourceType,message.aggregateId,version,work.title,payload.state,
   work.queue,payload.companyId,payload.branchId,payload.departmentId,times.workAt,
 ],`wp-work-item:${message.aggregateId}:${version}`,{companyId:payload.companyId,now:times.workAt},noop);
 await nativeOwnerWrite('wp_projection_checkpoint',[message.id,times.checkpointAt,times.updatedAt],`wp-checkpoint:${message.id}`,{now:times.checkpointAt},noop);
}
