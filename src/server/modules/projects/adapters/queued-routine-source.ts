import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';

/** Only the native queued Routine aliases are accepted. Explicit Project
 * membership keeps its original cross-branch policy; the exact captured
 * assignment/context has already been checked by the IAM owner predicate. */
export function queuedProjectRoutineSourcePredicate(){
 const actor='work_routine_authorization.authority_json::jsonb',user=`(${actor}->>'userId')`,role=`(${actor}->>'role')`;
 return `EXISTS(SELECT 1 FROM staff_projects routine_project WHERE routine_project.id=t.project_id AND routine_project.company_id=t.company_id AND iam_role_allows(${role},routine_project.company_id,'records.read') AND (routine_project.owner_user_id=${user} OR EXISTS(SELECT 1 FROM staff_project_members routine_member WHERE routine_member.project_id=routine_project.id AND routine_member.user_id=${user})))`;
}

/** A Routine only reads its Project source. Lease these original owner rows
 * before its first snapshot so a concurrent membership writer cannot pass
 * between selection and the complete original queued command's final check. */
export function queuedProjectRoutineSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE staff_projects,staff_project_members IN SHARE MODE');}
