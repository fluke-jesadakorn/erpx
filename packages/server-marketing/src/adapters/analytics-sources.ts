import {crmSharedHotelAnalyticsSources} from '@erpx/server-crm/public/analytics-hotel-sources';
import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {assignmentReadSql} from '@erpx/server-identity_access/public/record-source-authority';
import {hasPermission} from '@erpx/feature-identity_access/lib/permissions';
import {commercePackageEnforcementMode} from '@erpx/server-commercial/public/purchase-command-boundary';
import {purchasedCapabilityPredicate} from '@erpx/server-commercial/public/purchased-capability-sql';
import {marketingClockBody} from './materialized-clock-body-sql';
import {crmMarketingCandidateSources} from '@erpx/server-marketing/adapters/crm-purchase-sources';
import {crmHotelReservationSources} from '@erpx/server-pms/public/crm-purchase-sources';
import {marketingModuleMemberSql} from './module-membership';
import {marketingRecordScopeAuthoritySql} from './record-scope-authority';
import {marketingCandidateSourceSql} from './marketing-candidate-source';
import {marketingAnalyticsCostRows} from './analytics-cost-rows';
import {marketingAnalyticsCampaignSql} from './analytics-campaign-source-sql';
import {marketingAnalyticsTouchSql} from './analytics-touch-source-sql';
const touchRows=`SELECT touch.* FROM marketing_touches touch JOIN crm_leads lead ON lead.id=touch.lead_id AND lead.company_id=touch.company_id LEFT JOIN marketing_campaigns campaign ON campaign.id=touch.campaign_id AND campaign.company_id=touch.company_id WHERE (touch.campaign_id IS NULL OR campaign.id IS NOT NULL) AND (touch.source_kind='staff' AND touch.source_id=lead.id OR touch.source_kind='form' AND EXISTS(SELECT 1 FROM (${crmMarketingCandidateSources}) candidate WHERE candidate.id=touch.source_id AND candidate.company_id=touch.company_id AND candidate.crm_lead_id=touch.lead_id) OR touch.source_kind='pms' AND EXISTS(SELECT 1 FROM (${crmHotelReservationSources}) reservation WHERE reservation.id=touch.source_id AND reservation.company_id=touch.company_id AND reservation.branch_id=lead.branch_id))`;
/** Fixed source declarations for CRM reporting only. Original Marketing module
 * selection and current record authority are evaluated inside every statement. */
export async function marketingAnalyticsSource(database:SqlDatabase,actor:WorkContext,kind:'campaign'|'candidate'|'touch'|'cost'){
 const definitions={campaign:marketingAnalyticsCampaignSql(true),candidate:crmMarketingCandidateSources,touch:marketingAnalyticsTouchSql(true,touchRows),cost:marketingAnalyticsCostRows('marketing_campaign_costs cost JOIN marketing_campaigns campaign ON campaign.id=cost.campaign_id AND campaign.company_id=cost.company_id')},legacy={campaign:marketingAnalyticsCampaignSql(false),candidate:marketingCandidateSourceSql,touch:marketingAnalyticsTouchSql(false,touchRows),cost:marketingAnalyticsCostRows('marketing_campaign_costs cost')};
 if(await commercePackageEnforcementMode()!=='enforced')return {sql:legacy[kind],args:[] as unknown[]};
 const history=purchasedCapabilityPredicate('marketing_report.company_id','sales.marketing','history','marketing_report_clock.now_ms');
 return {sql:`WITH marketing_report_clock AS MATERIALIZED${marketingClockBody('')} SELECT marketing_report.* FROM (${definitions[kind]}) marketing_report CROSS JOIN marketing_report_clock WHERE ${hasPermission(actor,'records.read')&&actor.modules.includes('marketing')?'TRUE':'FALSE'} AND marketing_report.company_id=? AND (marketing_report.branch_id IS NULL OR ?::text IS NULL OR marketing_report.branch_id=?) AND (marketing_report.department_id IS NULL OR ?::text IS NULL OR marketing_report.department_id=?) AND ${history} ${kind==='touch'?`AND (marketing_report.source_kind<>'pms' OR (${hasPermission(actor,'pms.read')?'TRUE':'FALSE'} AND ${purchasedCapabilityPredicate('marketing_report.company_id','pms.reservations','history','marketing_report_clock.now_ms')} AND EXISTS(SELECT 1 FROM (${crmSharedHotelAnalyticsSources}) shared WHERE shared.company_id=marketing_report.company_id AND shared.lead_id=marketing_report.lead_id AND shared.reservation_id=marketing_report.source_id) AND ${assignmentReadSql('marketing_report','pms.read')}))`:''} AND ${marketingRecordScopeAuthoritySql('marketing_report','marketing_scope')} AND EXISTS(${marketingModuleMemberSql('selected')})`,args:[actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId,...(kind==='touch'?[actor.assignmentId,actor.userId,actor.role]:[]),actor.branchId,actor.departmentId,actor.assignmentId,actor.userId,actor.role,actor.assignmentId]};
}
