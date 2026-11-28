import "server-only";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import {holdingResources} from "../../../../../platform/runtime/server/src/public/runtime";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {commercePackageEnforcementMode, readPurchasedAccesses} from "@erpx/commercial-server/public/purchase-command-boundary";
import type {CommerceOrderKind, CommercePurchaseControls} from "@erpx/commerce-ui/lib/purchase-controls";
import {nativeCommerceReadAll,RD_PURCHASE_CONTROL_INVOICE,RD_PURCHASE_CONTROL_ORDER,RD_PURCHASE_CONTROL_RETURN_LINES} from './native-commerce-read';

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
    const sourceKind = target.type === "invoice" ? RD_PURCHASE_CONTROL_INVOICE : RD_PURCHASE_CONTROL_ORDER;
    const rows = await nativeCommerceReadAll<{kind: CommerceOrderKind; company_id: string; branch_id: string | null; department_id: string | null}>(sourceKind,[target.id],database);
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
  const rows = await nativeCommerceReadAll<{id:string;has_stock:boolean|number}>(RD_PURCHASE_CONTROL_RETURN_LINES,[orderId,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId],database);
  return Object.fromEntries(rows.map(row=>[row.id,{...purchase,stockSettleExisting:purchase.settleExisting && (!row.has_stock || purchase.stockSettleExisting)}]));
}
