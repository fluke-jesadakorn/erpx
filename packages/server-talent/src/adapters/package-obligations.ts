import "server-only";
import {createHash} from "node:crypto";
import type {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {packageSettlementSource,type PackageSettlementSourceLock} from "@erpx/server-commercial/public/package-settlement-sources";
import {talentSettlementPlacements,talentSettlementPlacementReadLocks} from "@erpx/server-organization/public/talent-settlement-sources";
import {existsGuardPrefix} from "./talent-read-tail-r27-sources";
import {selectProjection} from "./talent-read-tail-r37-sources";
type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};
const sources=new WeakMap<SqlDatabase,string>();
/** Owner-local EXISTS-row family over the same Recruitment source rows. The
 * prefix is shared with the r27 version-guard family; the FROM source and WHERE
 * predicate are parameterized and the emitted SQL stays byte-identical. */
const existsRow=(source:string,where:string)=>`${existsGuardPrefix(source)}${where})`;
const requisitions=`${selectProjection(`r.*,${existsRow(`(${talentSettlementPlacements}) branch`,`branch.id=r.branch_id AND branch.company_id=r.company_id AND branch.kind IN ('branch','headquarters')`)} AND (r.department_id IS NULL OR ${existsRow(`(${talentSettlementPlacements}) department`,`department.id=r.department_id AND department.company_id=r.company_id AND department.branch_id=r.branch_id AND department.kind='department'`)}) source_valid`)} FROM talent_requisitions r`;
const applications=`${selectProjection(`a.*,COALESCE(r.source_valid AND a.requisition_version<=r.version AND c.id IS NOT NULL,false) source_valid`)} FROM talent_applications a LEFT JOIN (${requisitions}) r ON r.id=a.requisition_id AND r.company_id=a.company_id LEFT JOIN talent_candidates c ON c.id=a.candidate_id AND c.company_id=a.company_id`;
const offerEventPrefix=`EXISTS(${selectProjection('1')} FROM talent_offer_events event WHERE event.offer_id=o.id AND event.version`;
const offers=`${selectProjection(`o.*,COALESCE(a.source_valid AND o.application_version<=a.version,false) source_valid,${offerEventPrefix}<=o.version AND event.status=o.status AND event.action=CASE o.status WHEN 'declined' THEN 'decline' WHEN 'withdrawn' THEN 'withdraw' WHEN 'accepted' THEN 'accept-offer' ELSE '' END) terminal_evidence`)} FROM talent_offers o LEFT JOIN (${applications}) a ON a.id=o.application_id AND a.company_id=o.company_id`;
/** Raw owned rows remain present when lineage is broken. Included People
 * settlement consumes these facts; valid-only projections cannot produce zero. */
export const talentPackageSettlementSources={requisitions,applications,offers,
 handoffs:`${selectProjection(`h.*,o.status offer_status,o.version offer_version,o.currency,o.base_amount_minor,o.start_date,c.version candidate_version,COALESCE(o.source_valid AND o.status='accepted' AND a.status='accepted' AND h.candidate_id=a.candidate_id AND h.branch_id=r.branch_id AND h.department_id IS NOT DISTINCT FROM r.department_id AND h.source_offer_version<=o.version AND ${offerEventPrefix}=h.source_offer_version AND event.action='accept-offer' AND event.status='accepted'),false) source_valid`)} FROM talent_hire_handoffs h LEFT JOIN (${offers}) o ON o.id=h.offer_id AND o.company_id=h.company_id LEFT JOIN talent_applications a ON a.id=o.application_id AND a.company_id=h.company_id LEFT JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=h.company_id LEFT JOIN talent_candidates c ON c.id=h.candidate_id AND c.company_id=h.company_id`,
 receipts:`${selectProjection('*')} FROM talent_hire_consumption_receipts`,
} as const;
/** Owner-local company-scoped materialized CTE family. The CTE name, row alias
 * and lineage source are parameterized; the emitted SQL is byte-identical to
 * the former three inline copies over the same Recruitment owner rows. */
const scopedMaterialized=(name:string,alias:string,source:string)=>`${name} AS MATERIALIZED(${selectProjection(`${alias}.*`)} FROM (${source}) ${alias},scope WHERE ${alias}.company_id=scope.company_id)`;
export async function lockTalentPackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){
 const {database,companyId}=packageSettlementSource(owner,lock);await database.prepare("LOCK TABLE talent_requisitions,talent_candidates,talent_applications,talent_proposals,talent_offers,talent_hire_handoffs,talent_hire_consumption_receipts,talent_requisition_events,talent_candidate_events,talent_application_events,talent_offer_events,talent_candidate_privacy_events IN SHARE MODE").run();await database.batch(talentSettlementPlacementReadLocks(database));sources.set(database,companyId);
}
export function assertTalentPackageObligationSources(owner:PostgresDatabase,database:SqlDatabase,companyId:string){owner.assertActiveTransaction(database);if(sources.get(database)!==companyId)throw Error("TALENT_PACKAGE_OBLIGATION_SOURCE_REQUIRED");}
export async function talentPackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 assertTalentPackageObligationSources(owner,database,input.companyId);if(input.moduleId!=="talent"||input.packageId!=="recruit"||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)throw Error("TALENT_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
 const facts=(await database.prepare(`WITH scope AS(${selectProjection('?::text company_id')}),${scopedMaterialized("requisitions","r",requisitions)},${scopedMaterialized("applications","a",applications)},${scopedMaterialized("offers","o",offers)}
 ${selectProjection(`'requisition' kind,r.id,r.version,r.status state,NOT r.source_valid OR r.status NOT IN ('closed','cancelled','rejected') pending`)} FROM requisitions r
 UNION ALL ${selectProjection(`'candidate',c.id,c.version,c.status||CASE WHEN c.privacy_hold THEN ':held' ELSE '' END,c.status='active'`)} FROM talent_candidates c,scope WHERE c.company_id=scope.company_id
 UNION ALL ${selectProjection(`'application',a.id,a.version,a.status,NOT a.source_valid OR CASE a.status WHEN 'rejected' THEN false WHEN 'withdrawn' THEN false WHEN 'offered' THEN NOT ${existsRow("offers o","o.application_id=a.id AND o.source_valid AND o.status IN ('declined','withdrawn') AND o.terminal_evidence")} WHEN 'accepted' THEN NOT ${existsRow("offers o JOIN talent_hire_handoffs h ON h.offer_id=o.id AND h.company_id=o.company_id","o.application_id=a.id AND o.source_valid AND o.status='accepted' AND o.terminal_evidence")} ELSE true END`)} FROM applications a
 UNION ALL ${selectProjection(`'proposal',p.id,p.version,p.status,NOT a.source_valid OR p.application_version>a.version OR p.status='submitted'`)} FROM talent_proposals p JOIN applications a ON a.id=p.application_id
 UNION ALL ${selectProjection(`'offer',o.id,o.version,o.status,NOT o.source_valid OR o.status NOT IN ('withdrawn','declined','accepted') OR NOT o.terminal_evidence OR o.status='accepted' AND NOT ${existsRow("talent_hire_handoffs h","h.offer_id=o.id AND h.company_id=o.company_id")}`)} FROM offers o
 ORDER BY kind,id`).bind(input.companyId).all<{kind:string;id:string;version:number;state:string;pending:boolean}>()).results;
 return {companyId:input.companyId,packageId:input.packageId,moduleId:"talent",observedAt:input.now,revision:createHash("sha256").update(JSON.stringify({companyId:input.companyId,packageVersion:input.packageVersion,facts})).digest("hex"),outstanding:facts.filter(fact=>fact.pending).length};
}
