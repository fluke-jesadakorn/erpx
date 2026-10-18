import "server-only";
import type {WorkContext} from "../../identity_access/public/authorization";
import {holdingResources} from "../../runtime/public/runtime";
import {ApiError} from "../../runtime/public/http";
import {commercePackageEnforcementMode, readPurchasedAccesses} from "../../commercial/public/purchase-command-boundary";
import type {CommerceOrderKind, CommercePurchaseControls} from "@/lib/commerce/purchase-controls";

type Target = {type: "kind"; kind: CommerceOrderKind} | {type: "order" | "invoice"; id: string};
/** Presentation only: existing typed owner readers and permissions authorize
 * each page. Record targets resolve persisted ownership instead of trusting an
 * invoice label or product host. No grant facts leave the server. */
export async function commercePurchaseControls(actor: WorkContext, target: Target): Promise<CommercePurchaseControls | undefined> {
  const mode = await commercePackageEnforcementMode();
  if (mode === "disabled") return undefined;
  const database = (await holdingResources(actor.holdingId)).write;
  let kind: CommerceOrderKind;
  if (target.type === "kind") kind = target.kind;
  else {
    const rows = (await database.prepare(`SELECT DISTINCT o.id,o.kind,o.company_id,o.branch_id,o.department_id FROM commerce_orders o
      ${target.type === "invoice" ? "JOIN commerce_fulfillments f ON f.order_id=o.id" : ""}
      WHERE ${target.type === "invoice" ? "f.invoice_id" : "o.id"}=?`).bind(target.id).all<{kind: CommerceOrderKind; company_id: string; branch_id: string | null; department_id: string | null}>()).results;
    if (!rows.length && target.type === "invoice") return undefined;
    if (rows.length !== 1 || rows[0].company_id !== actor.companyId || actor.branchId && rows[0].branch_id !== actor.branchId || actor.departmentId && rows[0].department_id !== actor.departmentId)
      throw new ApiError(404, "COMMERCE_SOURCE_UNAVAILABLE", "This Commerce source is unavailable in the current workspace.");
    kind = rows[0].kind;
  }
  if (!["quote", "sales", "purchase"].includes(kind)) throw new ApiError(404, "COMMERCE_SOURCE_UNAVAILABLE", "This Commerce source kind is unavailable.");
  const base = kind === "quote" ? "sales.quotes" : kind === "sales" ? "sales.orders" : "supply.purchasing";
  const stock = kind === "purchase" ? "supply.stock" : "inventory.stock-core", finance = kind === "purchase" ? "finance.inventory-valuation" : "finance.sales-posting";
  const access = await readPurchasedAccesses(database, mode, actor.companyId, [base, stock, finance, "supply.receiving", "sales.orders"]), owner = access[base];
  return {newWork: owner.newWork, settleExisting: owner.settleExisting, history: owner.history, revision: owner.revision,
    convertNewWork: owner.newWork && access["sales.orders"].newWork,
    stockNewWork: owner.newWork && access[stock].newWork, stockSettleExisting: owner.settleExisting && access[stock].settleExisting,
    financeSettleExisting: owner.settleExisting && access[finance].settleExisting,
    receivingSettleExisting: owner.settleExisting && (kind !== "purchase" || access["supply.receiving"].settleExisting)};
}

/** Return approval uses its actual retained lines, including service-only
 * customer credits. One scoped read serves all returns on this order page. */
export async function commerceReturnPurchaseControls(actor: WorkContext, orderId: string, purchase: CommercePurchaseControls | undefined): Promise<Record<string, CommercePurchaseControls>> {
  if (!purchase) return {};
  const database = (await holdingResources(actor.holdingId)).write;
  const rows = (await database.prepare(`SELECT r.id,EXISTS(SELECT 1 FROM commerce_return_lines rl
    JOIN commerce_fulfillment_lines fl ON fl.id=rl.fulfillment_line_id JOIN commerce_order_lines ol ON ol.id=fl.order_line_id
    WHERE rl.return_id=r.id AND ol.item_type!='service') has_stock FROM commerce_returns r
    JOIN commerce_orders o ON o.id=r.order_id WHERE o.id=? AND o.company_id=?
    AND (? IS NULL OR o.branch_id=?) AND (? IS NULL OR o.department_id=?)`)
    .bind(orderId,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).all<{id:string;has_stock:boolean|number}>()).results;
  return Object.fromEntries(rows.map(row=>[row.id,{...purchase,stockSettleExisting:purchase.settleExisting && (!row.has_stock || purchase.stockSettleExisting)}]));
}
