import {collaborationNotificationSnapshotSources as workflowSnapshotSources} from '@erpx/server-workflow/public/collaboration-notification-snapshot-sources';
import {collaborationNotificationSnapshotSources as commerceSnapshotSources} from '@erpx/server-commerce/public/collaboration-notification-snapshot-sources';
import {collaborationNotificationSnapshotSources as financeSnapshotSources} from '@erpx/server-finance/public/collaboration-notification-snapshot-sources';
const workflowSources=workflowSnapshotSources();
const commerceSources=commerceSnapshotSources();
const financeSources=financeSnapshotSources();
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {FlowKind} from "@erpx/feature-shared/lib/flow";
/** Fixed projections supplied only by the native Workflow composition. */
export function stageWorkflowNotification(db:SqlDatabase,input:{id:string;kind:FlowKind;now:number;companyId:string;recordId:string;actorId:string;documentTable:string;recipientSql:string}){
 const {recipientSql:recipient,documentTable}=input;
 return db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
    SELECT ?,d.company_id,${recipient},?,d.id,d.version,?,? FROM ${documentTable} d ${workflowSources.workflow_notification_effects_flow_routes_r_join} ON r.company_id=d.company_id AND r.kind=? AND r.record_id=d.id
    WHERE d.company_id=? AND d.id=? AND (${recipient}) IS NOT NULL AND (${recipient})!=? ON CONFLICT DO NOTHING`).bind(input.id,input.kind,'Workflow updated: review your next action',input.now,input.kind,input.companyId,input.recordId,input.actorId);
}
