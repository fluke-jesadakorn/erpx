import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {recordImageSql} from './record-image-sources';
import {iamScopeGuardSql} from './iam-scope-guard';
const scope="r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?)";
/** The Work/Plan purchase-source eligibility witness now shares the common IAM
 * guard tail with the Work owner-eligibility/can-review images; this owner only
 * supplies its fixed raw relations, aliases, row/user expression, permission and
 * scope. The composed string stays byte-identical to its pre-dedupe literal. */
const eligible=iamScopeGuardSql(
  'assignments assignment JOIN hr_people person ON person.user_id=assignment.user_id AND person.company_id=assignment.company_id JOIN hr_engagements engagement ON engagement.person_id=person.id AND engagement.company_id=person.company_id',
  'assignment','person','engagement','r','r.owner_id','records.write',
  `(assignment.scope_kind='holding' OR assignment.scope_kind='branch' AND assignment.scope_node_id=r.branch_id OR assignment.scope_kind='department' AND assignment.scope_node_id=r.department_id)`,
);
/** Closed Work/Plan owner image, including its actual physical Asset and the
 * existing worker-reference eligibility; no People purchase is inferred. */
export function maintenancePurchaseSourceQuery(kind:"work"|"plan",file=false){return recordImageSql(
 `'record',${file?"to_jsonb(r)-'version'-'updated_at'":"to_jsonb(r)"},'asset',to_jsonb(a),'ownerEligible',${eligible}`,
 ` FROM ${kind==="work"?"maintenance_work_orders":"maintenance_preventive_plans"} r JOIN operational_assets a ON a.id=r.asset_id AND a.company_id=r.company_id AND a.branch_id=r.branch_id AND a.department_id IS NOT DISTINCT FROM r.department_id WHERE r.id=? AND ${scope}`,
);}
export function maintenancePurchaseSourceReadLocks(database:SqlDatabase){return [database.prepare("LOCK TABLE maintenance_work_orders,maintenance_preventive_plans,maintenance_preventive_occurrences IN SHARE ROW EXCLUSIVE MODE")];}
