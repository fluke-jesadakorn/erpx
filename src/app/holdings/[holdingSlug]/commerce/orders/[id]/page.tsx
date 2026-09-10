import { holdingResources } from "@/server/runtime";
import { getEmployeeRequest } from "@/server/employee-requests";
import Link from "next/link";
import { requireWorkContext,publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { getOrder,commerceOverview,commercePermissions } from "@/server/commerce";
import { OrderActions,OrderEditor,FulfillmentForm,InvoiceDeliveryForm } from "@/components/modules/commerce-controls";
import { StatusBadge } from "@/components/ui/primitives";
import { minorToDecimal } from "@/lib/finance-money";
export const dynamic="force-dynamic";
export default async function OrderPage({params}:{params:Promise<{holdingSlug:string;id:string}>}) {
  const {holdingSlug,id}=await params;const actor=await requireWorkContext(holdingSlug);const base=workspaceBase(publicScope(actor));const data=await getOrder(actor,id);const {order,lines}=data;const permissions=commercePermissions(actor);const canEdit=order.kind==="purchase"?permissions.purchase:permissions.sales;const options=canEdit&&order.state==="draft"?await commerceOverview(actor):null;
  const linkedRequest=await (await holdingResources(actor.holdingId)).write.prepare("SELECT id,title FROM employee_requests WHERE company_id=? AND purchase_order_id=?").bind(actor.companyId,id).first<{id:string;title:string}>();
  let requestLink:typeof linkedRequest=null;
  if(linkedRequest){try{await getEmployeeRequest((await holdingResources(actor.holdingId)).write,actor,linkedRequest.id);requestLink=linkedRequest;}catch{}}
  return <main className="content-stack" data-source="live">{requestLink&&<Link href={`${base}/requests/${requestLink.id}`}>Original purchase request · {requestLink.title}</Link>}<Link href={`${base}/commerce?view=${order.kind}`}>← Back to {order.kind==="purchase"?"purchasing":"sales"}</Link><header className="section-head"><div><p className="eyebrow">{String(order.kind)} document</p><h1>{String(order.number)}</h1><p className="muted">{String(order.party_name)} · {String(order.warehouse_name)} · Required {String(order.due_date)}</p></div><StatusBadge status={String(order.state)}/></header>
    {canEdit&&<OrderActions base={base} order={order} canApprove={permissions.approve&&order.created_by!==actor.userId}/>}
    {order.state==="submitted"&&order.created_by===actor.userId&&<p className="muted">Waiting for a different manager to approve this document.</p>}
    <section className="card"><h2>Order lines</h2><div className="business-table-wrap"><table className="business-table"><thead><tr><th>Product</th><th>Ordered</th><th>{order.kind==="purchase"?"Received":"Delivered"}</th><th>Unit price</th><th>Tax</th><th>Subtotal</th></tr></thead><tbody>{lines.map(l=><tr key={String(l.id)}><td>{String(l.sku)} · {String(l.description)}</td><td>{Number(l.quantity_milli)/1000}</td><td>{Number(l.fulfilled_milli)/1000}</td><td>{minorToDecimal(Number(l.price_minor))}</td><td>{Number(l.tax_bps)/100}%</td><td>{minorToDecimal(Number(l.subtotal_minor))}</td></tr>)}</tbody></table></div><p><strong>Total {String(order.currency)} {minorToDecimal(Number(order.subtotal_minor)+Number(order.tax_minor))}</strong> · Tax {minorToDecimal(Number(order.tax_minor))}</p><p className="muted">{String(order.note)}</p>{order.source_id&&<Link href={`${base}/commerce/orders/${order.source_id}`}>Source quotation</Link>}{data.children.map(c=><Link className="button secondary" key={String(c.id)} href={`${base}/commerce/orders/${c.id}`}>Open {String(c.number)}</Link>)}</section>
    {options&&<OrderEditor base={base} options={options} kind={String(order.kind)} order={order} lines={lines}/>}
    {permissions.stock&&order.kind!=="quote"&&["confirmed","partially_fulfilled"].includes(String(order.state))&&<FulfillmentForm base={base} order={order} lines={lines}/>}
    <section className="card"><h2>Receipts, deliveries and invoices</h2>{!data.fulfillments.length&&<p className="muted">Movements appear here when goods are received or delivered.</p>}{data.fulfillments.map(f=><article className="card" key={String(f.id)}><div className="section-head"><strong>{String(f.number)} · {String(f.fulfillment_date)}</strong>{f.invoice_id?<Link href={`${base}/finance/invoices/${f.invoice_id}`}>{String(f.invoice_number)} · {String(f.invoice_status)} · Outstanding {minorToDecimal(Number(f.outstanding_minor))}</Link>:<span className="badge">Awaiting invoice</span>}</div>{permissions.finance&&!f.invoice_id&&<InvoiceDeliveryForm base={base} fulfillment={f}/>}</article>)}</section>
    <section className="card"><h2>Audit trail</h2>{data.history.map((h,i)=><div className="row" key={i}><strong>{String(h.action).replaceAll("commerce.order.","").replaceAll("_"," ")}</strong><span>{String(h.actor_name??h.actor_id)} · Version {String(h.record_version)}</span><time>{new Date(Number(h.created_at)).toISOString()}</time></div>)}</section>
  </main>;
}
