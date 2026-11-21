import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {ClaimedDraftRequest} from '@erpx/server-organization/public/draft-contract';
import type {DraftDefinition} from "@erpx/feature-organization/lib/organization-draft";
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {workflowWave3Statement} from './native-workflow-wave3';
export async function appendOrganizationDraftEvents(db:SqlDatabase,row:ClaimedDraftRequest,actor:WorkContext,definition:DraftDefinition,affectedCount:number,now:number){
        await (await workflowWave3Statement(db,{kind:'company',id:row.company_id},'wf_org_draft_audit',[`audit-${row.id}`,row.id,actor.userId,actor.assignmentId,row.id,definition.reason,now],`org-draft-audit:${row.id}`,now)).run();
        await (await workflowWave3Statement(db,{kind:'company',id:row.company_id},'wf_org_draft_outbox',[`event-${row.id}`,row.id,row.id,JSON.stringify({companyId:row.company_id,draftId:row.draft_id,affectedCount:affectedCount,state:'applied'}),now],`org-draft-outbox:${row.id}`,now)).run();

}
