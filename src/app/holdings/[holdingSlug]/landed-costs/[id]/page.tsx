import Link from 'next/link';
import {StatusBadge} from '@/components/ui/primitives';
import {LandedCostActions} from '@/components/modules/landed-cost-controls';
import {minorToDecimal} from '@/lib/finance-money';
import {hasPermission} from '@/lib/permissions';
import {samePerson} from '@/lib/iam';
import {workspaceBase} from '@/lib/workspace';
import {publicScope,requireWorkContext} from '@/server/authorization';
import {landedCostDetail} from '@/server/inventory-landed-costs';
import {pageError} from '@/server/page-error';

export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor)),data=await landedCostDetail(actor,id).catch(e=>pageError(e,holdingSlug)),{cost,lines,lots,events}=data;
 const canApprove=!actor.branchId&&hasPermission(actor,'finance.approve')&&!samePerson(actor,cost.created_by),canCancel=hasPermission(actor,'finance.write')&&samePerson(actor,cost.created_by);
 return <div className="content-stack"><Link className="back-link" href={`${base}/commerce/orders/${cost.order_id}`}>← Purchase {String(cost.order_number)}</Link><Link href={`${base}/work`}>Back to Work</Link><header className="section-head"><div><p className="eyebrow">Inventory landed cost</p><h1>{String(cost.number)}</h1><p>{String(cost.description)}</p><p className="muted">Purchase {String(cost.order_number)} · Receipt {String(cost.fulfillment_number)} · {String(cost.warehouse_name)} · {String(cost.cost_date)}</p></div><StatusBadge status={String(cost.status)}/></header>
  <section className="card"><p><strong>{String(cost.currency)} {minorToDecimal(Number(cost.amount_minor))}</strong> allocated by {String(cost.allocation_method).replaceAll('_',' ')}</p><p className="muted">Prepared records keep their stock and lot snapshots. Approval and posting stop if quantity or value changed after preparation.</p>{cost.journal_id&&<Link className="button secondary" href={`${base}/finance/journals/${cost.journal_id}`}>Open posted journal</Link>}</section>
  <LandedCostActions key={String(cost.version)} base={base} record={cost} canApprove={canApprove} canCancel={canCancel}/>
  <section className="card"><h2>Allocation by item</h2><div className="business-table-wrap"><table className="business-table"><thead><tr><th>Item</th><th>Received</th><th>Receipt value</th><th>Allocated cost</th></tr></thead><tbody>{lines.map(l=><tr key={String(l.id)}><td>{String(l.sku)} · {String(l.name)}</td><td>{Number(l.quantity_milli)/1000} {String(l.unit)}</td><td>{minorToDecimal(Number(l.receipt_value_minor))}</td><td>{minorToDecimal(Number(l.allocated_minor))}</td></tr>)}</tbody></table></div></section>
  {!!lots.length&&<section className="card"><h2>Lot and serial allocation</h2><div className="business-table-wrap"><table className="business-table"><thead><tr><th>Lot / serial</th><th>Quantity</th><th>Receipt value</th><th>Allocated cost</th></tr></thead><tbody>{lots.map(l=><tr key={String(l.id)}><td>{l.serial_number?`Serial ${l.serial_number}`:`Lot ${l.lot_code}`}</td><td>{Number(l.quantity_milli)/1000}</td><td>{minorToDecimal(Number(l.receipt_value_minor))}</td><td>{minorToDecimal(Number(l.allocated_minor))}</td></tr>)}</tbody></table></div></section>}
  <section className="card"><h2>Audit history</h2>{events.map(e=><div className="row" key={String(e.id)}><strong>{String(e.action).replaceAll('_',' ')}</strong><span>{String(e.reason)} · Version {String(e.reviewed_version)}</span><time>{new Date(Number(e.created_at)).toISOString()}</time></div>)}</section>
 </div>;
}
