import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {ClaimedDraftRequest} from '../../organization/public/draft-contract';
import type {DraftDefinition} from "@/lib/organization/organization-draft";
import type {WorkContext} from '../../identity_access/public/authorization';
export async function appendOrganizationDraftEvents(db:SqlDatabase,row:ClaimedDraftRequest,actor:WorkContext,definition:DraftDefinition,affectedCount:number,now:number){
        await db.prepare("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at) VALUES(?,?,?,?,'organization.draft.applied','organization_draft_request',?,1,?,?)")
          .bind(`audit-${row.id}`,row.id,actor.userId,actor.assignmentId,row.id,definition.reason,now).run();
        await db.prepare("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES(?,?,'organization.draft.applied','organization_draft_request',?,1,?,?)")
          .bind(`event-${row.id}`,row.id,row.id,JSON.stringify({companyId:row.company_id,draftId:row.draft_id,affectedCount:affectedCount,state:'applied'}),now).run();

}
