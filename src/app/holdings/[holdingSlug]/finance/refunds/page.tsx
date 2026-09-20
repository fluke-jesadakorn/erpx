import Link from 'next/link';
import { requireWorkContext,publicScope } from '@/server/authorization';
import { workspaceBase } from '@/lib/workspace';
import { listRefunds } from '@/server/refunds';
import { minorToDecimal } from '@/lib/finance-money';
export const dynamic='force-dynamic';
export default async function RefundsPage({params}:{params:Promise<{holdingSlug:string}>}) {
 const actor=await requireWorkContext((await params).holdingSlug),base=workspaceBase(publicScope(actor)),rows=await listRefunds(actor);
 return <div className="content-stack"><Link href={`${base}/finance`}>← Finance</Link><h1>Return refunds</h1><p>Posted returns create a separate obligation for money already paid. Review the beneficiary, prepare instructions, then confirm against bank evidence. Exporting does not move money.</p><section className="card"><div className="business-table-wrap"><table className="business-table"><thead><tr><th>Source return</th><th>Counterparty</th><th>Direction</th><th>Amount</th><th>Remaining</th><th>Status</th></tr></thead><tbody>{rows.map(row=><tr key={String(row.id)}><td><Link href={`${base}/finance/refunds/${row.id}`}>{String(row.return_number)}</Link></td><td>{String(row.party_name)}</td><td>{row.direction==='outgoing'?'Pay customer':'Receive from supplier'}</td><td>{String(row.currency)} {minorToDecimal(Number(row.amount_minor))}</td><td>{minorToDecimal(Number(row.outstanding_minor))}</td><td>{String(row.status)}</td></tr>)}</tbody></table></div>{!rows.length&&<p>No posted returns with a cash refund obligation in this workspace.</p>}{rows.length===200&&<p>Showing the 200 most recently updated obligations.</p>}</section></div>;
}
