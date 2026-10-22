import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SocialPostSourceEntry,SocialPostEntryScope} from '@erpx/contracts/social-post-entry';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {readPurchasedAccess} from '@erpx/server-commercial/public/purchase-command-boundary';
import {scheduledFeaturePredicate} from '@erpx/server-pms/public/hospitality-rollout';
import {projectSocialPurchaseSources} from '@erpx/server-projects/public/social-purchase-sources';
import {socialPmsSourceFacts} from '@erpx/server-pms/public/social-source-facts';
import {workSocialProductReadScope} from '@erpx/server-collaboration/adapters/work-social-purchases';
import {sourceAccessSql} from '@erpx/server-collaboration/adapters/social-source';
/** Candidate entry reads actual native source/current assignment version. It
 * does not choose a role or construct a session authority from its candidate. */
export async function socialPostProductSourceEntry(database:SqlDatabase,scope:SocialPostEntryScope,target:SocialPostSourceEntry){
 if(!Number.isSafeInteger(scope.assignmentVersion)||scope.assignmentVersion<1)return null;
 const purchase=await workSocialProductReadScope(database,scope);
 const row=await database.prepare(`${purchase.prefix}SELECT 1 FROM social_posts p WHERE p.id=? AND p.company_id=? AND p.kind<>'system' AND p.state='published' AND ${purchase.where}
 AND EXISTS(SELECT 1 FROM social_purchase_actor WHERE EXISTS(SELECT 1 FROM assignments g WHERE g.id=social_purchase_actor.assignment_id AND g.version=?) AND ${scheduledFeaturePredicate('p.company_id','collaboration')}
 AND ${sourceAccessSql('p','social_purchase_actor.assignment_id',{userId:'social_purchase_actor.user_id',now:'social_purchase_actor.now_ms'})}
 AND EXISTS(SELECT 1 FROM social_post_sources source WHERE source.post_id=p.id AND (source.source_kind='pms_record' AND EXISTS(SELECT 1 FROM (${socialPmsSourceFacts}) native WHERE native.id=source.source_id AND native.company_id=p.company_id) OR EXISTS(SELECT 1 FROM (${projectSocialPurchaseSources}) native WHERE native.source_kind=source.source_kind AND native.source_id=source.source_id AND native.company_id=p.company_id AND ${scheduledFeaturePredicate('native.company_id','projects')}))))`).bind(...purchase.args,target.id,scope.companyId,scope.assignmentVersion).first();
 return row?{purchasePurpose:'history' as const}:null;
}
/** The exact page repeats source entry under the selected native actor. A
 * source-only page never loads Social discovery or a Work queue snapshot. */
export async function socialPostSourceOnly(actor:WorkContext,id:string){
 const database=(await holdingResources(actor.holdingId)).write,access=await readPurchasedAccess(database,'enforced',{companyId:actor.companyId,capability:'work.social'});
 if(access.history)return false;
 const version=actor.authority?.assignmentVersion??actor.iam?.queuedAssignmentVersion??await database.prepare('SELECT version FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role=?').bind(actor.assignmentId,actor.userId,actor.companyId,actor.role).first<number>('version');
 return typeof version==='number'&&!!await socialPostProductSourceEntry(database,{companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,userId:actor.userId,assignmentId:actor.assignmentId,assignmentVersion:version,role:actor.role},{kind:'post',id});
}
