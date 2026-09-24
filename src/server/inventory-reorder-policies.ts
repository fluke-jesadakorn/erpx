import "server-only";
import { samePerson } from "@/lib/iam";
import { hasPermission } from "@/lib/permissions";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import { assertion, executeCommand, replayCommand } from "./commands";
import { sha256 } from "./crypto";
import { ApiError } from "./http";
import { holdingResources } from "./runtime";
import type { SqlStatement } from "./runtime/types";

const quantityInput = z.string().regex(/^\d{1,12}(?:\.\d{1,3})?$/, "Use a positive quantity with at most three decimal places.");
export const inventoryReorderPolicySchema = z.object({
  warehouseId: z.string().min(1).max(200),
  productId: z.string().min(1).max(200),
  reorderPoint: quantityInput,
  reason: z.string().trim().min(5).max(2000),
  expectedVersion: z.number().int().positive().optional(),
}).strict();
export const inventoryReorderPolicyActionSchema = z.object({
  action: z.enum(["approve", "reject", "withdraw", "retire"]),
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(5).max(2000),
}).strict();

export type InventoryReorderPolicyRow = {
  id: string;
  company_id: string;
  warehouse_id: string;
  warehouse_name: string;
  branch_id: string;
  product_id: string;
  sku: string;
  product_name: string;
  unit: string;
  current_reorder_point_milli: string | null;
  proposed_reorder_point_milli: string | null;
  requested_by: string;
  approved_by: string | null;
  approved_at: number | null;
  retired_at: number | null;
  last_decision: "approved" | "rejected" | "retired" | null;
  last_reviewed_by: string | null;
  last_review_reason: string | null;
  last_reviewed_at: number | null;
  version: number;
  created_at: number;
  updated_at: number;
};

function milliQuantity(value: string) {
  const [whole, fraction = ""] = value.split("."), quantity = BigInt(whole) * BigInt(1000) + BigInt((fraction + "000").slice(0, 3));
  if (quantity <= BigInt(0) || quantity > BigInt("9223372036854775807"))
    throw new ApiError(422, "REORDER_POINT_INVALID", "The reorder point must fit a positive inventory quantity.");
  return quantity.toString();
}

function requireCompanyInventoryScope(actor: WorkContext) {
  if (actor.departmentId) throw new ApiError(403, "INVENTORY_SCOPE_REQUIRED", "Use a Company or Branch inventory assignment.");
}

export async function listInventoryReorderPolicies(actor: WorkContext) {
  if (!hasPermission(actor, "inventory.read")) throw new ApiError(403, "INVENTORY_FORBIDDEN", "The current role cannot read inventory reorder policies.");
  requireCompanyInventoryScope(actor);
  const { write } = await holdingResources(actor.holdingId);
  return (await write.prepare(`SELECT r.id,r.company_id,r.warehouse_id,r.product_id,r.current_reorder_point_milli::text current_reorder_point_milli,
    r.proposed_reorder_point_milli::text proposed_reorder_point_milli,r.requested_by,r.approved_by,r.approved_at,r.retired_at,
    r.last_decision,r.last_reviewed_by,r.last_review_reason,r.last_reviewed_at,r.version,r.created_at,r.updated_at,
    w.name warehouse_name,w.branch_id,p.sku,p.name product_name,p.unit
    FROM inventory_reorder_policies r
    JOIN commerce_warehouses w ON w.id=r.warehouse_id AND w.company_id=r.company_id
    JOIN commerce_products p ON p.id=r.product_id AND p.company_id=r.company_id
    WHERE r.company_id=? AND (?::text IS NULL OR w.branch_id=?)
    ORDER BY w.name,p.sku,r.id LIMIT 500`)
    .bind(actor.companyId, actor.branchId, actor.branchId)
    .all<InventoryReorderPolicyRow>()).results;
}

export async function saveInventoryReorderPolicy(actor: WorkContext, input: z.infer<typeof inventoryReorderPolicySchema>, key: string) {
  if (!hasPermission(actor, "inventory.prepare")) throw new ApiError(403, "INVENTORY_PREPARE_FORBIDDEN", "The current role cannot propose inventory reorder points.");
  requireCompanyInventoryScope(actor);
  const { write } = await holdingResources(actor.holdingId), point = milliQuantity(input.reorderPoint);
  const warehouse = await write.prepare("SELECT id,branch_id,version FROM commerce_warehouses WHERE id=? AND company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?)")
    .bind(input.warehouseId, actor.companyId, actor.branchId, actor.branchId).first<{ id: string; branch_id: string; version: number }>();
  if (!warehouse) throw new ApiError(404, "WAREHOUSE_NOT_FOUND", "The active warehouse is unavailable in the selected scope.");
  const product = await write.prepare("SELECT id,version FROM commerce_products WHERE id=? AND company_id=? AND status='active'")
    .bind(input.productId, actor.companyId).first<{ id: string; version: number }>();
  if (!product) throw new ApiError(404, "PRODUCT_NOT_FOUND", "The active product is unavailable in the selected Company.");
  const id = `reorder-${(await sha256(`${actor.companyId}:${warehouse.id}:${product.id}`)).slice(0, 32)}`;
  const prior = await replayCommand(write, actor, key, "inventory.reorder.propose", id, input);
  if (prior) return prior;
  const current = await write.prepare("SELECT id,version,proposed_reorder_point_milli,current_reorder_point_milli,retired_at FROM inventory_reorder_policies WHERE id=? AND company_id=?")
    .bind(id, actor.companyId).first<Pick<InventoryReorderPolicyRow, "id" | "version" | "proposed_reorder_point_milli" | "current_reorder_point_milli" | "retired_at">>();
  if (current?.proposed_reorder_point_milli !== null && current?.proposed_reorder_point_milli !== undefined)
    throw new ApiError(409, "REORDER_PROPOSAL_PENDING", "A reorder point for this product and warehouse is already awaiting review.");
  if (current && input.expectedVersion !== Number(current.version))
    throw new ApiError(409, "REORDER_POLICY_CHANGED", "The reorder policy changed. Refresh and review it before proposing another value.");
  if (!current && input.expectedVersion !== undefined)
    throw new ApiError(409, "REORDER_POLICY_CHANGED", "This reorder policy does not exist. Refresh the stock view.");
  const version = current ? Number(current.version) + 1 : 1, now = Date.now();
  const snapshot = JSON.stringify({ warehouseId: warehouse.id, productId: product.id, proposedReorderPointMilli: point, currentReorderPointMilli: current?.current_reorder_point_milli ?? null, reason: input.reason });
  const steps: SqlStatement[] = [
    assertion(write, key, "warehouse-version", "EXISTS(SELECT 1 FROM commerce_warehouses WHERE id=? AND company_id=? AND version=? AND status='active' AND (?::text IS NULL OR branch_id=?))", [warehouse.id, actor.companyId, warehouse.version, actor.branchId, actor.branchId]),
    assertion(write, key, "product-version", "EXISTS(SELECT 1 FROM commerce_products WHERE id=? AND company_id=? AND version=? AND status='active')", [product.id, actor.companyId, product.version]),
    current
      ? assertion(write, key, "policy-version", "EXISTS(SELECT 1 FROM inventory_reorder_policies WHERE id=? AND company_id=? AND version=? AND proposed_reorder_point_milli IS NULL)", [id, actor.companyId, current.version])
      : assertion(write, key, "policy-unique", "NOT EXISTS(SELECT 1 FROM inventory_reorder_policies WHERE id=? OR (warehouse_id=? AND product_id=?))", [id, warehouse.id, product.id]),
    current
      ? write.prepare(`UPDATE inventory_reorder_policies SET proposed_reorder_point_milli=?,requested_by=?,version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=?`)
          .bind(point, actor.userId, now, id, actor.companyId, current.version)
      : write.prepare(`INSERT INTO inventory_reorder_policies(id,company_id,warehouse_id,product_id,proposed_reorder_point_milli,requested_by,version,created_at,updated_at)
          VALUES(?,?,?,?,?,?,1,?,?)`).bind(id, actor.companyId, warehouse.id, product.id, point, actor.userId, now, now),
    write.prepare(`INSERT INTO inventory_reorder_policy_events(id,policy_id,policy_version,action,actor_id,reason,snapshot_json,created_at)
      VALUES(?,?,?, ?,?,?,?::jsonb,?)`).bind(`${id}:${version}`, id, version, current ? "revise" : "propose", actor.userId, input.reason, snapshot, now),
  ];
  return executeCommand(write, actor, key, "inventory.reorder.propose", input, id, version, { id, status: "pending", version }, steps, { companyId: actor.companyId, branchId: warehouse.branch_id, departmentId: null });
}

export async function transitionInventoryReorderPolicy(actor: WorkContext, id: string, input: z.infer<typeof inventoryReorderPolicyActionSchema>, key: string) {
  requireCompanyInventoryScope(actor);
  const { write } = await holdingResources(actor.holdingId), operation = `inventory.reorder.${input.action}`;
  const prior = await replayCommand(write, actor, key, operation, id, input);
  if (prior) return prior;
  const row = await write.prepare(`SELECT r.*,w.branch_id FROM inventory_reorder_policies r JOIN commerce_warehouses w ON w.id=r.warehouse_id
    WHERE r.id=? AND r.company_id=? AND w.company_id=? AND (?::text IS NULL OR w.branch_id=?)`)
    .bind(id, actor.companyId, actor.companyId, actor.branchId, actor.branchId).first<InventoryReorderPolicyRow>();
  if (!row) throw new ApiError(404, "REORDER_POLICY_NOT_FOUND", "The inventory reorder policy is unavailable in the selected scope.");
  if (Number(row.version) !== input.expectedVersion) throw new ApiError(409, "REORDER_POLICY_CHANGED", "The reorder policy changed. Refresh and review it before continuing.");
  const review = input.action === "approve" || input.action === "reject" || input.action === "retire";
  if (review && (!hasPermission(actor, "inventory.approve") || actor.branchId || actor.departmentId))
    throw new ApiError(403, "INVENTORY_APPROVAL_FORBIDDEN", "An independent Company inventory approver must review this policy.");
  if (input.action === "withdraw" && !hasPermission(actor, "inventory.prepare"))
    throw new ApiError(403, "INVENTORY_PREPARE_FORBIDDEN", "The current role cannot withdraw a reorder proposal.");
  if (input.action === "approve" || input.action === "reject") {
    if (!row.proposed_reorder_point_milli) throw new ApiError(409, "REORDER_PROPOSAL_MISSING", "There is no reorder proposal awaiting review.");
    if (samePerson(actor, row.requested_by)) throw new ApiError(403, "INDEPENDENT_APPROVER_REQUIRED", "A different Company approver must review the proposed reorder point.");
  } else if (input.action === "withdraw") {
    if (!row.proposed_reorder_point_milli || !samePerson(actor, row.requested_by)) throw new ApiError(403, "REORDER_WITHDRAW_FORBIDDEN", "Only the person who proposed a pending reorder point can withdraw it.");
  } else {
    if (row.proposed_reorder_point_milli || !row.current_reorder_point_milli || row.retired_at) throw new ApiError(409, "REORDER_POLICY_NOT_ACTIVE", "Only an active approved reorder point without a pending proposal can be retired.");
    if (row.approved_by && samePerson(actor, row.approved_by)) throw new ApiError(403, "INDEPENDENT_APPROVER_REQUIRED", "A different Company approver must retire this reorder point.");
  }
  const now = Date.now(), version = input.expectedVersion + 1;
  const update = input.action === "approve"
    ? write.prepare(`UPDATE inventory_reorder_policies SET current_reorder_point_milli=proposed_reorder_point_milli,proposed_reorder_point_milli=NULL,approved_by=?,approved_at=?,retired_at=NULL,last_decision='approved',last_reviewed_by=?,last_review_reason=?,last_reviewed_at=?,version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=?`).bind(actor.userId, now, actor.userId, input.reason, now, now, id, actor.companyId, input.expectedVersion)
    : input.action === "reject"
      ? write.prepare(`UPDATE inventory_reorder_policies SET proposed_reorder_point_milli=NULL,last_decision='rejected',last_reviewed_by=?,last_review_reason=?,last_reviewed_at=?,version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=?`).bind(actor.userId, input.reason, now, now, id, actor.companyId, input.expectedVersion)
      : input.action === "withdraw"
        ? write.prepare(`UPDATE inventory_reorder_policies SET proposed_reorder_point_milli=NULL,last_decision=NULL,last_reviewed_by=NULL,last_review_reason=NULL,last_reviewed_at=NULL,version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=?`).bind(now, id, actor.companyId, input.expectedVersion)
        : write.prepare(`UPDATE inventory_reorder_policies SET retired_at=?,last_decision='retired',last_reviewed_by=?,last_review_reason=?,last_reviewed_at=?,version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=?`).bind(now, actor.userId, input.reason, now, now, id, actor.companyId, input.expectedVersion);
  const snapshot = JSON.stringify({ action: input.action, currentReorderPointMilli: row.current_reorder_point_milli, proposedReorderPointMilli: row.proposed_reorder_point_milli, reason: input.reason });
  const steps: SqlStatement[] = [
    assertion(write, key, "policy-version", "EXISTS(SELECT 1 FROM inventory_reorder_policies WHERE id=? AND company_id=? AND version=? AND (proposed_reorder_point_milli IS NOT DISTINCT FROM ?))", [id, actor.companyId, input.expectedVersion, row.proposed_reorder_point_milli]),
    update,
    write.prepare(`INSERT INTO inventory_reorder_policy_events(id,policy_id,policy_version,action,actor_id,reason,snapshot_json,created_at)
      VALUES(?,?,?,?,?,?,?::jsonb,?)`).bind(`${id}:${version}`, id, version, input.action, actor.userId, input.reason, snapshot, now),
  ];
  return executeCommand(write, actor, key, operation, input, id, version, { id, status: input.action === "reject" ? (row.current_reorder_point_milli ? "approved" : "unconfigured") : input.action === "retire" ? "retired" : input.action === "withdraw" ? (row.current_reorder_point_milli ? "approved" : "unconfigured") : "approved", version }, steps, { companyId: actor.companyId, branchId: row.branch_id, departmentId: null });
}
