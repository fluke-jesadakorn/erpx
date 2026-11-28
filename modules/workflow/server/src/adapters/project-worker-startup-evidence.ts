import "server-only";
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {nativeWorkflowReadFirst} from './native-workflow-read';
import {commandReceiptsProjectionSql} from './closed-projection-sources';
/** Fixed original owner evidence projections retained as cross-owner assertion
 * fragments. The command evidence image below is planned by the mounted
 * Workflow wave3 command planner and carries no SQL. */
export const projectWorkerCommandEvidence={receipts:commandReceiptsProjectionSql(['command_id','operation','record_id','actor_id','authority_json','result_json']),audit:'SELECT command_id,action,record_id,record_version FROM audit_events',outbox:'SELECT id,command_id,event_type,aggregate_id,aggregate_version,payload_json FROM outbox_events'} as const;
export async function projectWorkerCommandEvidenceImage(database:SqlDatabase,companyId:string,recordId:string){
  const row=await nativeWorkflowReadFirst<{image:string}>(
    'wf_native_project_worker_command_evidence',
    [recordId,companyId,recordId,companyId,recordId,companyId],
    companyId,
    database,
  );
  return row?.image??null;
}
