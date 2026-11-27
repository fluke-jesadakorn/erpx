import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {SocialPostSourceEntry,SocialPostEntryScope} from '@erpx/contracts/social-post-entry';
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import {projectSocialPurchaseSources} from '@erpx/server-projects/public/social-purchase-sources';
import {socialPmsSourceFacts} from '@erpx/server-pms/public/social-source-facts';
import {workSocialProductReadScope} from '@erpx/server-collaboration/public/work-social-read-policy';
import {sourceAccessSql} from '@erpx/server-collaboration/public/social-source-policy';
import {socialPostByIdPredicate,socialPmsSourceFactExists} from './read-tail-sql';
/** Candidate entry reads actual native source/current assignment version. It
 * does not choose a role or construct a session authority from its candidate. */
export async function socialPostProductSourceEntry(database:SqlDatabase,scope:SocialPostEntryScope,target:SocialPostSourceEntry){
 if(!Number.isSafeInteger(scope.assignmentVersion)||scope.assignmentVersion<1)return null;
 const purchase=await workSocialProductReadScope(database,scope);
 const row=await database.prepare(`${purchase.prefix}${socialPostByIdPredicate(`p.kind<>'system' AND p.state='published' AND ${purchase.where}
 AND EXISTS(SELECT 1 FROM social_purchase_actor WHERE EXISTS(SELECT 1 FROM assignments g WHERE g.id=social_purchase_actor.assignment_id AND g.version=?) AND ${scheduledFeaturePredicate('p.company_id','collaboration')}
 AND ${sourceAccessSql('p','social_purchase_actor.assignment_id',{userId:'social_purchase_actor.user_id',now:'social_purchase_actor.now_ms'})}
 AND EXISTS(SELECT 1 FROM social_post_sources source WHERE source.post_id=p.id AND (source.source_kind='pms_record' AND ${socialPmsSourceFactExists(socialPmsSourceFacts,'native','source.source_id','p.company_id')} OR EXISTS(SELECT 1 FROM (${projectSocialPurchaseSources}) native WHERE native.source_kind=source.source_kind AND native.source_id=source.source_id AND native.company_id=p.company_id AND ${scheduledFeaturePredicate('native.company_id','projects')}))))`)}`).bind(...purchase.args,target.id,scope.companyId,scope.assignmentVersion).first();
 return row?{purchasePurpose:'history' as const}:null;
}
