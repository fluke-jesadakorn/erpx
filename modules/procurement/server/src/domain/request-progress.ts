export type ProgressRow = Record<string, unknown>;

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

export type PurchaseProgressRequest={id:string;kind:string;state:string;blocked_reason:string|null;approval_json:string;items_json:string;purchase_order_id?:string|null};
export type PurchaseProgressFacts={rfqs:ProgressRow[];orders:ProgressRow[];quantities:ProgressRow|null;receipts:ProgressRow|null;bills:ProgressRow|null;payments:ProgressRow|null;allocations:ProgressRow[]};
export function projectPurchaseProgress(request:PurchaseProgressRequest,facts:PurchaseProgressFacts,canPurchase:boolean,canFinance:boolean):PurchaseRequestProgress{
 const {rfqs,orders,quantities,receipts,bills,payments,allocations}=facts,rfq=rfqs[0]??null,order=orders[0]??null;
 const approval=JSON.parse(request.approval_json) as {decision?:string},approved=approval.decision==="approved";
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
