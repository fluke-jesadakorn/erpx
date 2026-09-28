import { BusinessDocumentBranding } from "@/components/brand/business-document";
import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { CustomerDeliveryNotification,StockReservationForm,FulfillmentForm,InvoiceDeliveryForm,OrderActions,OrderEditor,ReturnActions,ReturnRequestForm } from "@/components/modules/commerce-controls";
import { StatusBadge } from "@/components/ui/primitives";
import { FlowPanel,documentFlow } from "@/components/workplace/flow-panel";
import { RecordNextAction } from "@/components/workplace/record-next-action";
import { WorkReturn } from "@/components/workplace/work-return";
import { minorToDecimal } from "@/lib/finance-money";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { commerceOverview,commercePermissions,getOrder } from "@/server/commerce";
import { getEmployeeRequest } from "@/server/employee-requests";
import { holdingResources } from "@/server/runtime";
import Link from "next/link";
import {supplierPromiseDetail} from "@/server/supplier-promises";
import {SupplierBillCapture} from "@/components/modules/supplier-bill-console";
import {supplierBillWorkspace,supplierInvoicePolicy} from "@/server/supplier-bill-matching";
import {SupplierPromiseConsole} from "@/components/modules/supplier-promise-console";
import {pageError} from "@/server/page-error";
import {landedCostsForOrder,landedCostOptions} from "@/server/inventory-landed-costs";
import {LandedCostCreateForm} from "@/components/modules/landed-cost-controls";
import {RecordDocumentPanel} from "@/components/attachments/record-document-panel";
export const dynamic="force-dynamic";
export default async function OrderPage({params,searchParams}:{params:Promise<{holdingSlug:string;id:string}>;searchParams:Promise<{print?:string;brand?:string}>}) {
  const {holdingSlug,id}=await params;const query=await searchParams;const printing=query.print==="1";const actor=await requireWorkContext(holdingSlug);const base=workspaceBase(publicScope(actor));const data=await getOrder(actor,id).catch(error=>pageError(error,holdingSlug));const {order,lines}=data;
  const permissions=commercePermissions(actor);const canEdit=order.kind==="purchase"?permissions.purchase:permissions.sales;const options=canEdit&&order.state==="draft"?await commerceOverview(actor):null;
  const linkedRequest=await (await holdingResources(actor.holdingId)).write.prepare("SELECT id,title FROM employee_requests WHERE company_id=? AND purchase_order_id=?").bind(actor.companyId,id).first<{id:string;title:string}>();
  let requestLink:typeof linkedRequest=null;
  if(linkedRequest){try{await getEmployeeRequest((await holdingResources(actor.holdingId)).write,actor,linkedRequest.id);requestLink=linkedRequest;}catch{}}
  const flow = await documentFlow(actor,"order",id);
  const bills=order.kind==='purchase'&&hasPermission(actor,'finance.read')?(await supplierBillWorkspace(actor)).filter(b=>b.order_id===id):[];
  const billPolicy=order.kind==='purchase'&&hasPermission(actor,'finance.read')?await supplierInvoicePolicy(actor):null;
  const billReceipts=(bill:(typeof bills)[number])=>JSON.parse(String(bill.receipt_ids_json??'[]')) as string[];
  const activeReceiptIds=new Set(bills.filter(b=>b.status!=='returned').flatMap(billReceipts));
  const matchableReceipts=data.fulfillments.filter(receipt=>!receipt.invoice_id&&!activeReceiptIds.has(String(receipt.id)));
  const supplier=order.kind==='purchase'?await supplierPromiseDetail(actor,id):null;
  const landedCosts=order.kind==='purchase'&&hasPermission(actor,'finance.read')?await landedCostsForOrder(actor,id):[];
  const landedOptions=new Map(order.kind==='purchase'&&hasPermission(actor,'finance.write')?await Promise.all(data.fulfillments.filter(f=>f.stock_journal_status==='posted').map(async f=>[String(f.id),await landedCostOptions(actor,String(f.id))] as const)):[]);
  const backLabel=order.kind==="purchase"?"purchasing":order.kind==="quote"?"quotations":"sales";
  return <FeaturePage className="content-stack" data-source="live" data-print-document={printing||undefined}><WorkReturn base={base}/><BusinessDocumentBranding actor={actor} kind="order" recordId={id} requestedMode={query.brand} title={`${String(order.kind)} order`} reference={String(order.number)} printing={printing}/>{requestLink&&<Link href={`${base}/requests/${requestLink.id}`}>Original purchase request · {requestLink.title}</Link>}<Link className="back-link" href={`${base}/commerce?view=${order.kind}`}>← Back to {backLabel}</Link><header className="section-head"><div><p className="eyebrow">{String(order.kind)} document</p><h1>{String(order.number)}</h1><p className="muted">{String(order.party_name)} · {String(order.warehouse_name)} · Required {String(order.due_date)}</p></div><StatusBadge status={String(order.state)}/></header><FeatureTabs/>{!printing&&order.kind!=="purchase"&&<Link href={`${base}/support?orderId=${encodeURIComponent(id)}`}>Customer support</Link>}{!printing&&order.kind!=="purchase"&&<Link href={`${base}/customers/${order.party_id}`}>Customer 360</Link>}{!printing&&order.kind!=="purchase"&&<Link href={`${base}/commerce/orders/${id}/notes`}>Customer contact notes (internal)</Link>}{!printing&&order.kind!=="purchase"&&<Link href={`${base}/customer-followups?orderId=${encodeURIComponent(id)}`}>Assign customer follow-up</Link>}<WidgetGrid pageKey="commerce-orders-detail"><RecordNextAction flow={flow}/>
    {canEdit&&<OrderActions base={base} order={order} canApprove={flow.canAct&&permissions.approve&&order.created_by!==actor.userId}/>}
    {!printing&&<RecordDocumentPanel
      base={base}
      targetType={order.kind==="purchase"?"purchase_order":"commerce_order"}
      targetId={id}
      expectedTargetVersion={Number(order.version)}
      editable={canEdit}
      title={order.kind==="purchase"?"Purchase order documents":"Customer and sales documents"}
      purposes={order.kind==="purchase"
        ? [{key:"order_document",label:"Purchase order document",description:"Attach the issued purchase order or controlled supporting document."},{key:"supplier_acknowledgement",label:"Supplier acknowledgement",description:"Attach the supplier's acceptance, confirmation, or correspondence."}]
        : [{key:"customer_brief",label:"Customer brief",description:"Attach the customer's requirements or approved brief."},{key:"signed_quote",label:"Signed quotation",description:"Attach the customer-approved or signed quotation revision."},{key:"customer_po",label:"Customer purchase order",description:"Attach the customer's signed or issued purchase order."},{key:"contract",label:"Contract",description:"Attach the executed commercial agreement or contract revision."},{key:"specification",label:"Specification",description:"Attach the approved product, service, or delivery specification."}]}
    />}
    {order.state==="submitted"&&order.created_by===actor.userId&&<p className="muted">Waiting for a different manager to approve this document.</p>}
    <FeatureWidget id="commerce-orders-detail-order-lines" title="Order lines"><section className="card">{order.price_list_id&&<p><strong>Price-list snapshot:</strong> {String(order.price_list_id)} · version {String(order.price_list_version)}{Number(order.discount_approval_required)>0?` · exceptional discount approved by ${String(order.discount_approved_by??'pending manager review')}`:' · within automatic discount authority'}</p>}{order.discount_reason&&<p>Discount reason: {String(order.discount_reason)}</p>}<div className="business-table-wrap"><table className="business-table"><thead><tr><th>Product</th><th>Traceability</th><th>Ordered</th><th>{order.kind==="purchase"?"Received":"Delivered"}</th><th>Unit price</th>{order.kind!=="purchase"&&<th>List / discount</th>}<th>Tax</th><th>Subtotal</th></tr></thead><tbody>{lines.map(l=><tr key={String(l.id)}><td>{String(l.sku)} · {String(l.description)}</td><td>{l.tracking_mode==="none"?"None":String(l.tracking_mode)}</td><td>{Number(l.quantity_milli)/1000} {String(l.unit)}</td><td>{Number(l.fulfilled_milli)/1000} {String(l.unit)}</td><td>{minorToDecimal(Number(l.price_minor))}</td>{order.kind!=="purchase"&&<td>{minorToDecimal(Number(l.list_price_minor??l.price_minor))} / {Number(l.discount_bps??0)/100}%</td>}<td>{Number(l.tax_bps)/100}%</td><td>{minorToDecimal(Number(l.subtotal_minor))}</td></tr>)}</tbody></table></div><p><strong>Total {String(order.currency)} {minorToDecimal(Number(order.subtotal_minor)+Number(order.tax_minor))}</strong> · Tax {minorToDecimal(Number(order.tax_minor))}</p><p className="muted">{String(order.note)}</p>{order.source_id&&<Link href={`${base}/commerce/orders/${order.source_id}`}>Source quotation</Link>}{data.children.map(c=><Link className="button secondary" key={String(c.id)} href={`${base}/commerce/orders/${c.id}`}>Open {String(c.number)}</Link>)}</section></FeatureWidget>
    {supplier&&<SupplierPromiseConsole key={Number(order.version)} base={base} data={supplier}/>}
    {options&&<FeatureWidget id="commerce-orders-detail-order-details" title="Order details"><OrderEditor base={base} options={options} kind={String(order.kind)} order={order} lines={lines}/></FeatureWidget>}
    {flow.canAct&&permissions.stock&&order.kind!=="quote"&&["confirmed","partially_fulfilled"].includes(String(order.state))&&<FulfillmentForm base={base} order={order} lines={lines} lots={data.lots} pendingEvidence={data.pendingDeliveryEvidence}/>}
    {permissions.stock&&order.kind==='sales'&&['confirmed','partially_fulfilled'].includes(String(order.state))&&<StockReservationForm base={base} order={order} lines={lines}/>}
    {!!data.reservations.length&&<FeatureWidget id="commerce-orders-detail-stock-reservations" title="Stock reservation history"><section className="card">{data.reservations.map(r=><article className="row" key={String(r.id)}><strong>{String(r.description)}</strong><span>Reserved {Number(r.quantity_milli)/1000} · Consumed {Number(r.consumed_milli)/1000} · Released {Number(r.released_milli)/1000}</span><span>Expiry {new Date(Number(r.expires_at)).toISOString()}</span><span>{String(r.reason)}{r.release_reason?` · ${r.release_reason}`:''}</span></article>)}</section></FeatureWidget>}
    <FeatureWidget id="commerce-orders-detail-receipts-deliveries-and-invoices" title="Receipts, deliveries and invoices">
      <section className="card">
        {!data.fulfillments.length&&<p className="muted">Movements appear here when goods are received or delivered.</p>}
        {order.kind==='purchase'&&billPolicy?.matchingMode==='required'&&!!matchableReceipts.length&&<p><strong>Company control:</strong> Finance must match supplier evidence to the posted goods receipts and obtain independent review before preparing AP.</p>}
        {order.kind==='purchase'&&permissions.finance&&!!matchableReceipts.length&&<SupplierBillCapture base={base} fulfillments={matchableReceipts} lines={data.fulfillmentLines} currency={String(order.currency)}/>}
        {data.fulfillments.map(f=>{
          const receiptBills=bills.filter(b=>billReceipts(b).includes(String(f.id)));
          return <article className="card" key={String(f.id)}>
            <div className="section-head">
              <strong>{String(f.number)} · {String(f.fulfillment_date)}</strong>
              {f.invoice_id?<Link href={`${base}/finance/invoices/${f.invoice_id}`}>{String(f.invoice_number)} · {String(f.invoice_status)} · Outstanding {minorToDecimal(Number(f.outstanding_minor))}</Link>:<span className="badge">Awaiting invoice</span>}
            </div>
            {permissions.finance&&f.journal_id&&<Link href={`${base}/finance/journals/${f.journal_id}`}>Review goods movement GL</Link>}
            {landedOptions.has(String(f.id))&&(landedOptions.get(String(f.id))!.blocker?<section className="card"><strong>Landed cost blocked</strong><p className="muted">{landedOptions.get(String(f.id))!.blocker}</p></section>:<LandedCostCreateForm base={base} fulfillment={landedOptions.get(String(f.id))!.fulfillment} accounts={landedOptions.get(String(f.id))!.accounts} currency={String(order.currency)}/>)}
            {data.fulfillmentLines.filter(line=>line.fulfillment_id===f.id).map(line=>{const assigned=data.lotAllocations.filter(allocation=>allocation.fulfillment_line_id===line.id);return <div key={String(line.id)}><p>{String(line.sku)} · {Number(line.quantity_milli)/1000}</p>{assigned.map(allocation=><p className="muted" key={String(allocation.id)}>{allocation.serial_number?`Serial ${allocation.serial_number}`:`Lot ${allocation.lot_code}`} · {Number(allocation.quantity_milli)/1000} · {String(allocation.disposition)}{allocation.expiry_date?` · expires ${allocation.expiry_date}`:""}</p>)}</div>;})}
            {f.evidence_note&&<p><strong>Delivery confirmation:</strong> {String(f.evidence_note)}</p>}
            {data.fulfillmentEvidence.filter(e=>e.fulfillment_id===f.id).map(e=><p key={String(e.attachment_id)}><Link href={`/api/v1${base}/attachments/${e.attachment_id}/content`}>Proof of delivery · {String(e.original_name)}</Link> · {String(e.media_type)} · {Number(e.bytes)} bytes</p>)}
            {order.kind==='sales'&&permissions.sales&&<CustomerDeliveryNotification base={base} fulfillment={f} notification={data.customerNotifications.find(notification=>notification.fulfillment_id===f.id)} customerEmail={String(order.notification_email??'')} customerConsent={String(order.notification_consent??'unknown')} adapter={String(data.customerNotificationSettings.adapter)}/>}
            {order.kind==='purchase'&&permissions.finance&&receiptBills.map(b=><Link className="row" key={String(b.id)} href={`${base}/supplier-bills/${b.id}`}>Supplier invoice {String(b.reference)} · {String(b.status)} · {String(b.receipt_count)} receipt(s)</Link>)}
            {!receiptBills.length&&billPolicy?.matchingMode!=='required'&&flow.steps.find(s=>s.key==="invoicing")?.primary?.userId===actor.userId&&permissions.finance&&!f.invoice_id&&<InvoiceDeliveryForm base={base} fulfillment={f}/>}
            {canEdit&&f.invoice_id&&["posted","partially_paid","paid"].includes(String(f.invoice_status))&&<ReturnRequestForm base={base} fulfillment={f} lines={data.fulfillmentLines} lotAllocations={data.lotAllocations} customerReturn={order.kind==='sales'}/>}
          </article>;
        })}
      </section>
    </FeatureWidget>
    {order.kind==='purchase'&&hasPermission(actor,'finance.read')&&<FeatureWidget id="commerce-orders-detail-landed-costs" title="Landed costs"><section className="card">{!landedCosts.length&&<p className="muted">Freight, duty and handling allocations will appear here after Finance submits them from a posted receipt.</p>}{landedCosts.map(c=><Link className="row" key={String(c.id)} href={`${base}/landed-costs/${c.id}`}><strong>{String(c.number)} · {String(c.currency)} {minorToDecimal(Number(c.amount_minor))}</strong><span>{String(c.allocation_method).replaceAll('_',' ')} · {String(c.status)}</span></Link>)}</section></FeatureWidget>}
    <FeatureWidget id="commerce-orders-detail-returns-and-adjustments" title="Returns and adjustments"><section className="card">{!data.returns.length&&<p className="muted">Approved customer and supplier returns appear here with their stock and credit/debit-note journals.</p>}{data.returns.map(r=><article className="card" key={String(r.id)}><div className="section-head"><strong>{String(r.number)} · {minorToDecimal(Number(r.total_minor))} {String(order.currency)}</strong><StatusBadge status={String(r.status)}/></div><p>{String(r.reason)}</p>{data.returnLotAllocations.filter(a=>a.return_id===r.id).map(a=><p className="muted" key={String(a.id)}>{a.serial_number?`Serial ${String(a.serial_number)}`:`Lot ${String(a.lot_code)}`} · {Number(a.quantity_milli)/1000} · stock value {minorToDecimal(Number(a.value_minor))} {String(order.currency)}</p>)}{data.returnQualityHolds.filter(h=>h.return_id===r.id).map(h=><p key={String(h.id)}><Link href={`${base}/quality/${h.id}`}>Quality disposition · {h.serial_number?`Serial ${String(h.serial_number)}`:h.lot_code?`Lot ${String(h.lot_code)}`:'returned quantity'} · {String(h.status)}</Link><span className="muted"> · {String(h.condition_code).replaceAll('_',' ')} · due {String(h.quality_due_date)}</span></p>)}{r.credited_minor!==null&&<p>Invoice credit {minorToDecimal(Number(r.credited_minor))} · Refund due {minorToDecimal(Number(r.refund_minor))} {String(order.currency)}</p>}{Number(r.refund_minor)>0&&permissions.finance&&<Link href={`${base}/finance/refunds/refund-${r.id}`}>Review refund obligation</Link>}<div className="form-actions">{r.stock_journal_id&&<Link href={`${base}/finance/journals/${r.stock_journal_id}`}>Stock reversal GL · {String(r.stock_journal_status)}</Link>}{r.credit_journal_id&&<Link href={`${base}/finance/journals/${r.credit_journal_id}`}>{r.kind==="customer_return"?"Credit":"Debit"} note GL · {String(r.credit_journal_status)}</Link>}</div><ReturnActions base={base} record={r} canApprove={permissions.approve&&r.requested_by!==actor.userId} canComplete={permissions.finance&&hasPermission(actor,"finance.approve")&&r.stock_journal_status==="posted"&&r.credit_journal_status==="posted"}/></article>)}</section></FeatureWidget>
    <FeatureWidget id="commerce-orders-detail-audit-trail" title="Audit trail"><section className="card">{data.history.map((h,i)=><div className="row" key={i}><strong>{String(h.action).replaceAll("commerce.order.","").replaceAll("_"," ")}</strong><span>{String(h.actor_name??h.actor_id)} · Version {String(h.record_version)}</span><time>{new Date(Number(h.created_at)).toISOString()}</time></div>)}</section></FeatureWidget>
  <FlowPanel flow={flow} actor={actor} kind="order" id={id} base={base}/></WidgetGrid></FeaturePage>;
}
