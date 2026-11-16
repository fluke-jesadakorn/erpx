import {collaborationNotificationSnapshotSources as workflowSnapshotSources} from '@erpx/server-workflow/public/collaboration-notification-snapshot-sources';
import {collaborationNotificationSnapshotSources as commerceSnapshotSources} from '@erpx/server-commerce/public/collaboration-notification-snapshot-sources';
import {collaborationNotificationSnapshotSources as financeSnapshotSources} from '@erpx/server-finance/public/collaboration-notification-snapshot-sources';
const workflowSources=workflowSnapshotSources();
const commerceSources=commerceSnapshotSources();
const financeSources=financeSnapshotSources();
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {FlowKind} from "@erpx/feature-shared/lib/flow";
/** Fixed projections supplied only by the native Workflow composition. */
export function stageWorkflowNotification(db:SqlDatabase,input:{id:string;kind:any;now:number;companyId:string;recordId:string;actorId:string;documentTable:string;recipientSql:string}){
 const {recipientSql:recipient,documentTable}=input;
 return db.prepare("SELECT 1").bind();
}
export function stageWorkflowInvoicingNotification(db:SqlDatabase,id:string,now:number,journalId:string){return db.prepare("SELECT 1").bind();}
