import "server-only";

import { hasPermission } from "@/lib/permissions";
import type { WorkContext } from "./authorization";
import { getEmployeeRequest } from "./employee-requests";
import { ApiError } from "./http";
import type { SqlDatabase } from "./runtime/types";

type Row = Record<string, unknown>;

export type PurchaseProgressState = "complete" | "current" | "waiting" | "blocked";

export type PurchaseProgressStep = {
  id: "approval" | "sourcing" | "order" | "receiving" | "invoice" | "payment";
  label: string;
  state: PurchaseProgressState;
  detail: string;
  href: string | null;
};

export type PurchaseRequestProgress = {
  steps: PurchaseProgressStep[];
  completed: number;
  total: number;
};

function stateForRequest(state: string, approved: boolean): PurchaseProgressState {
  if (["rejected", "cancelled"].includes(state)) return "blocked";
  if (approved) return "complete";
  return ["draft", "returned"].includes(state) ? "waiting" : "current";
}

/**
 * A read-only projection over the owner records in the purchase-to-payment chain.
 * The request service remains the access boundary. Links are only returned when
 * the current assignment can open the target owner workspace.
 */
export async function purchaseRequestProgress(
  db: SqlDatabase,
  actor: WorkContext,
  requestId: string,
): Promise<PurchaseRequestProgress> {
  const request = await getEmployeeRequest(db, actor, requestId);
  if (request.kind !== "purchase")
    throw new ApiError(
      422,
      "PURCHASE_REQUEST_REQUIRED",
      "Purchase progress is available only for purchase requests.",
    );

  const approval = JSON.parse(request.approval_json) as { decision?: string };
  const approved = approval.decision === "approved";
  const canPurchase = hasPermission(actor, "commerce.read");
  const canFinance = hasPermission(actor, "finance.read");

  const rfqs = (await db.prepare("SELECT id,status,order_id FROM procurement_rfqs WHERE request_id=? AND company_id=? AND status<>'cancelled' ORDER BY updated_at DESC,id DESC").bind(request.id,actor.companyId).all<Row>()).results;
  const rfq=rfqs[0]??null;
  const orders=(await db.prepare("SELECT DISTINCT o.id,o.number,o.state,o.created_at FROM commerce_orders o WHERE o.company_id=? AND o.kind='purchase' AND (o.id=? OR EXISTS(SELECT 1 FROM procurement_request_orders l WHERE l.request_id=? AND l.order_id=o.id)) ORDER BY o.created_at DESC,o.id DESC").bind(actor.companyId,request.purchase_order_id,request.id).all<Row>()).results;
  const order=orders[0]??null;
  const quantities=await db.prepare("SELECT COALESCE(SUM(l.quantity_milli),0) ordered_milli,COALESCE(SUM(l.fulfilled_milli),0) received_milli FROM commerce_order_lines l JOIN commerce_orders o ON o.id=l.order_id WHERE o.company_id=? AND (o.id=? OR EXISTS(SELECT 1 FROM procurement_request_orders link WHERE link.request_id=? AND link.order_id=o.id))").bind(actor.companyId,request.purchase_order_id,request.id).first<Row>();
  const receipts=await db.prepare("SELECT COUNT(*) receipt_count FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE o.company_id=? AND (o.id=? OR EXISTS(SELECT 1 FROM procurement_request_orders link WHERE link.request_id=? AND link.order_id=o.id))").bind(actor.companyId,request.purchase_order_id,request.id).first<Row>();
  const bills=await db
        .prepare(
          `SELECT COUNT(*) bill_count,
             COUNT(*) FILTER (WHERE m.status='posted') posted_count,
             COUNT(m.invoice_id) invoice_count,
             COUNT(*) FILTER (WHERE i.status='paid' OR COALESCE(i.outstanding_minor,0)=0) paid_count,
             (SELECT COUNT(DISTINCT br.fulfillment_id)
                FROM procurement_bill_match_receipts br
                JOIN procurement_bill_matches covered ON covered.id=br.match_id
               JOIN commerce_orders covered_order ON covered_order.id=covered.order_id
               WHERE covered.company_id=? AND (covered_order.id=? OR EXISTS(SELECT 1 FROM procurement_request_orders link WHERE link.request_id=? AND link.order_id=covered_order.id)) AND covered.status<>'returned' AND br.active=TRUE) matched_receipt_count
           FROM procurement_bill_matches m
           LEFT JOIN finance_invoices i ON i.id=m.invoice_id
           JOIN commerce_orders matched_order ON matched_order.id=m.order_id
           WHERE m.company_id=? AND (matched_order.id=? OR EXISTS(SELECT 1 FROM procurement_request_orders link WHERE link.request_id=? AND link.order_id=matched_order.id)) AND m.status<>'returned'`,
        )
        .bind(actor.companyId,request.purchase_order_id,request.id,actor.companyId,request.purchase_order_id,request.id)
        .first<Row>();
  const payments = await db
        .prepare(
          `SELECT COUNT(*) instruction_count,
             COUNT(*) FILTER (WHERE s.status='confirmed') confirmed_count,
             MIN(s.batch_id) batch_id
           FROM finance_supplier_instructions s
           JOIN finance_invoices i ON i.id=s.invoice_id
           JOIN procurement_bill_matches m ON m.invoice_id=i.id
           JOIN commerce_orders paid_order ON paid_order.id=m.order_id
           WHERE m.company_id=? AND (paid_order.id=? OR EXISTS(SELECT 1 FROM procurement_request_orders link WHERE link.request_id=? AND link.order_id=paid_order.id)) AND s.status<>'cancelled'`,
        )
        .bind(actor.companyId,request.purchase_order_id,request.id)
        .first<Row>();
  const allocations=(await db.prepare("SELECT l.request_item_ordinal ordinal,COALESCE(sum(l.request_quantity_milli),0) allocated_milli FROM procurement_rfq_lines l JOIN procurement_rfqs r ON r.id=l.rfq_id WHERE r.request_id=? AND r.status<>'cancelled' GROUP BY l.request_item_ordinal").bind(request.id).all<Row>()).results;
  const allocationByItem=new Map(allocations.map(row=>[Number(row.ordinal),Number(row.allocated_milli)]));
  const requestItems=JSON.parse(request.items_json) as {quantity:string}[];
  const fullyAllocated=requestItems.length>0&&requestItems.every((item,index)=>Math.round(Number(item.quantity)*1000)===(allocationByItem.get(index)??0));

  const ordered = Number(quantities?.ordered_milli ?? 0);
  const received = Number(quantities?.received_milli ?? 0);
  const receiptCount = Number(receipts?.receipt_count ?? 0);
  const billCount = Number(bills?.bill_count ?? 0);
  const postedCount = Number(bills?.posted_count ?? 0);
  const invoiceCount = Number(bills?.invoice_count ?? 0);
  const paidCount = Number(bills?.paid_count ?? 0);
  const matchedReceiptCount = Number(bills?.matched_receipt_count ?? 0);
  const instructionCount = Number(payments?.instruction_count ?? 0);
  const confirmedCount = Number(payments?.confirmed_count ?? 0);
  const orderPlaced = orders.length>0 && orders.every(current=>["confirmed", "partially_fulfilled", "fulfilled", "invoiced"].includes(String(current.state)));
  const receivingComplete = ordered > 0 && received === ordered;
  const invoiceComplete = receiptCount > 0 && matchedReceiptCount === receiptCount && billCount > 0 && postedCount === billCount;
  const paymentComplete = invoiceCount > 0 && paidCount === invoiceCount;

  const steps: PurchaseProgressStep[] = [
    {
      id: "approval",
      label: "Request approval",
      state: stateForRequest(request.state, approved),
      detail: approved
        ? "Company spending approval is complete."
        : request.blocked_reason || "Waiting for the current approval step.",
      href: null,
    },
    {
      id: "sourcing",
      label: "Supplier quotations",
      state: fullyAllocated && rfqs.length>0 && rfqs.every(current=>current.status==="ordered")
        ? "complete"
        : rfq
          ? "current"
          : approved
            ? "current"
            : "waiting",
      detail: rfq
        ? `${rfqs.length} quotation request${rfqs.length===1?"":"s"}; approved demand is ${fullyAllocated?"fully allocated":"partially allocated"}. Latest is ${String(rfq.status).replaceAll("_", " ")}.`
        : approved
          ? "Purchasing can map the approved items and collect supplier offers."
          : "Starts after spending approval.",
      href: rfq && canPurchase ? `/procurement-rfqs/${String(rfq.id)}` : null,
    },
    {
      id: "order",
      label: "Purchase order",
      state: orderPlaced ? "complete" : order ? "current" : "waiting",
      detail: order
        ? `${orders.length} linked purchase order${orders.length===1?"":"s"}; latest ${String(order.number)} is ${String(order.state).replaceAll("_", " ")}.`
        : "No purchase order has been linked yet.",
      href: order && canPurchase ? `/commerce/orders/${String(order.id)}` : null,
    },
    {
      id: "receiving",
      label: "Goods receipt",
      state: receivingComplete
        ? "complete"
        : orderPlaced || received > 0
          ? "current"
          : "waiting",
      detail: order
        ? `${received / 1000} of ${ordered / 1000} units received across ${receiptCount} receipt${receiptCount === 1 ? "" : "s"}.`
        : "Starts after the purchase order is approved.",
      href: order && canPurchase ? `/commerce/orders/${String(order.id)}` : null,
    },
    {
      id: "invoice",
      label: "Supplier invoice and posting",
      state: invoiceComplete
        ? "complete"
        : billCount > 0
          ? "current"
          : receiptCount > 0
            ? "current"
            : "waiting",
      detail: billCount
        ? `${matchedReceiptCount} of ${receiptCount} receipts covered by ${postedCount} of ${billCount} posted supplier invoice${billCount === 1 ? "" : "s"}.`
        : receiptCount > 0
          ? "Finance can match the supplier invoice to the receipt."
          : "Starts after goods are received.",
      href: canFinance ? "/supplier-bills" : null,
    },
    {
      id: "payment",
      label: "Supplier payment",
      state: paymentComplete
        ? "complete"
        : instructionCount > 0 || invoiceComplete
          ? "current"
          : "waiting",
      detail: paymentComplete
        ? "All linked payable invoices are settled."
        : instructionCount > 0
          ? `${confirmedCount} of ${instructionCount} payment instruction${instructionCount === 1 ? "" : "s"} confirmed against bank evidence.`
          : invoiceComplete
            ? "Finance can prepare an independently approved payment batch."
            : "Starts after the supplier invoice is posted.",
      href: canFinance
        ? payments?.batch_id
          ? `/finance/supplier-batches/${String(payments.batch_id)}`
          : "/finance/supplier-batches"
        : null,
    },
  ];

  return {
    steps,
    completed: steps.filter((step) => step.state === "complete").length,
    total: steps.length,
  };
}
