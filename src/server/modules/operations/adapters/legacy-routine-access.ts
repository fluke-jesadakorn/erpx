import 'server-only';
import {createHash} from 'node:crypto';
import {permissionForRoles} from '@/lib/identity_access/permissions';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {StaffPolicyIdentityAccessFacts} from '../../identity_access/public/operations-staff-policy-facts';
import type {LegacyRoutineAccess} from '../../identity_access/public/rules/legacy-module-access';
import type {TemplateRow} from '../domain/routines';

/** One-off license evidence only. Actual commands still require their original
 * staff, creator, independent review and task configuration policies. */
export function lockLegacyRoutineAccess(database:SqlDatabase){
 return database.prepare('LOCK TABLE routine_templates IN SHARE MODE').run();
}

/** Original activeStaff uses inclusive UTC employment dates. Re-review at the
 * next date boundary even if no lifecycle mutation changes an owner version. */
export function nextLegacyRoutineTransition(observedAt:number){
 return (Math.floor(observedAt/86_400_000)+1)*86_400_000;
}

export async function legacyRoutineAccess(database:SqlDatabase,companyId:string,assignment:{id:string;userId:string},now=Date.now()):Promise<LegacyRoutineAccess>{
 const original=StaffPolicyIdentityAccessFacts(database),staff=(await original.readReadActiveStaffResults(companyId,now,now,assignment.userId)).results;
 const owner=!!await original.readCompanyAuthorityEffect(assignment.id,companyId,permissionForRoles(['holding_owner']),now,now);
 // These are exactly the original get/list visibility alternatives. A private
 // orphan, another creator or a Project routine is not standalone coverage.
 const templates=(await database.prepare('SELECT t.* FROM routine_templates t WHERE t.company_id=? AND t.project_id IS NULL AND (t.published=1 OR t.creator_user_id=? OR ?=1) ORDER BY t.id')
  .bind(companyId,assignment.userId,owner?1:0).all<TemplateRow>()).results;
 const image={staff:staff.map(row=>({personId:row.person_id,engagementId:row.engagement_id,version:row.engagement_version,companyId:row.company_id,branchId:row.branch_id,departmentId:row.department_id,start:row.start_date,end:row.end_date})).sort((a,b)=>a.engagementId.localeCompare(b.engagementId)),templates};
 return {source:'operations/original-routines/v1',activeStaff:staff.length===1,retainedTemplates:templates.length,revision:createHash('sha256').update(JSON.stringify(image)).digest('hex')};
}
