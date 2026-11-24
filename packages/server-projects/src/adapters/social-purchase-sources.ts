import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {workOperationsSettlementSources} from '@erpx/server-operations/public/work-package-obligation-sources';
import {handoverTaskAccessSql} from '@erpx/server-operations/public/handover-policy';
import {projectOwnerOrMemberColumnSql} from '@erpx/server-projects/adapters/project-read-projections';
import {orderedImageAggregateHead} from './read-tail-r29';
/** Byte-identical members projection shared by both source arms and the
 * collaboration-home audience image; one definition keeps every source relation
 * and bind order identical. */
export const membersJsonSql=`'members',(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM staff_project_members m WHERE m.project_id=p.id)`;
/** Actual native source identity. An ordinary Task or missing Project never
 * becomes included Project work because a caller supplied its source label. */
export const projectSocialPurchaseSources=`SELECT 'staff_project'::text source_kind,p.id source_id,p.company_id,p.id project_id,p.owner_user_id,jsonb_build_object('project',to_jsonb(p),${membersJsonSql})::text source_image
 FROM staff_projects p UNION ALL SELECT 'staff_task',t.id,t.company_id,p.id,p.owner_user_id,jsonb_build_object('task',to_jsonb(t),'project',to_jsonb(p),${membersJsonSql})::text
 FROM (${workOperationsSettlementSources.tasks}) t JOIN staff_projects p ON p.id=t.project_id AND p.company_id=t.company_id`;
/** Fixed social source/viewer aliases preserve each original owner policy:
 * explicit Project membership, or the native Task participant categories. */
export function projectSocialSourceReadPolicy(){return `(social_project_source.source_kind='staff_project' AND ${projectOwnerOrMemberColumnSql('social_project_source','social_purchase_actor.user_id','member','project_id')} OR social_project_source.source_kind='staff_task' AND EXISTS(SELECT 1 FROM (${workOperationsSettlementSources.tasks}) social_project_task WHERE social_project_task.id=social_project_source.source_id AND social_project_task.company_id=social_project_source.company_id AND ${handoverTaskAccessSql('social_project_task','social_purchase_actor.user_id')}))`;}
export function projectSocialSourceReadLocks(database:SqlDatabase){return database.prepare('LOCK TABLE staff_projects,staff_project_members IN SHARE MODE');}

/** Private evidence reads retain actual Project identity and membership while
 * the Operations owner consumes bytes and completes its access audit. */
export const projectTaskAttachmentReadSources=`SELECT task.id task_id,task.company_id,
 jsonb_build_object('project',to_jsonb(project),'members',(${orderedImageAggregateHead('member','user_id')} FROM staff_project_members member WHERE member.project_id=project.id)) image
 FROM (${workOperationsSettlementSources.tasks}) task JOIN staff_projects project ON project.id=task.project_id AND project.company_id=task.company_id`;

/** Private-file access audits can have deferred owner effects. */
export function projectSocialAttachmentSourceReadLocks(database:SqlDatabase){return database.prepare('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE');}
