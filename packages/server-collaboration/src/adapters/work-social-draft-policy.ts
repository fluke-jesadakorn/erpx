import { actorScope, socialOwnerModes, requireWorkSocialRead } from '@erpx/server-collaboration/adapters/work-social-read-policy';
import { units } from '@erpx/server-collaboration/adapters/work-social-unit-state';
import 'server-only';

import { randomUUID } from 'node:crypto';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

import {ApiError} from '@erpx/server-runtime/public/http';
import type {StaffActor} from '@erpx/server-operations/public/staff-policy';
import { workOperationsSettlementSources } from '@erpx/server-operations/public/work-package-obligation-sources';
import { projectSocialPurchaseSources } from '@erpx/server-projects/public/social-purchase-sources';

import { socialPmsSourceFacts } from '@erpx/server-pms/public/social-source-facts';
import { pmsSocialSourcePurchaseScopes } from '@erpx/server-pms/public/social-source-purchases';

import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";

import {iamMutationGuards} from '@erpx/server-identity_access/public/iam-store';
import {workContextGuards} from '@erpx/server-workflow/public/command-guards';

import { workPackageEnforcementMode, projectsPackageEnforcementMode, readPurchasedAccesses, checkPurchasedCapability } from '@erpx/server-commercial/public/purchase-command-boundary';
import type {CapabilityPurpose} from '@erpx/server-commercial/public/rules/packages';
import { operationalHandoverReceiptPredicate } from '@erpx/server-collaboration/adapters/work-social-source';

export type Source={kind:'staff_task'|'staff_project'|'pms_record';id:string};
export type Command={key:string;operation:string;purpose:CapabilityPurpose;type:'post'|'draft'|'story';id?:string;sources?:Source[];audienceProjectId?:string|null;publishedDraftReplay?:boolean;privateImages?:true;providerSources?:true};
export async function current(database:SqlDatabase,actor:StaffActor,key:string){const scope=actorScope(actor);if(!await database.prepare(`${scope.prefix}SELECT 1 FROM social_purchase_actor WHERE ${scheduledFeaturePredicate('company_id','collaboration')}`).bind(...scope.args).first())throw new ApiError(403,'WORK_SOCIAL_AUTHORITY_CHANGED','The original current Social account or role changed.');await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)]);}
export async function actualSources(database:SqlDatabase,actor:Pick<StaffActor,'companyId'>,command:Command):Promise<Source[]>{
 const rows:Source[]=[];if(command.id&&command.type==='post')rows.push(...(await database.prepare('SELECT source_kind kind,source_id id FROM social_post_sources s JOIN social_posts p ON p.id=s.post_id WHERE p.id=? AND p.company_id=? ORDER BY s.ordinal').bind(command.id,actor.companyId).all<Source>()).results);
 if(command.id&&command.type==='draft'){const draft=await database.prepare('SELECT sources_json,COALESCE(comment_parent_post_id,edit_post_id,published_post_id) post_id FROM social_post_drafts WHERE id=? AND company_id=?').bind(command.id,actor.companyId).first<{sources_json:string;post_id:string|null}>();if(draft){rows.push(...JSON.parse(draft.sources_json) as Source[]);if(draft.post_id)rows.push(...(await database.prepare('SELECT source_kind kind,source_id id FROM social_post_sources s JOIN social_posts p ON p.id=s.post_id WHERE p.id=? AND p.company_id=? ORDER BY s.ordinal').bind(draft.post_id,actor.companyId).all<Source>()).results);}}
 rows.push(...command.sources??[]);return [...new Map(rows.map(row=>[`${row.kind}:${row.id}`,row])).values()];
}
export async function capabilities(database:SqlDatabase,actor:StaffActor,command:Command,sources:Source[],purpose:CapabilityPurpose){
 const rows=(await database.prepare(`WITH requested AS(SELECT kind,id FROM jsonb_to_recordset(?::jsonb) AS r(kind text,id text)) SELECT r.kind,r.id,p.project_id,(r.kind='pms_record' AND EXISTS(SELECT 1 FROM (${socialPmsSourceFacts}) s WHERE s.id=r.id AND s.company_id=?)) pms,t.id task_id,t.template_id FROM requested r LEFT JOIN (${projectSocialPurchaseSources}) p ON p.source_kind=r.kind AND p.source_id=r.id AND p.company_id=? LEFT JOIN (${workOperationsSettlementSources.tasks}) t ON r.kind='staff_task' AND t.id=r.id AND t.company_id=?`).bind(JSON.stringify(sources),actor.companyId,actor.companyId,actor.companyId).all<{kind:string;id:string;project_id:string|null;pms:boolean;task_id:string|null;template_id:string|null}>()).results;
 const handover=units.get(database)?.handover,typedUnit=handover&&(command.operation==='work.handover.publish'&&command.key===`${handover.key}-operations`||command.operation==='social.draft.save'&&command.key===handover.key&&command.id===`post-draft-${handover.key}`||command.operation==='social.post.publish'&&command.key===`${handover.key}-publish`&&command.id===`post-draft-${handover.key}`),typedPost=command.type==='post'&&command.id?await database.prepare(`SELECT 1 FROM social_posts p WHERE p.id=? AND p.company_id=? AND ${operationalHandoverReceiptPredicate('p')}`).bind(command.id,actor.companyId).first():null;
 const codes=new Set<string>(),project=await projectsPackageEnforcementMode(),social=(await readPurchasedAccesses(database,await workPackageEnforcementMode(),actor.companyId,['work.social']))['work.social'],socialAllowed=purpose==='new-work'?social.newWork:purpose==='settle-existing'?social.settleExisting:social.history;
 if(typedUnit||typedPost){for(const row of rows){if(row.kind==='staff_task'&&row.task_id&&!row.project_id){codes.add('work.actions');if(row.template_id)codes.add('work.advanced-routines');}else if(row.project_id&&project==='enforced'){codes.add('projects.wbs');codes.add('projects.handover');if(row.kind==='staff_task')codes.add('projects.tasks');}else if(!row.pms&&!row.project_id)codes.add('work.social');}return [...codes];}
 if(socialAllowed||!rows.length||rows.some(row=>!row.project_id&&!row.pms))codes.add('work.social');
 // System posts are original owner projections rather than human freeform.
 if(command.type==='post'&&command.id&&await database.prepare("SELECT 1 FROM social_posts WHERE id=? AND company_id=? AND kind='system'").bind(command.id,actor.companyId).first())codes.delete('work.social');
 for(const row of rows)if(row.project_id&&project==='enforced'){codes.add('projects.wbs');if(row.kind==='staff_task')codes.add('projects.tasks');}
 return [...codes];
}
export async function requireWorkSocialDraftRead(database:SqlDatabase,actor:StaffActor,id:string,edit=false){
 const {work:mode,project,disabled}=await socialOwnerModes();if(disabled)return;const row=await database.prepare('SELECT comment_parent_post_id,edit_post_id,published_post_id FROM social_post_drafts WHERE id=? AND company_id=?').bind(id,actor.companyId).first<{comment_parent_post_id:string|null;edit_post_id:string|null;published_post_id:string|null}>();if(!row)throw new ApiError(404,'DRAFT_NOT_FOUND','Draft unavailable.');
 const command:Command={key:'read',operation:'read',type:'draft',id,purpose:edit?row.comment_parent_post_id?'settle-existing':'new-work':'history'},sources=await actualSources(database,actor,command);await current(database,actor,`work-social-draft:${randomUUID()}`);for(const capability of await capabilities(database,actor,command,sources,command.purpose))await checkPurchasedCapability(database,capability.startsWith('projects.')?project:mode,{companyId:actor.companyId,capability,purpose:command.purpose});await pmsSocialSourcePurchaseScopes(database,actor,sources.filter(source=>source.kind==='pms_record').map(source=>source.id),command.purpose);const post=row.comment_parent_post_id??row.edit_post_id??row.published_post_id;if(post)await requireWorkSocialRead(database,actor,post);
}
