import 'server-only';
import {projectSocialPurchaseSources} from '../../projects/public/social-purchase-sources';
import {socialPmsSourceFacts} from '../../pms/public/social-source-facts';
import {workOperationsSettlementSources} from '../../operations/public/work-package-obligation-sources';
export function operationalHandoverBindings(alias:'p'|'post'){return `(SELECT COALESCE(jsonb_agg(jsonb_build_object('kind',source.source_kind,'id',source.source_id) ORDER BY source.ordinal),'[]'::jsonb) FROM social_post_sources source WHERE source.post_id=${alias}.id)`;}
/** Only the immutable original Operations publication receipt proves this
 * foundation. A caller-supplied purpose, source label or key cannot mint it. */
export function operationalHandoverReceiptPredicate(alias:'p'|'post'){return `${alias}.purpose='handover' AND ${alias}.audience_kind='participants' AND EXISTS(SELECT 1 FROM command_receipts operational_receipt WHERE operational_receipt.operation='work.handover.publish' AND operational_receipt.record_id=${alias}.id AND operational_receipt.actor_id=${alias}.author_id AND operational_receipt.authority_json::jsonb->>'companyId'=${alias}.company_id AND operational_receipt.result_json::jsonb->>'id'=${alias}.id AND operational_receipt.result_json::jsonb->'sourceBindings'=${operationalHandoverBindings(alias)})`;}
/** Code-owned aliases only. Every source must resolve to the included native
 * owner; one foreign label never licenses mixed ordinary or missing sources. */
export function workSocialNativePostPredicate(alias:'p'|'post'){
 const included=`EXISTS(SELECT 1 FROM (${projectSocialPurchaseSources}) included_project WHERE included_project.source_kind=native_source.source_kind AND included_project.source_id=native_source.source_id AND included_project.company_id=${alias}.company_id) OR native_source.source_kind='pms_record' AND EXISTS(SELECT 1 FROM (${socialPmsSourceFacts}) included_pms WHERE included_pms.id=native_source.source_id AND included_pms.company_id=${alias}.company_id)`;
 return `${alias}.kind<>'system' AND NOT (${operationalHandoverReceiptPredicate(alias)}) AND (NOT EXISTS(SELECT 1 FROM social_post_sources native_source WHERE native_source.post_id=${alias}.id) OR EXISTS(SELECT 1 FROM social_post_sources native_source WHERE native_source.post_id=${alias}.id AND NOT (${included})))`;
}
/** A historical publish receipt cannot prove an orphaned business source has
 * settled. Raw Company obligations retain it even when no actor can see it. */
export function workSocialUnresolvedPostPredicate(){return `EXISTS(SELECT 1 FROM social_post_sources unresolved_source WHERE unresolved_source.post_id=p.id AND NOT (unresolved_source.source_kind='staff_task' AND EXISTS(SELECT 1 FROM (${workOperationsSettlementSources.tasks}) source_task WHERE source_task.id=unresolved_source.source_id AND source_task.company_id=p.company_id) OR EXISTS(SELECT 1 FROM (${projectSocialPurchaseSources}) source_project WHERE source_project.source_kind=unresolved_source.source_kind AND source_project.source_id=unresolved_source.source_id AND source_project.company_id=p.company_id) OR unresolved_source.source_kind='pms_record' AND EXISTS(SELECT 1 FROM (${socialPmsSourceFacts}) source_pms WHERE source_pms.id=unresolved_source.source_id AND source_pms.company_id=p.company_id)))`;}
