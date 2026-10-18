import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {StaffActor} from '../../operations/public/staff-policy';
import {pmsPackageEnforcementMode,readPurchasedAccesses,checkPurchasedCapability} from '../../commercial/public/purchase-command-boundary';
import {packageClockSql,purchasedCapabilityPredicate} from '../../commercial/public/purchased-capability-sql';
import type {CapabilityPurpose} from '../../commercial/public/rules/packages';
import {scheduledFeaturePredicate} from './hospitality-rollout';
import {pmsSourcesSql} from './pms/source-query';
import {ApiError} from '../../runtime/public/http';

const capabilities=['pms.reservations','pms.front-desk','pms.rooms','pms.housekeeping','pms.guest-accounts','pms.night-audit'] as const;
const capabilityByKind:Readonly<Record<string,typeof capabilities[number]>>={reservation:'pms.reservations',stay:'pms.front-desk',room:'pms.rooms',block:'pms.rooms',task:'pms.housekeeping',guest_account:'pms.guest-accounts',refund:'pms.guest-accounts',correction:'pms.guest-accounts',transfer:'pms.guest-accounts',shift:'pms.guest-accounts',night:'pms.night-audit'};
/** Closed aliases selected by the Social owner. A financial history fallback
 * is limited to actual guest accounts/transfers; it never licenses Hotel work. */
export function pmsSocialSourcePurchasePredicate(purpose:CapabilityPurpose){
 const purchased=(capability:string)=>purchasedCapabilityPredicate('social_pms_source.company_id',capability,purpose,'social_purchase_actor.now_ms');
 const financial=purpose==='history'?`social_pms_source.kind IN('guest_account','transfer') AND iam_role_allows(social_purchase_actor.role,social_pms_source.company_id,'finance.read') AND ${scheduledFeaturePredicate('social_pms_source.company_id','finance')} AND (${purchased('finance.hotel-posting')} OR ${purchased('finance.accounting')})`:'FALSE';
 const hotel=`CASE social_pms_source.kind ${Object.entries(capabilityByKind).map(([kind,capability])=>`WHEN '${kind}' THEN ${purchased(capability)}`).join(' ')} ELSE FALSE END`;
 return `((${scheduledFeaturePredicate('social_pms_source.company_id','pms')} AND (${hotel})) OR (${financial}))`;
}
/** Resolve only native persisted same-Company sources. Original PMS source
 * IAM/participant policy is independently repeated by Social before effects. */
export async function pmsSocialSourcePurchaseScopes(database:SqlDatabase,actor:Pick<StaffActor,'companyId'|'role'>,ids:readonly string[],purpose:CapabilityPurpose){
 const mode=await pmsPackageEnforcementMode();if(mode==='disabled'||!ids.length)return [];
 const rows=(await database.prepare(`SELECT id,kind FROM (${pmsSourcesSql}) social_pms_source WHERE company_id=? AND id IN(SELECT jsonb_array_elements_text(?::jsonb))`).bind(actor.companyId,JSON.stringify(ids)).all<{id:string;kind:string}>()).results;
 if(rows.length!==new Set(ids).size)throw new ApiError(404,'POST_NOT_FOUND','This post is unavailable.');
 const access=await readPurchasedAccesses(database,mode,actor.companyId,[...capabilities,'finance.hotel-posting','finance.accounting']);
 const allowed=(capability:string)=>purpose==='new-work'?access[capability].newWork:purpose==='settle-existing'?access[capability].settleExisting:access[capability].history;
 const scopes=new Set<string>();
 for(const row of rows){let capability:string|undefined=capabilityByKind[row.kind];if(!capability)throw new ApiError(404,'POST_NOT_FOUND','This post is unavailable.');
  if(!allowed(capability)&&purpose==='history'&&['guest_account','transfer'].includes(row.kind)&&await database.prepare("SELECT 1 WHERE iam_role_allows(?,?,'finance.read')").bind(actor.role,actor.companyId).first())capability=['finance.hotel-posting','finance.accounting'].find(allowed)??capability;
  await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability,purpose});scopes.add(capability);
 }
 if(mode==='enforced'&&!await database.prepare(`WITH social_purchase_actor AS(SELECT ?::text role,${packageClockSql} now_ms) SELECT 1 WHERE NOT EXISTS(SELECT 1 FROM (${pmsSourcesSql}) social_pms_source CROSS JOIN social_purchase_actor WHERE social_pms_source.company_id=? AND social_pms_source.id IN(SELECT jsonb_array_elements_text(?::jsonb)) AND NOT (${pmsSocialSourcePurchasePredicate(purpose)}))`).bind(actor.role,actor.companyId,JSON.stringify(ids)).first())throw new ApiError(403,'FEATURE_COMING_SOON','The original Hotel source is not currently released.');
 return [...scopes].map(capability=>({mode,companyId:actor.companyId,capability,purpose}));
}
