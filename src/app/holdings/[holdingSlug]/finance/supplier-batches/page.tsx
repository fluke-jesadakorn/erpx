import Link from 'next/link';
import {requireWorkContext,publicScope} from '@/server/authorization';
import {workspaceBase} from '@/lib/workspace';
import {listSupplierBatches,supplierBatchWorkspace} from '@/server/supplier-payments';
import {SupplierBatchConsole} from '@/components/modules/supplier-batch-console';
import {pageError} from '@/server/page-error';
export const dynamic='force-dynamic';
export default async function SupplierBatchesPage({params}:{params:Promise<{holdingSlug:string}>}) {
 const actor=await requireWorkContext((await params).holdingSlug),base=workspaceBase(publicScope(actor));
 const [rows,data]=await Promise.all([listSupplierBatches(actor).catch(error=>pageError(error,actor.holdingSlug)),supplierBatchWorkspace(actor).catch(error=>pageError(error,actor.holdingSlug))]);
 return <div className="content-stack"><Link href={`${base}/finance`}>← Finance</Link><h1>Supplier payment batches</h1><p>Reserve invoice balances, review beneficiaries independently, then export instructions for manual bank payment. Confirm each result against imported bank evidence.</p><section className="card"><h2>Recent batches</h2>{rows.length?<table className="business-table"><thead><tr><th>Planned date</th><th>Currency</th><th>State</th><th>Pending instructions</th></tr></thead><tbody>{rows.map(b=><tr key={String(b.id)}><td><Link href={`${base}/finance/supplier-batches/${b.id}`}>{String(b.planned_date)}</Link></td><td>{String(b.currency)}</td><td>{String(b.status)}</td><td>{String(b.pending_count)}{Number(b.failed_count)>0?` · ${b.failed_count} failed with unpaid invoices`:""}{Number(b.unknown_count)>0?' · Bank result uncertain':''}</td></tr>)}</tbody></table>:<p>No supplier payment batches in this Company.</p>}{rows.length===200&&<p>Showing the 200 most recently updated batches.</p>}</section><SupplierBatchConsole base={base} data={data}/></div>;
}
