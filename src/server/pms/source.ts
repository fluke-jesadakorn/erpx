import "server-only";
import { hasPermission } from "@/lib/permissions";
import type { WorkContext } from "../authorization";
import type { StaffActor } from "../staff-policy";
import type { SqlDatabase } from "../runtime/types";
import { ApiError } from "../http";
import { pmsSourcesSql, pmsSourceReadableSql } from "./source-policy";
import { sourceRecordLabel } from "@/lib/source-record";

export type PmsSource = {
  id: string; property_id: string; company_id: string; branch_id: string;
  property_name: string; business_date: string; kind: string;
  data: { state?: string; number?: string; type?: string; roomId?: string; folioId?: string;
    requestedBy?: string; recordedBy?: string; userId?: string; completedBy?:string;assigneeId?:string;date?:string };
  version: number; updated_at: number;
};
export function decodePmsSource<T extends PmsSource>(source:T):T {
  return {...source,data:typeof source.data === "string" ? JSON.parse(source.data) : source.data};
}

export async function readablePmsSource(db: SqlDatabase, actor: StaffActor, id: string) {
  const source = await db.prepare(`SELECT pr.* FROM (SELECT ?::text user_id) recipient CROSS JOIN (${pmsSourcesSql}) pr WHERE pr.id=? AND pr.company_id=?
    AND (?::text IS NULL OR pr.branch_id=?) AND ?::text IS NULL
    AND ${pmsSourceReadableSql("pr", "recipient.user_id", actor.assignmentId ? "?::text" : undefined)}`)
    .bind(actor.userId, id, actor.companyId, actor.branchId, actor.branchId, actor.departmentId,...(actor.assignmentId?[actor.assignmentId]:[])).first<PmsSource>();
  const decoded=source?decodePmsSource(source):null;
  if (!decoded || !hasPermission(actor, "pms.read") || !pmsSourceAllowed(actor, decoded))
    throw new ApiError(404, "PMS_SOURCE_NOT_FOUND", "This hotel work is unavailable.");
  return decoded;
}

function pmsSourceAllowed(actor: StaffActor, source: PmsSource) {
  const can = (permission: string) => hasPermission(actor, permission);
  switch (source.kind) {
    case "room": case "task": case "block": return can("pms.housekeeping") || can("pms.inspect") || can("pms.maintenance");
    case "stay": case "reservation": return can("pms.frontdesk") || can("pms.reservations") || can("pms.cashier");
    case "folio": return can("pms.cashier") || can("pms.reports");
    case "refund": case "correction": return can("pms.approve") || (can("pms.cashier") && source.data.requestedBy === actor.userId);
    case "transfer": return can("finance.approve") || (can("pms.cashier") && source.data.recordedBy === actor.userId);
    case "shift": return can("pms.approve") || can("pms.audit") || (can("pms.cashier") && source.data.userId === actor.userId);
    case "night": return can("pms.audit") || can("pms.reports");
    default: return false;
  }
}

export function pmsSourceLink(source: PmsSource) {
  const section = source.kind === "stay" ? "front-desk" : source.kind === "reservation" ? "reservations"
    : ["room", "task", "block"].includes(source.kind) ? "housekeeping" : source.kind === "shift" ? "cashier"
    : source.kind === "night" ? "night-audit" : "folios";
  const query = new URLSearchParams({ property: source.property_id, record: source.id });
  if (source.data.folioId) query.set("folio", source.data.folioId);
  if (source.kind === "folio") query.set("folio", source.id);
  if (source.kind === "task") query.set("task", source.id);
  if (["transfer","refund"].includes(source.kind)) query.set("queue","approval");
  if (source.kind === "correction") query.set("queue","corrections");
  return { section, sourceKind: source.kind, version: source.version, label: sourceRecordLabel(source.kind),
    href: `/pms/${section}?${query}`, title: `${source.property_name} · ${sourceRecordLabel(source.kind)}${source.data.number ? " " + source.data.number : ""}` };
}

export async function pmsSourceRecipients(db: SqlDatabase, actor: WorkContext) {
  const now = Date.now();
  return (await db.prepare(`SELECT DISTINCT a.user_id id,COALESCE(h.legal_name,a.user_id) name
    FROM assignments a LEFT JOIN hr_people h ON h.user_id=a.user_id AND h.company_id=a.company_id
    AND h.status='active' AND h.merged_into_id IS NULL
    WHERE a.company_id=? AND a.user_id<>? AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>?
    AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?)
    AND a.scope_kind IN ('holding','branch') AND (?::text IS NULL OR a.scope_kind='holding' OR a.scope_node_id=?)
    AND iam_role_allows(a.role,a.company_id,'pms.read') ORDER BY name,a.user_id LIMIT 100`)
    .bind(actor.companyId,actor.userId,now,now,now,now,actor.branchId,actor.branchId).all<{id:string;name:string}>()).results;
}
