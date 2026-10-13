import {recordPeoplePlacementLevelHistory} from "../../organization/public/people-placement-level-history";
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {ClaimedDraftRequest} from '../../organization/public/draft-contract';
import type {DraftDefinition} from "@/lib/organization/organization-draft";
import type {OrganizationAccessData} from "@/lib/organization/organization-access";
import type {WorkContext} from '../../identity_access/public/authorization';
import {personPlacement} from "@/lib/organization/organization-draft";
export async function applyOrganizationDraftPlacement(db:SqlDatabase,row:ClaimedDraftRequest,actor:WorkContext,definition:DraftDefinition,person:OrganizationAccessData['people'][number],before:OrganizationAccessData['people'][number],now:number){
          await db.prepare("UPDATE hr_engagements SET position_id=?,branch_id=?,department_id=?,level_id=?,manager_person_id=?,job_title=?,version=version+1,updated_at=? WHERE company_id=? AND id=?")
            .bind(person.position_id,person.branch_id,person.department_id,person.level_id,person.manager_person_id,person.job_title,now,row.company_id,person.engagement_id).run();
          await db.prepare("UPDATE employees SET branch_id=?,department_id=? WHERE company_id=? AND person_id=?").bind(person.branch_id,person.department_id,row.company_id,person.person_id).run();
          await db.prepare("INSERT INTO staff_organization_history(id,company_id,engagement_id,before_json,after_json,actor_id,reason,command_id,created_at) VALUES(?,?,?,?,?,?,?,?,?)")
            .bind(`history-${row.id}-${person.engagement_id}`,row.company_id,person.engagement_id,JSON.stringify(personPlacement(before)),JSON.stringify({...personPlacement(person),effectiveAt:row.effective_at}),actor.userId,definition.reason,row.id,now).run();
          if(before.level_id!==person.level_id)await recordPeoplePlacementLevelHistory(db,`level-${row.id}-${person.engagement_id}`,person.engagement_id,before.level_id,person.level_id,actor.userId,definition.reason,row.id,now);

}
