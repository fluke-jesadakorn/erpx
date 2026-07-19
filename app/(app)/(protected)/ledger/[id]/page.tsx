import 'server-only';
import Link from 'next/link';
import { notFound, redirect } from 'next/navigation';
import { PageLayout } from '@/components/PageLayout';
import { loadActor } from '@/server/guard';
import { matchPerm } from '@/perm/grammar';
import { loadJournal } from '@/finance';

export const dynamic = 'force-dynamic';

export default async function JournalPage({ params }: { params: Promise<{ id: string }> }) {
  const actor = await loadActor();
  if (!actor) redirect('/login');
  if (!matchPerm(actor.permissions, 'finance:ledger:view::allow') && !matchPerm(actor.permissions, 'finance:gl:view::allow')) redirect('/');
  const id = Number((await params).id);
  if (!Number.isInteger(id)) notFound();
  const journal = await loadJournal(id).catch(() => null);
  if (!journal) notFound();
  const total = journal.lines.reduce((sum, line) => sum + Number(line.debitThb ?? 0), 0);
  const sourceHref = journal.waybillId ? `/waybill/${journal.waybillId}` : null;
  return (
    <PageLayout title={journal.journalNo ?? `Journal #${journal.id}`} subtitle={journal.description} category={{ label: 'Ledger', icon: 'BookOpen', href: '/ledger' }} width="wide" actions={<Link className="glass-chip" href="/ledger">Back to ledger</Link>}>
      <section className="panel-elevated p-5"><div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4"><div><div className="text-xs uppercase text-mute">Status</div><div className="mt-1 font-bold text-positive">{journal.status}</div></div><div><div className="text-xs uppercase text-mute">Posting date</div><div className="mt-1 font-mono">{journal.postingDate}</div></div><div><div className="text-xs uppercase text-mute">Source</div><div className="mt-1 font-mono text-sm">{journal.sourceType} · {journal.sourceId}</div></div><div><div className="text-xs uppercase text-mute">Total</div><div className="mt-1 font-mono font-bold">THB {total.toLocaleString('en-US', { minimumFractionDigits: 2 })}</div></div></div>{sourceHref && <div className="mt-4"><Link className="action-button" href={sourceHref}>Open source waybill</Link></div>}</section>
      <section className="panel-elevated mt-5 overflow-hidden"><div className="overflow-x-auto"><table className="w-full text-sm"><thead className="bg-paper-2 text-left text-xs uppercase text-mute"><tr><th className="px-4 py-3">Line</th><th className="px-4 py-3">Account</th><th className="px-4 py-3">Description</th><th className="px-4 py-3">Dimensions</th><th className="px-4 py-3 text-right">Debit</th><th className="px-4 py-3 text-right">Credit</th></tr></thead><tbody className="divide-y divide-rule">{journal.lines.map((line, index) => <tr key={index}><td className="px-4 py-3 font-mono text-mute">{index + 1}</td><td className="px-4 py-3 font-mono font-bold">{line.accountCode}</td><td className="px-4 py-3">{line.description}</td><td className="px-4 py-3 text-xs text-ink-2">Branch {line.branchId}{line.departmentId ? ` · Dept ${line.departmentId}` : ''}{line.customerId ? ` · Customer ${line.customerId}` : ''}{line.vendorId ? ` · Vendor ${line.vendorId}` : ''}{line.productId ? ` · Product ${line.productId}` : ''}{line.warehouseId ? ` · Warehouse ${line.warehouseId}` : ''}</td><td className="px-4 py-3 text-right font-mono">{Number(line.debitThb ?? 0).toFixed(2)}</td><td className="px-4 py-3 text-right font-mono">{Number(line.creditThb ?? 0).toFixed(2)}</td></tr>)}</tbody></table></div></section>
      <section className="panel-elevated mt-5 p-5"><h2 className="text-lg font-bold">Audit identity</h2><dl className="mt-3 grid gap-3 text-sm sm:grid-cols-2"><div><dt className="text-mute">Source event key</dt><dd className="break-all font-mono">{journal.sourceEventKey}</dd></div><div><dt className="text-mute">Preparer / approver</dt><dd className="font-mono">{journal.preparerId ?? '—'} / {journal.approverId ?? '—'}</dd></div>{journal.reversalOfId && <div><dt className="text-mute">Reversal of</dt><dd><Link className="text-accent" href={`/ledger/${journal.reversalOfId}`}>Journal #{journal.reversalOfId}</Link></dd></div>}</dl></section>
    </PageLayout>
  );
}
