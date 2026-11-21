import {recordPeoplePlacementLevelHistory} from "@erpx/server-organization/public/people-placement-level-history";
import { nativeOrganizationDraftPlacement } from '@erpx/server-collaboration/adapters/native-organization-draft-placement';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {ClaimedDraftRequest} from '@erpx/server-organization/public/draft-contract';
import type {DraftDefinition} from "@erpx/feature-organization/lib/organization-draft";
import type {OrganizationAccessData} from "@erpx/feature-organization/lib/organization-access";
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {personPlacement} from "@erpx/feature-organization/lib/organization-draft";
export async function applyOrganizationDraftPlacement(db:SqlDatabase,row:ClaimedDraftRequest,actor:WorkContext,definition:DraftDefinition,person:OrganizationAccessData['people'][number],before:OrganizationAccessData['people'][number],now:number){
          await nativeOrganizationDraftPlacement(db, row.company_id).engagement([person.position_id,person.branch_id,person.department_id,person.level_id,person.manager_person_id,person.job_title,now,row.company_id,person.engagement_id]);
          await nativeOrganizationDraftPlacement(db, row.company_id).employee([person.branch_id,person.department_id,row.company_id,person.person_id]);
          await nativeOrganizationDraftPlacement(db, row.company_id).history([`history-${row.id}-${person.engagement_id}`,row.company_id,person.engagement_id,JSON.stringify(personPlacement(before)),JSON.stringify({...personPlacement(person),effectiveAt:row.effective_at}),actor.userId,definition.reason,row.id,now]);
          if(before.level_id!==person.level_id)await recordPeoplePlacementLevelHistory(db,`level-${row.id}-${person.engagement_id}`,person.engagement_id,before.level_id,person.level_id,actor.userId,definition.reason,row.id,now);

}
