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
export function stageWorkflowInvoicingNotification(db:SqlDatabase,id:string,now:number,journalId:string){return db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
    SELECT ?,o.company_id,r.steps_json::jsonb -> 'invoicing' ->> 'primaryUserId','order',o.id,o.version,'Goods movement GL posted: prepare the invoice',? ${commerceSources.workflow_notification_effects_commerce_orders_o_from} ${commerceSources.workflow_notification_effects_commerce_fulfillments_f_join} ON f.order_id=o.id ${workflowSources.workflow_notification_effects_flow_routes_r_join} ON r.company_id=o.company_id AND r.kind='order' AND r.record_id=o.id ${financeSources.workflow_notification_effects_finance_journals_j_join} ON j.id=f.journal_id WHERE j.id=? AND j.status='posted' AND f.invoice_id IS NULL AND (r.steps_json::jsonb -> 'invoicing' ->> 'primaryUserId') IS NOT NULL ON CONFLICT DO NOTHING`).bind(id,now,journalId);}
