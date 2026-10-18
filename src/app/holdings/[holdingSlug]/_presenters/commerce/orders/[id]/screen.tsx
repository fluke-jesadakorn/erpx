import {customerNotificationReconciliationControls} from "@/server/modules/sales/public/sales-customer-notifications";
import {landedCostPurchaseControls} from "@/server/modules/inventory/public/landed-cost-purchases";
import { commercePurchaseControls, commerceReturnPurchaseControls } from "@/server/modules/commerce/public/purchase-controls";
import { CommerceEnvironment, CommercePurchaseProvider } from "@/components/commerce/purchase-context";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { commercePresentationQueries } from "@/server/modules/commerce/public/presentation-queries";


import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { Table, TableFrame } from "@/components/ui/foundation";
import { TimeText } from "@/components/people_payroll/time/time-preference-context";
import { BusinessDocumentBranding } from "@/components/brand/business-document";
import {
  FeaturePage,
  FeatureTabs,
  FeatureWidget,
  WidgetGrid,
} from "@/components/shared/features/feature-page";
import { CustomerDeliveryNotification, StockReservationForm, FulfillmentForm, ServiceCompletionForm, InvoiceDeliveryForm, OrderActions, OrderEditor, ReturnActions, ReturnRequestForm } from "@/components/records/loaders/modules--commerce-controls";
import { StatusBadge } from "@/components/ui/primitives";
import { FlowPanel, documentFlow } from "@/components/workplace/flow-panel";
import { RecordNextAction } from "@/components/workplace/record-next-action";
import { WorkReturn } from "@/components/records/loaders/workplace--work-return";
import { minorToDecimal } from "@/lib/finance/finance-money";
import { hasPermission } from "@/lib/identity_access/permissions";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { commerceOverview, commercePermissions, getOrder } from "@/server/modules/commerce/public/commerce";
import {salesEditorOptions,type SalesEditorOptions} from "@/server/modules/commerce/public/sales-editor";


import { RecordLink as Link } from "@/components/records/record-link";
import { supplierPromiseDetail } from "@/server/modules/procurement/public/supplier-promises";
import { SupplierBillCapture } from "@/components/records/loaders/modules--supplier-bill-console";
import { supplierBillWorkspace, supplierInvoicePolicy } from "@/server/modules/procurement/public/supplier-bill-matching";
import { SupplierPromiseConsole } from "@/components/records/loaders/modules--supplier-promise-console";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { landedCostsForOrder, landedCostOptions } from "@/server/modules/inventory/public/inventory-landed-costs";
import { LandedCostCreateForm } from "@/components/records/loaders/modules--landed-cost-controls";
import { RecordDocumentPanel } from "@/components/records/loaders/attachments--record-document-panel";
import { Card } from "@/components/ui/foundation";
export const dynamic = "force-dynamic";
export default async function OrderPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
  searchParams: Promise<{ print?: string; brand?: string }>;
}) {
  const { holdingSlug, id } = await params;
  const query = await searchParams;
  const printing = query.print === "1";
  const actor = await requireWorkContext(holdingSlug);
  const base = workspaceBase(publicScope(actor));
  const data = await getOrder(actor, id).catch((error) => pageError(error, holdingSlug));
  const { order, lines } = data;
  const purchase = await commercePurchaseControls(actor,{type:"order",id});
  const returnPurchase = data.returns.length ? await commerceReturnPurchaseControls(actor,id,purchase) : {};
  const reconciliationControls = Object.fromEntries(await Promise.all(data.customerNotifications.filter(row=>row.status==="unknown").map(async row=>[String(row.id),await customerNotificationReconciliationControls(actor,String(row.id))] as const)));
  const permissions = commercePermissions(actor);
  const canEdit = order.kind === "purchase" ? permissions.purchase : permissions.sales;
  const emptySalesOptions:SalesEditorOptions={currency:"",products:[],parties:[],warehouses:[],nodes:[],priceLists:[],priceListItems:[]};
  const options = canEdit && order.state === "draft" ? order.kind === "purchase" ? await commerceOverview(actor) : order.kind === "quote" || order.kind === "sales" ? purchase?.newWork===false ? emptySalesOptions : await salesEditorOptions(actor,order.kind) : null : null;
  const linkedRequest = await (await commercePresentationQueries(actor.holdingId)).commerceEntrypointRecordFacts().linkedRequest(actor.companyId,id);
  let requestLink: typeof linkedRequest = null;
  if (linkedRequest) {
    try {
      await (await commercePresentationQueries(actor.holdingId)).getEmployeeRequest(actor, linkedRequest.id);
      requestLink = linkedRequest;
    } catch {}
  }
  const flow = await documentFlow(actor, "order", id);
  const bills =
    order.kind === "purchase" && hasPermission(actor, "finance.read")
      ? (await supplierBillWorkspace(actor)).filter((b) => b.order_id === id)
      : [];
  const billPolicy =
    order.kind === "purchase" && hasPermission(actor, "finance.read")
      ? await supplierInvoicePolicy(actor)
      : null;
  const billReceipts = (bill: (typeof bills)[number]) =>
    JSON.parse(String(bill.receipt_ids_json ?? "[]")) as string[];
  const activeReceiptIds = new Set(
    bills.filter((b) => b.status !== "returned").flatMap(billReceipts),
  );
  const matchableReceipts = data.fulfillments.filter(
    (receipt) => !receipt.invoice_id && !activeReceiptIds.has(String(receipt.id)),
  );
  const supplier = order.kind === "purchase" ? await supplierPromiseDetail(actor, id) : null;
  const landedPurchase=order.kind==="purchase"&&hasPermission(actor,"finance.read")?await landedCostPurchaseControls(actor):undefined;
  const landedCosts =
    order.kind === "purchase" && hasPermission(actor, "finance.read") && landedPurchase?.history!==false
      ? await landedCostsForOrder(actor, id)
      : [];
  const landedOptions = new Map(
    order.kind === "purchase" && hasPermission(actor, "finance.write") && landedPurchase?.newWork!==false
      ? await Promise.all(
          data.fulfillments
            .filter((f) => f.stock_journal_status === "posted")
            .map(
              async (f) => [String(f.id), await landedCostOptions(actor, String(f.id))] as const,
            ),
        )
      : [],
  );
  const fxAccounts=permissions.finance&&hasPermission(actor,"finance.read")&&hasPermission(actor,"finance.approve")
    ?(await (await commercePresentationQueries(actor.holdingId)).commerceEntrypointFinanceFacts().fxAccounts(actor.companyId)).results:[];
  const backLabel =
    order.kind === "purchase" ? "purchasing" : order.kind === "quote" ? "quotations" : "sales";
  return (
    <CommerceEnvironment purchase={purchase}><FeaturePage
      className="ui-page-content"
      data-source="live"
      data-print-document={printing || undefined}
    >
      <WorkReturn base={base} />
      <BusinessDocumentBranding
        actor={actor}
        kind="order"
        recordId={id}
        requestedMode={query.brand}
        title={`${String(order.kind)} order`}
        reference={String(order.number)}
        printing={printing}
      />
      {requestLink && (
        <Link href={canonicalUiHref(`${base}/requests/${requestLink.id}`)}>
          Original purchase request · {requestLink.title}
        </Link>
      )}
      <Link className="back-link" href={canonicalUiHref(`${base}/commerce?view=${order.kind}`)}>
        ← Back to {backLabel}
      </Link>
      <header className="ui-section-header">
        <div>
          <p className="eyebrow">{String(order.kind)} document</p>
          <h1>{String(order.number)}</h1>
          <p className="muted">
            {String(order.party_name)} · {String(order.warehouse_name ?? "Services")} · Required{" "}
            {String(order.due_date)}
          </p>
        </div>
        <StatusBadge appearance="standard" status={String(order.state)} />
      </header>
      <FeatureTabs appearance="standard" />
      {!printing && order.kind !== "purchase" && (
        <Link href={canonicalUiHref(`${base}/support?orderId=${encodeURIComponent(id)}`)}>Customer support</Link>
      )}
      {!printing && order.kind !== "purchase" && (
        <Link href={canonicalUiHref(`${base}/customers/${order.party_id}`)}>Customer 360</Link>
      )}
      {!printing && order.kind !== "purchase" && (
        <Link href={canonicalUiHref(`${base}/commerce/orders/${id}/notes`)}>Customer contact notes (internal)</Link>
      )}
      {!printing && order.kind !== "purchase" && (
        <Link href={canonicalUiHref(`${base}/customer-followups?orderId=${encodeURIComponent(id)}`)}>
          Assign customer follow-up
        </Link>
      )}
      <WidgetGrid pageKey="commerce-orders-detail">
        <RecordNextAction flow={flow} />
        {canEdit && (
          <OrderActions
            base={base}
            order={order}
            canApprove={flow.canAct && permissions.approve && order.created_by !== actor.userId}
          />
        )}
        {!printing && (
          <RecordDocumentPanel appearance="standard"
            base={base}
            targetType={order.kind === "purchase" ? "purchase_order" : "commerce_order"}
            targetId={id}
            expectedTargetVersion={Number(order.version)}
            editable={canEdit}
            title={
              order.kind === "purchase"
                ? "Purchase order documents"
                : "Customer and sales documents"
            }
            purposes={
              order.kind === "purchase"
                ? [
                    {
                      key: "order_document",
                      label: "Purchase order document",
                      description:
                        "Attach the issued purchase order or controlled supporting document.",
                    },
                    {
                      key: "supplier_acknowledgement",
                      label: "Supplier acknowledgement",
                      description:
                        "Attach the supplier's acceptance, confirmation, or correspondence.",
                    },
                  ]
                : [
                    {
                      key: "customer_brief",
                      label: "Customer brief",
                      description: "Attach the customer's requirements or approved brief.",
                    },
                    {
                      key: "signed_quote",
                      label: "Signed quotation",
                      description: "Attach the customer-approved or signed quotation revision.",
                    },
                    {
                      key: "customer_po",
                      label: "Customer purchase order",
                      description: "Attach the customer's signed or issued purchase order.",
                    },
                    {
                      key: "contract",
                      label: "Contract",
                      description: "Attach the executed commercial agreement or contract revision.",
                    },
                    {
                      key: "specification",
                      label: "Specification",
                      description:
                        "Attach the approved product, service, or delivery specification.",
                    },
                  ]
            }
          />
        )}
        {order.state === "submitted" && order.created_by === actor.userId && (
          <p className="muted">Waiting for a different manager to approve this document.</p>
        )}
        <FeatureWidget appearance="standard" id="commerce-orders-detail-order-lines" title="Order lines">
          <Card as="section" appearance="standard" className="ui-panel">
            {order.price_list_id && (
              <p>
                <strong>Price-list snapshot:</strong> {String(order.price_list_id)} · version{" "}
                {String(order.price_list_version)}
                {Number(order.discount_approval_required) > 0
                  ? ` · exceptional discount approved by ${String(order.discount_approved_by ?? "pending manager review")}`
                  : " · within automatic discount authority"}
              </p>
            )}
            {order.discount_reason && <p>Discount reason: {String(order.discount_reason)}</p>}
            <TableFrame appearance="standard" label="Records" className="business-table-wrap">
              <Table className="business-table">
                <thead>
                  <tr>
                    <th>Product</th>
                    <th>Traceability</th>
                    <th>Ordered</th>
                    <th>{order.kind === "purchase" ? "Received" : "Delivered"}</th>
                    <th>Unit price</th>
                    {order.kind !== "purchase" && <th>List / discount</th>}
                    <th>Tax</th>
                    <th>Subtotal</th>
                  </tr>
                </thead>
                <tbody>
                  {lines.map((l) => (
                    <tr key={String(l.id)}>
                      <td>
                        <Link href={canonicalUiHref(`${base}/record-details/product/${encodeURIComponent(String(l.product_id))}`)}>{String(l.sku)} · {String(l.description)}</Link>
                      </td>
                      <td>{l.tracking_mode === "none" ? "None" : String(l.tracking_mode)}</td>
                      <td>
                        {Number(l.quantity_milli) / 1000} {String(l.unit)}
                      </td>
                      <td>
                        {Number(l.fulfilled_milli) / 1000} {String(l.unit)}
                      </td>
                      <td>{minorToDecimal(Number(l.price_minor))}</td>
                      {order.kind !== "purchase" && (
                        <td>
                          {minorToDecimal(Number(l.list_price_minor ?? l.price_minor))} /{" "}
                          {Number(l.discount_bps ?? 0) / 100}%
                        </td>
                      )}
                      <td>{Number(l.tax_bps) / 100}%</td>
                      <td>{minorToDecimal(Number(l.subtotal_minor))}</td>
                    </tr>
                  ))}
                </tbody>
              </Table>
            </TableFrame>
            <p>
              <strong>
                Total {String(order.currency)}{" "}
                {minorToDecimal(Number(order.subtotal_minor) + Number(order.tax_minor))}
              </strong>{" "}
              · Tax {minorToDecimal(Number(order.tax_minor))}
            </p>
            <p className="muted">{String(order.note)}</p>
            {order.source_id && (
              <Link href={canonicalUiHref(`${base}/commerce/orders/${order.source_id}`)}>Source quotation</Link>
            )}
            {data.children.map((c) => (
              <UiActionLink appearance="standard"
                variant="secondary"
                key={String(c.id)}
                href={canonicalUiHref(`${base}/commerce/orders/${c.id}`)}
              >
                Open {String(c.number)}
              </UiActionLink>
            ))}
          </Card>
        </FeatureWidget>
        {supplier && (
          <SupplierPromiseConsole key={String(order.id)} base={base} data={supplier} />
        )}
        {options && (
          <FeatureWidget appearance="standard" id="commerce-orders-detail-order-details" title="Order details">
            <OrderEditor
              base={base}
              options={options}
              kind={String(order.kind)}
              order={order}
              lines={lines}
            />
          </FeatureWidget>
        )}
        {flow.canAct &&
          permissions.stock && lines.some(line => line.item_type !== "service") &&
          order.kind !== "quote" &&
          ["confirmed", "partially_fulfilled"].includes(String(order.state)) && (
            <FulfillmentForm
              base={base}
              order={order}
              lines={lines}
              lots={data.lots}
              pendingEvidence={data.pendingDeliveryEvidence}
            />
          )}
        {flow.canAct && permissions.sales && order.kind === "sales" && ["confirmed", "partially_fulfilled"].includes(String(order.state)) && <ServiceCompletionForm base={base} order={order} lines={lines} />}
        {permissions.stock &&
          order.kind === "sales" &&
          ["confirmed", "partially_fulfilled"].includes(String(order.state)) && (
            <StockReservationForm base={base} order={order} lines={lines.filter(line => line.item_type !== "service")} />
          )}
        {!!data.reservations.length && (
          <FeatureWidget
            id="commerce-orders-detail-stock-reservations"
            title="Stock reservation history"
          >
            <Card as="section" appearance="standard" className="ui-panel">
              {data.reservations.map((r) => (
                <article className="ui-record-item" key={String(r.id)}>
                  <strong>{String(r.description)}</strong>
                  <span>
                    Reserved {Number(r.quantity_milli) / 1000} · Consumed{" "}
                    {Number(r.consumed_milli) / 1000} · Released {Number(r.released_milli) / 1000}
                  </span>
                  <span>Expiry {<TimeText appearance="standard" value={Number(r.expires_at)} />}</span>
                  <span>
                    {String(r.reason)}
                    {r.release_reason ? ` · ${r.release_reason}` : ""}
                  </span>
                </article>
              ))}
            </Card>
          </FeatureWidget>
        )}
        <FeatureWidget
          id="commerce-orders-detail-receipts-deliveries-and-invoices"
          title="Receipts, deliveries and invoices"
        >
          <div className="ui-detail-list">
            {!data.fulfillments.length && (
              <p className="muted">Movements appear here when goods are received or delivered.</p>
            )}
            {order.kind === "purchase" &&
              billPolicy?.matchingMode === "required" &&
              !!matchableReceipts.length && (
                <p>
                  <strong>Company control:</strong> Finance must match supplier evidence to the
                  posted goods receipts and obtain independent review before preparing AP.
                </p>
              )}
            {order.kind === "purchase" && permissions.finance && (
              <SupplierBillCapture
                base={base}
                fulfillments={matchableReceipts}
                lines={data.fulfillmentLines}
                currency={String(order.currency)}
                purchase={billPolicy?.purchase}
              />
            )}
            {data.fulfillments.map((f) => {
              const receiptBills = bills.filter((b) => billReceipts(b).includes(String(f.id)));
              return (
                <article className="ui-detail-entry" key={String(f.id)}>
                  <div className="ui-section-header">
                    <strong>
                      {String(f.number)} · {String(f.fulfillment_date)}
                    </strong>
                    {f.invoice_id ? (
                      <Link href={canonicalUiHref(`${base}/finance/invoices/${f.invoice_id}`)}>
                        {String(f.invoice_number)} · {String(f.invoice_status)} · Outstanding{" "}
                        {minorToDecimal(Number(f.outstanding_minor))}
                      </Link>
                    ) : (
                      <span className="ui-badge">Awaiting invoice</span>
                    )}
                  </div>
                  {permissions.finance && f.journal_id && (
                    <Link href={canonicalUiHref(`${base}/finance/journals/${f.journal_id}`)}>
                      Review goods movement GL
                    </Link>
                  )}
                  {landedOptions.has(String(f.id)) &&
                    (landedOptions.get(String(f.id))!.blocker ? (
                      <aside className="ui-inline-note">
                        <strong>Landed cost blocked</strong>
                        <p className="muted">{landedOptions.get(String(f.id))!.blocker}</p>
                      </aside>
                    ) : (
                      <LandedCostCreateForm
                        base={base}
                        fulfillment={landedOptions.get(String(f.id))!.fulfillment}
                        accounts={landedOptions.get(String(f.id))!.accounts}
                        currency={String(order.currency)}
                        purchase={landedPurchase}
                      />
                    ))}
                  {data.fulfillmentLines
                    .filter((line) => line.fulfillment_id === f.id)
                    .map((line) => {
                      const assigned = data.lotAllocations.filter(
                        (allocation) => allocation.fulfillment_line_id === line.id,
                      );
                      return (
                        <div key={String(line.id)}>
                          <p>
                            {String(line.sku)} · {Number(line.quantity_milli) / 1000}
                          </p>
                          {assigned.map((allocation) => (
                            <p className="muted" key={String(allocation.id)}>
                              {allocation.serial_number
                                ? `Serial ${allocation.serial_number}`
                                : `Lot ${allocation.lot_code}`}{" "}
                              · {Number(allocation.quantity_milli) / 1000} ·{" "}
                              {String(allocation.disposition)}
                              {allocation.expiry_date ? ` · expires ${allocation.expiry_date}` : ""}
                            </p>
                          ))}
                        </div>
                      );
                    })}
                  {f.evidence_note && (
                    <p>
                      <strong>Delivery confirmation:</strong> {String(f.evidence_note)}
                    </p>
                  )}
                  {data.fulfillmentEvidence
                    .filter((e) => e.fulfillment_id === f.id)
                    .map((e) => (
                      <p key={String(e.attachment_id)}>
                        <Link href={`/api/v1${base}/attachments/${e.attachment_id}/content`}>
                          Proof of delivery · {String(e.original_name)}
                        </Link>{" "}
                        · {String(e.media_type)} · {Number(e.bytes)} bytes
                      </p>
                    ))}
                  {order.kind === "sales" && permissions.sales && (
                    <CustomerDeliveryNotification
                      base={base}
                      fulfillment={f}
                      notification={data.customerNotifications.find(
                        (notification) => notification.fulfillment_id === f.id,
                      )}
                      reconciliation={reconciliationControls[String(data.customerNotifications.find(notification=>notification.fulfillment_id===f.id)?.id??"")]}
                      customerEmail={String(order.notification_email ?? "")}
                      customerConsent={String(order.notification_consent ?? "unknown")}
                      adapter={String(data.customerNotificationSettings.adapter)}
                    />
                  )}
                  {order.kind === "purchase" &&
                    permissions.finance &&
                    receiptBills.map((b) => (
                      <Link
                        className="ui-record-item"
                        key={String(b.id)}
                        href={canonicalUiHref(`${base}/supplier-bills/${b.id}`)}
                      >
                        Supplier invoice {String(b.reference)} · {String(b.status)} ·{" "}
                        {String(b.receipt_count)} receipt(s)
                      </Link>
                    ))}
                  {!receiptBills.length &&
                    billPolicy?.matchingMode !== "required" &&
                    flow.steps.find((s) => s.key === "invoicing")?.primary?.userId ===
                      actor.userId &&
                    permissions.finance &&
                    !f.invoice_id && <InvoiceDeliveryForm base={base} fulfillment={f} useSalesDefaults={order.kind === "sales"} completions={order.kind === "sales" ? data.fulfillments.filter(other => other.id !== f.id && !other.invoice_id && (other.completion_kind === "service" || other.stock_journal_status === "posted")) : []} />}
                  {canEdit &&
                    f.invoice_id &&
                    ["posted", "partially_paid", "paid"].includes(String(f.invoice_status)) && (
                      <ReturnRequestForm
                        base={base}
                        fulfillment={f}
                        lines={data.fulfillmentLines}
                        lotAllocations={data.lotAllocations}
                        customerReturn={order.kind === "sales"}
                      />
                    )}
                </article>
              );
            })}
          </div>
        </FeatureWidget>
        {order.kind === "purchase" && hasPermission(actor, "finance.read") && (
          <FeatureWidget appearance="standard" id="commerce-orders-detail-landed-costs" title="Landed costs">
            <Card as="section" appearance="standard" className="ui-panel">
              {!landedCosts.length && (
                <p className="muted">
                  Freight, duty and handling allocations will appear here after Finance submits them
                  from a posted receipt.
                </p>
              )}
              {landedCosts.map((c) => (
                <Link className="ui-record-item" key={String(c.id)} href={canonicalUiHref(`${base}/landed-costs/${c.id}`)}>
                  <strong>
                    {String(c.number)} · {String(c.currency)}{" "}
                    {minorToDecimal(Number(c.amount_minor))}
                  </strong>
                  <span>
                    {String(c.allocation_method).replaceAll("_", " ")} · {String(c.status)}
                  </span>
                </Link>
              ))}
            </Card>
          </FeatureWidget>
        )}
        <FeatureWidget
          id="commerce-orders-detail-returns-and-adjustments"
          title="Returns and adjustments"
        >
          <Card as="section" appearance="standard" className="ui-panel">
            {!data.returns.length && (
              <p className="muted">
                Approved customer and supplier returns appear here with their stock and
                credit/debit-note journals.
              </p>
            )}
            {data.returns.map((r) => (
              <Card as="article" appearance="standard" className="ui-panel" key={String(r.id)}>
                <div className="ui-section-header">
                  <strong>
                    {String(r.number)} · {minorToDecimal(Number(r.total_minor))}{" "}
                    {String(order.currency)}
                  </strong>
                  <StatusBadge appearance="standard" status={String(r.status)} />
                </div>
                <p>{String(r.reason)}</p>
                {data.returnLotAllocations
                  .filter((a) => a.return_id === r.id)
                  .map((a) => (
                    <p className="muted" key={String(a.id)}>
                      {a.serial_number
                        ? `Serial ${String(a.serial_number)}`
                        : `Lot ${String(a.lot_code)}`}{" "}
                      · {Number(a.quantity_milli) / 1000} · stock value{" "}
                      {minorToDecimal(Number(a.value_minor))} {String(r.stock_currency)}
                    </p>
                  ))}
                {data.returnQualityHolds
                  .filter((h) => h.return_id === r.id)
                  .map((h) => (
                    <p key={String(h.id)}>
                      <Link href={canonicalUiHref(`${base}/quality/${h.id}`)}>
                        Quality disposition ·{" "}
                        {h.serial_number
                          ? `Serial ${String(h.serial_number)}`
                          : h.lot_code
                            ? `Lot ${String(h.lot_code)}`
                            : "returned quantity"}{" "}
                        · {String(h.status)}
                      </Link>
                      <span className="muted">
                        {" "}
                        · {String(h.condition_code).replaceAll("_", " ")} · due{" "}
                        {String(h.quality_due_date)}
                      </span>
                    </p>
                  ))}
                {r.credited_minor !== null && (
                  <p>
                    Invoice credit {minorToDecimal(Number(r.credited_minor))} · Refund due{" "}
                    {minorToDecimal(Number(r.refund_minor))} {String(order.currency)}
                  </p>
                )}
                {Number(r.refund_minor) > 0 && permissions.finance && (
                  <Link href={canonicalUiHref(`${base}/finance/refunds/refund-${r.id}`)}>
                    Review refund obligation
                  </Link>
                )}
                <div className="ui-form-actions">
                  {r.stock_journal_id && (
                    <Link href={canonicalUiHref(`${base}/finance/journals/${r.stock_journal_id}`)}>
                      Stock reversal GL · {String(r.stock_journal_status)}
                    </Link>
                  )}
                  {r.credit_journal_id && (
                    <Link href={canonicalUiHref(`${base}/finance/journals/${r.credit_journal_id}`)}>
                      {r.kind === "customer_return" ? "Credit" : "Debit"} note GL ·{" "}
                      {String(r.credit_journal_status)}
                    </Link>
                  )}
                  {r.goods_fx_journal_id&&<Link href={canonicalUiHref(`${base}/finance/journals/${encodeURIComponent(String(r.goods_fx_journal_id))}`)}>Original goods FX reversal GL · {String(r.goods_fx_journal_status)}</Link>}
                </div>
                <CommercePurchaseProvider purchase={returnPurchase[String(r.id)] ?? purchase}><ReturnActions
                  base={base}
                  record={r}
                  fxAccounts={fxAccounts}
                  canApprove={permissions.approve && r.requested_by !== actor.userId}
                  canComplete={
                    permissions.finance &&
                    hasPermission(actor, "finance.approve") &&
                    (!r.stock_journal_id || r.stock_journal_status === "posted") &&
                    r.credit_journal_status === "posted" &&
                    (!r.goods_fx_journal_id || r.goods_fx_journal_status === "posted")
                  }
                /></CommercePurchaseProvider>
              </Card>
            ))}
          </Card>
        </FeatureWidget>
        <FeatureWidget appearance="standard" id="commerce-orders-detail-audit-trail" title="Audit trail">
          <Card as="section" appearance="standard" className="ui-panel">
            {data.history.map((h, i) => (
              <div className="ui-record-item" key={i}>
                <strong>
                  {String(h.action).replaceAll("commerce.order.", "").replaceAll("_", " ")}
                </strong>
                <span>
                  {String(h.actor_name ?? h.actor_id)} · Version {String(h.record_version)}
                </span>
                <time>{<TimeText appearance="standard" value={Number(h.created_at)} />}</time>
              </div>
            ))}
          </Card>
        </FeatureWidget>
        <FlowPanel flow={flow} actor={actor} kind="order" id={id} base={base} />
      </WidgetGrid>
    </FeaturePage></CommerceEnvironment>
  );
}
