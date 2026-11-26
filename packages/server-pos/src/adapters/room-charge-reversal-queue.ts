import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { nativeRoomChargeQueue, type RoomChargeReviewRow } from "./native_room_charges/client";

/**
 * Native POS room-charge reversal queue read. The former SQL authority (the
 * `pos_room_charge_reversal_requests` SELECT and all IAM/package derivations)
 * was deleted: the mounted Rust owner executes the read and derives every
 * response field. This module transports the queue and fails closed without
 * native configuration; no TypeScript SQL fallback remains.
 */
export async function roomChargeReversalQueue(
  actor: WorkContext,
  propertyId?: string,
): Promise<RoomChargeReviewRow[]> {
  return nativeRoomChargeQueue(actor, propertyId);
}

export async function roomChargeReversalWork(actor: WorkContext, stage: "pos" | "pms"): Promise<import("@erpx/feature-work_projection/lib/work-item").WorkItem[]> {
  if(actor.departmentId || !hasPermission(actor,`${stage}.approve`))return [];
  return (await roomChargeReversalQueue(actor)).filter(row=>row.stage===stage).map(row=>({id:row.id,type:stage,title:"Review Guest Account charge reversal",detail:row.reason,href:stage==="pms"?`/pms/guest-accounts?property=${row.property_id}&queue=approval`:"/pos?tab=history&queue=approval",status:row.state,updatedAt:row.updated_at,queue:row.canAct?"todo":"waiting",relationships:["approval"],canAct:row.canAct}));
}
