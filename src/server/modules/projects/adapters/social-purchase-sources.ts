import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {workOperationsSettlementSources} from '../../operations/public/work-package-obligation-sources';
import {handoverTaskAccessSql} from '../../operations/public/handover-policy';
/** Actual native source identity. An ordinary Task or missing Project never
 * becomes included Project work because a caller supplied its source label. */
export const projectSocialPurchaseSources=`SELECT 'staff_project'::text source_kind,p.id source_id,p.company_id,p.id project_id,p.owner_user_id,jsonb_build_object('project',to_jsonb(p),'members',(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM staff_project_members m WHERE m.project_id=p.id))::text source_image
 FROM staff_projects p UNION ALL SELECT 'staff_task',t.id,t.company_id,p.id,p.owner_user_id,jsonb_build_object('task',to_jsonb(t),'project',to_jsonb(p),'members',(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM staff_project_members m WHERE m.project_id=p.id))::text
 FROM (${workOperationsSettlementSources.tasks}) t JOIN staff_projects p ON p.id=t.project_id AND p.company_id=t.company_id`;
/** Fixed social source/viewer aliases preserve each original owner policy:
 * explicit Project membership, or the native Task participant categories. */
export function projectSocialSourceReadPolicy(){return `(social_project_source.source_kind='staff_project' AND (social_project_source.owner_user_id=social_purchase_actor.user_id OR EXISTS(SELECT 1 FROM staff_project_members member WHERE member.project_id=social_project_source.project_id AND member.user_id=social_purchase_actor.user_id)) OR social_project_source.source_kind='staff_task' AND EXISTS(SELECT 1 FROM (${workOperationsSettlementSources.tasks}) social_project_task WHERE social_project_task.id=social_project_source.source_id AND social_project_task.company_id=social_project_source.company_id AND ${handoverTaskAccessSql('social_project_task','social_purchase_actor.user_id')}))`;}
export function projectSocialSourceReadLocks(database:SqlDatabase){return database.prepare('LOCK TABLE staff_projects,staff_project_members IN SHARE MODE');}

/** Private evidence reads retain actual Project identity and membership while
 * the Operations owner consumes bytes and completes its access audit. */
export const projectTaskAttachmentReadSources=`SELECT task.id task_id,task.company_id,
 jsonb_build_object('project',to_jsonb(project),'members',(SELECT jsonb_agg(to_jsonb(member) ORDER BY member.user_id) FROM staff_project_members member WHERE member.project_id=project.id)) image
 FROM (${workOperationsSettlementSources.tasks}) task JOIN staff_projects project ON project.id=task.project_id AND project.company_id=task.company_id`;

/** Private-file access audits can have deferred owner effects. */
export function projectSocialAttachmentSourceReadLocks(database:SqlDatabase){return database.prepare('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE');}
