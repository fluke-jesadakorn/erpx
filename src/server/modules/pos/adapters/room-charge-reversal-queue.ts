import { hospitalityOwnerProjectionSources as pmsSources } from "../../pms/public/hospitality-owner-projection-sources";
import "server-only";
import { holdingResources } from "../../runtime/public/runtime";
import type { WorkContext } from "../../identity_access/public/authorization";
import { requirePermission } from "../../identity_access/public/iam-permissions";
import { hasPermission } from "@/lib/identity_access/permissions";
import { samePerson } from "@/lib/identity_access/iam";
import { ApiError } from "../../runtime/public/http";
import { readPurchasedAccesses, posPackageEnforcementMode } from "../../commercial/public/purchase-command-boundary";

export async function roomChargeReversalQueue(actor: WorkContext, propertyId?: string) {
  const pos = hasPermission(actor,"pos.approve"), pms = hasPermission(actor,"pms.approve");
  if (!pos && !pms) requirePermission(actor,"pos.approve");
  if (actor.departmentId) throw new ApiError(403,"POS_BRANCH_REQUIRED","Select a Company or Branch.");
  const db = (await holdingResources(actor.holdingId)).write;
  const mode = await posPackageEnforcementMode(), access = mode === "disabled" ? undefined : await readPurchasedAccesses(db, mode, actor.companyId, ["pos.hotel-charge", "pms.guest-accounts", "finance.hotel-posting"]);
  if (access && !Object.values(access).every(value => value.history)) return [];
  const purchasedSettlement = !access || Object.values(access).every(value => value.settleExisting);
  const rows = (await db.prepare(`SELECT q.*,s.number,s.created_by,(s.version=q.sale_version AND s.total_minor=q.amount_minor AND p.version=q.property_version AND p.state='open') source_current,
    (SELECT actor_id FROM pos_audit WHERE operation='sale.charge_to_room' AND record_id=s.id ORDER BY created_at DESC LIMIT 1) charged_by
    FROM pos_room_charge_reversal_requests q JOIN pos_sales s ON s.id=q.sale_id AND s.company_id=q.company_id JOIN (${pmsSources.pms_properties}) p ON p.id=q.property_id AND p.company_id=q.company_id
    WHERE q.company_id=? AND (?::text IS NULL OR q.branch_id=?) AND (?::text IS NULL OR q.property_id=?)
    AND q.state IN ('requested','pos_approved') ORDER BY q.created_at,q.id LIMIT 100`)
    .bind(actor.companyId,actor.branchId,actor.branchId,propertyId??null,propertyId??null)
    .all<{id:string;company_id:string;branch_id:string;sale_id:string;property_id:string;sale_version:number;property_version:number;restock_line_ids_json:string;number:string;state:"requested"|"pos_approved";version:number;reason:string;amount_minor:number;requested_by:string;pos_approved_by:string|null;created_by:string;charged_by:string|null;updated_at:number;source_current:boolean}>()).results;
  return rows.map(row => {
    const independent = ![row.requested_by,row.pos_approved_by,row.created_by,row.charged_by].some(user => user && samePerson(actor,user));
    const stage: "pos" | "pms" = row.state === "requested" ? "pos" : "pms";
    const allowed = purchasedSettlement && (stage === "pos" ? pos : pms);
    return { ...row, stage, canAct: allowed && independent,
      canApprove: allowed && independent && row.source_current,
      reasonDisabled: !allowed ? `The selected assignment requires ${stage}.approve.` : !independent ? "A different person must review this request." : !row.source_current ? "The source changed. Reject this request and request again." : null };
  });
}

export async function roomChargeReversalWork(actor: WorkContext, stage: "pos" | "pms"): Promise<import("@/lib/work_projection/work-item").WorkItem[]> {
  if(actor.departmentId || !hasPermission(actor,`${stage}.approve`))return [];
  return (await roomChargeReversalQueue(actor)).filter(row=>row.stage===stage).map(row=>({id:row.id,type:stage,title:"Review Guest Account charge reversal",detail:row.reason,href:stage==="pms"?`/pms/guest-accounts?property=${row.property_id}&queue=approval`:"/pos?tab=history&queue=approval",status:row.state,updatedAt:row.updated_at,queue:row.canAct?"todo":"waiting",relationships:["approval"],canAct:row.canAct}));
}
