import 'server-only';
import {createHash} from 'node:crypto';
import type {PostgresDatabase} from '@erpx/server-runtime/public/runtime/postgres';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {packageSettlementSource,type PackageSettlementSourceLock} from '@erpx/server-commercial/public/package-settlement-sources';
import {lockMarketingFilePackageObligationSources,marketingFilePackageObligationFacts} from '@erpx/server-files/public/marketing-package-obligation-sources';
import {crmMarketingCandidateSources} from '@erpx/server-marketing/adapters/crm-purchase-sources';
import {marketingPlainSourceSql} from './marketing-plain-source-sql';
import {marketingSelectHeadSql} from './select-head-sql';
import {marketingPointExistsSql} from './marketing-point-exists-sql';
import {MARKETING_RD_PACKAGE_OBLIGATIONS,nativeMarketingReadAll} from './native-marketing-read';
type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};const sources=new WeakMap<SqlDatabase,string>();
export async function lockMarketingPackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){const {database,companyId}=packageSettlementSource(owner,lock);await database.prepare('LOCK TABLE marketing_assets,marketing_asset_events,marketing_lead_candidates,marketing_engagement_events,marketing_campaigns,marketing_campaign_costs,marketing_touches,marketing_public_receipts,marketing_public_limits,command_receipts IN SHARE MODE').run();await lockMarketingFilePackageObligationSources(owner,lock);sources.set(database,companyId);}
export function assertMarketingPackageObligationSources(owner:PostgresDatabase,database:SqlDatabase,companyId:string){owner.assertActiveTransaction(database);if(sources.get(database)!==companyId)throw Error('MARKETING_PACKAGE_OBLIGATION_SOURCE_REQUIRED');}
/** Owner-local fixed settlement source projections. Each admitted variant emits
 * the original byte-identical statement; the shared plain-source frame keeps the
 * candidate reader and every settlement reader on one projection literal. A
 * caller picks a variant and never supplies raw SQL or a relation name. */
const settlementSourceRows=(source:'engagements'|'costs'|'touches'|'campaigns'):string=>marketingPlainSourceSql(source);
/** Raw native sources retain invalid lineage instead of filtering it into zero. */
export const marketingPackageSettlementSources={
 candidates:marketingSelectHeadSql(`c.*,valid.id IS NOT NULL AND EXISTS(${marketingPointExistsSql(`marketing_assets form JOIN marketing_asset_events activated ON activated.asset_id=form.id AND activated.version=c.form_version AND activated.action='activate' AND activated.actor_id<>form.created_by JOIN command_receipts command ON activated.id='marketing-asset-event-'||command.command_id AND command.operation='marketing.asset.activate' AND command.record_id=form.id AND command.actor_id=activated.actor_id AND command.authority_json::jsonb->>'companyId'=c.company_id`,'form.id=c.form_id AND form.company_id=c.company_id')}) source_valid`,` FROM marketing_lead_candidates c LEFT JOIN (${crmMarketingCandidateSources}) valid ON valid.id=c.id AND valid.company_id=c.company_id`),
 engagements:settlementSourceRows('engagements'),costs:settlementSourceRows('costs'),touches:settlementSourceRows('touches'),campaigns:settlementSourceRows('campaigns'),
};
export async function marketingPackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 assertMarketingPackageObligationSources(owner,database,input.companyId);if(input.moduleId!=='marketing'||input.packageId!=='sales-marketing'||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)throw Error('MARKETING_PACKAGE_OBLIGATION_SCOPE_REQUIRED');
 const native=(await nativeMarketingReadAll<{kind:string;id:string;fact:unknown;pending:boolean}>(MARKETING_RD_PACKAGE_OBLIGATIONS,{companyId:input.companyId},database)).results,files=await marketingFilePackageObligationFacts(owner,database,input.companyId),known=new Set(native.filter(fact=>fact.kind==='asset'||fact.kind==='campaign').map(fact=>`${fact.kind}:${fact.id}`)),facts=[...native,...files.map(fact=>({...fact,pending:fact.pending||!known.has(`${fact.target_type==='marketing_asset'?'asset':'campaign'}:${fact.target_id}`)}))];
 // Domain/provider activation has no native disconnect/callback reconciliation
 // receipt. Retiring its label cannot certify an external obligation as zero.
 return {companyId:input.companyId,packageId:input.packageId,moduleId:'marketing',observedAt:input.now,revision:createHash('sha256').update(JSON.stringify({companyId:input.companyId,packageVersion:input.packageVersion,facts})).digest('hex'),outstanding:facts.filter(fact=>fact.pending).length};
}
