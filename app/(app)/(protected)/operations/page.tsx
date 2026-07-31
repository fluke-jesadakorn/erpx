import 'server-only';

import Link from 'next/link';
import { redirect } from 'next/navigation';
import type { ReactNode } from 'react';
import {
  Activity,
  AlertTriangle,
  ArrowRight,
  Building2,
  CheckCircle2,
  ClipboardCheck,
  Filter,
  Package,
  RefreshCw,
  ShoppingCart,
  Truck,
} from 'lucide-react';
import { Badge, type BadgeTone } from '@/components/ui/Badge';
import { Kpi } from '@/components/ui/Kpi';
import { Panel } from '@/components/ui/Panel';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { NoPermissionView } from '@/components/NoPermissionView';
import { FolioBridgeReadStatus } from '@/components/platform/FolioBridgeReadStatus';
import { loadActor } from '@/server/guard';
import { getSecondaryLocale } from '@/server/locale';
import { matchPerm } from '@/perm/grammar';
import {
  loadOperationsDashboard,
  type OperationsAttentionItem,
  type OperationsDashboard,
  type OperationsStage,
} from '@/operations/dashboard';
import { parseOperationsBranchParam } from '@/operations/scope';
import { T } from '@/components/i18n/TServer';
import { loadFolioBridgeReadStatus } from '@/platform/bridge';

export const dynamic = 'force-dynamic';

interface PageProps {
  searchParams: Promise<{ branch?: string; days?: string }>;
}

function money(value: number, compact = false): string {
  return new Intl.NumberFormat('en-US', {
    style: 'currency', currency: 'THB', currencyDisplay: 'narrowSymbol',
    notation: compact ? 'compact' : 'standard', maximumFractionDigits: compact ? 1 : 0,
  }).format(value);
}

function labelStatus(value: string): string {
  return value.replace(/^so_/, '').replaceAll('_', ' ');
}

function statusTone(value: string): BadgeTone {
  if (['so_paid', 'disbursed', 'posted', 'completed'].includes(value)) return 'positive';
  if (['rejected', 'failed', 'reversed'].includes(value)) return 'critical';
  if (['so_invoiced', 'awaiting_disbursement', 'counted'].includes(value)) return 'caution';
  if (['draft', 'so_draft'].includes(value)) return 'neutral';
  return 'info';
}

export default async function OperationsPage({ searchParams }: PageProps) {
  const actor = await loadActor();
  if (!actor) redirect('/login');
  const locale = await getSecondaryLocale();
  const canView = matchPerm(actor.permissions, 'tile:operations:view::allow')
    || matchPerm(actor.permissions, 'finance:core_operations:view::allow')
    || matchPerm(actor.permissions, 'admin:system:bypass::allow');
  if (!canView) {
    return <>
      <BreadcrumbSetter crumbs={[{ label: <T id="breadcrumbs.home" locale={locale} />, href: '/' }, { label: 'Operations' }]} />
      <PageLayout title="Operations" subtitle="Sales, procurement, inventory, and fulfillment command center">
        <NoPermissionView kind="locked" actor={actor as any} attemptedPath="/operations" reason="Operations dashboard permission is required." />
      </PageLayout>
    </>;
  }

  const params = await searchParams;
  const branchScope = parseOperationsBranchParam(params.branch);
  const branchId = branchScope.invalid ? -1 : branchScope.branchId;
  const requestedDays = Number(params.days ?? 30);
  const activityDays: 7 | 30 | 90 = requestedDays === 7 || requestedDays === 90 ? requestedDays : 30;
  const bridgeStatus = await loadFolioBridgeReadStatus();
  let dashboard: OperationsDashboard;
  try {
    dashboard = await loadOperationsDashboard({ actor, branchId, activityDays });
  } catch (error) {
    console.error('Operations dashboard unavailable:', error);
    return <>
      <BreadcrumbSetter crumbs={[{ label: <T id="breadcrumbs.home" locale={locale} />, href: '/' }, { label: 'Operations' }]} />
      <PageLayout title="Operations" subtitle="Sales, procurement, inventory, and fulfillment command center">
          <Panel tone="elevated" padding="lg" className="border-caution/35">
            <div className="flex items-start gap-4">
              <AlertTriangle className="mt-0.5 shrink-0 text-caution" size={22} />
              <div><h2 className="text-lg font-semibold text-ink">Operations data is unavailable</h2><p className="mt-2 text-sm leading-6 text-ink-2">{error instanceof Error && error.message === 'requested operations branch scope is unavailable' ? 'The requested branch is not an active branch in the authenticated operations scope.' : 'Apply the operational-accounting migration and verify PostgreSQL connectivity.'} No operational records were changed.</p></div>
            </div>
          </Panel>
      </PageLayout>
    </>;
  }

  const selectedBranch = dashboard.branches.find((branch) => branch.id === branchId);
  const suffix = `days=${activityDays}${branchId ? `&branch=${branchId}` : ''}`;
  return <>
    <BreadcrumbSetter crumbs={[{ label: <T id="breadcrumbs.home" locale={locale} />, href: '/' }, { label: 'Operations', href: '/operations' }]} />
    <PageLayout
      width="wide"
      title="Operations"
      subtitle="Order flow, purchasing, inventory availability, fulfillment, and operational exceptions"
      category={{ label: 'Operations', icon: 'Truck', href: '/operations' }}
      actions={<Link href={`/operations?${suffix}`} className="glass-chip inline-flex items-center gap-2 px-3 py-2 text-xs text-ink-2"><RefreshCw size={13} /> Refresh</Link>}
      contentClassName="space-y-5"
    >
      <FolioBridgeReadStatus status={bridgeStatus} />
      <section className="panel-elevated relative isolate overflow-hidden px-5 py-6 sm:px-7">
        <div aria-hidden className="absolute -right-28 -top-28 -z-10 h-72 w-72 rounded-full bg-info/15 blur-3xl" />
        <div aria-hidden className="absolute -bottom-32 left-1/3 -z-10 h-72 w-72 rounded-full bg-accent/10 blur-3xl" />
        <div className="flex flex-wrap items-start justify-between gap-6">
          <div className="max-w-3xl">
            <Badge tone="info" size="sm"><Activity size={11} /> Live operating picture</Badge>
            <h1 className="mt-4 text-3xl font-semibold tracking-[-0.045em] text-ink sm:text-4xl">Move demand into cash—without losing control.</h1>
            <p className="mt-3 max-w-2xl text-sm leading-6 text-ink-2">One command center for sales demand, purchase execution, stock availability, approval queues, and accounting-linked movements.</p>
          </div>
          <div className="grid min-w-[16rem] gap-2 text-xs">
            <Scope icon={<Building2 size={14} />} label="Branch scope" value={selectedBranch ? `${selectedBranch.code} · ${selectedBranch.name}` : 'All branches'} />
            <Scope icon={<Activity size={14} />} label="Movement window" value={`Last ${activityDays} days`} />
            <Scope icon={<CheckCircle2 size={14} />} label="Source of truth" value={dashboard.readPath === 'erpx_projected' ? 'ERPX Operations projected read model' : 'Folio read-only break-glass'} tone="text-positive" />
          </div>
        </div>
        <div className="mt-6 flex flex-wrap gap-2">
          <QuickLink href="/sales?scope=all" icon={<ShoppingCart size={14} />} label="Sales orders" />
          <QuickLink href="/pr" icon={<ClipboardCheck size={14} />} label="Purchase requests" />
          <QuickLink href="/po" icon={<Truck size={14} />} label="Purchase orders" />
          <QuickLink href="/inventory" icon={<Package size={14} />} label="Inventory control" />
          <QuickLink href="/reports?report=inventory" icon={<Activity size={14} />} label="Operations reports" />
        </div>
      </section>

      <form className="panel grid gap-3 p-4 sm:grid-cols-[1.3fr_1fr_auto] sm:items-end">
        <label className="text-xs font-medium text-ink-2"><span className="flex items-center gap-1.5"><Building2 size={13} /> Branch</span><select className="field mt-1.5" name="branch" defaultValue={branchId ?? ''}><option value="">All branches</option>{dashboard.branches.map((branch) => <option key={branch.id} value={branch.id}>{branch.code} · {branch.name}</option>)}</select></label>
        <label className="text-xs font-medium text-ink-2"><span className="flex items-center gap-1.5"><Activity size={13} /> Activity window</span><select className="field mt-1.5" name="days" defaultValue={activityDays}><option value="7">Last 7 days</option><option value="30">Last 30 days</option><option value="90">Last 90 days</option></select></label>
        <button className="action-button inline-flex h-10 items-center justify-center gap-2"><Filter size={14} /> Apply scope</button>
      </form>

      {dashboard.readPath === 'erpx_projected' ? <ProjectedOperationsSurface dashboard={dashboard} /> : <>
      <section className="grid grid-cols-2 gap-3 lg:grid-cols-5">
        <Kpi label="Open sales" value={dashboard.metrics.openSalesOrders} caption={money(dashboard.metrics.openSalesValueThb, true)} tone={dashboard.metrics.overdueSalesOrders ? 'caution' : 'info'} size="sm" />
        <Kpi label="Purchase requests" value={dashboard.metrics.openPurchaseRequests} caption="awaiting completion" tone="accent" size="sm" />
        <Kpi label="Open purchase orders" value={dashboard.metrics.openPurchaseOrders} caption={money(dashboard.metrics.openPurchaseValueThb, true)} tone="info" size="sm" />
        <Kpi label="Inventory value" value={money(dashboard.metrics.inventoryValueThb, true)} caption={`${dashboard.metrics.activeProducts} active products`} tone="positive" size="sm" />
        <Kpi label="Operational workflows" value={dashboard.metrics.openWaybills} caption={`${dashboard.metrics.lowStockItems} low-stock items`} tone={dashboard.metrics.lowStockItems ? 'caution' : 'neutral'} size="sm" />
      </section>

      <div className="grid gap-5 xl:grid-cols-2">
        <Pipeline title="Lead to cash" subtitle="Sales review, credit, invoice, and collection" icon={<ShoppingCart size={17} />} stages={dashboard.salesStages} />
        <Pipeline title="Procure to pay" subtitle="Request, approval, order, receipt, and settlement" icon={<Truck size={17} />} stages={dashboard.procurementStages} />
      </div>

      <div className="grid gap-5 xl:grid-cols-[0.7fr_1.3fr]">
        <Panel padding="none" className="overflow-hidden">
          <div className="border-b border-rule px-5 py-4"><div className="flex items-center gap-2"><AlertTriangle size={16} className="text-caution" /><h2 className="font-semibold text-ink">Operational attention</h2></div><p className="mt-1 text-xs text-mute">Exceptions and queues requiring follow-up</p></div>
          <div className="divide-y divide-rule/70">{dashboard.attention.map((item) => <AttentionRow key={item.key} item={item} />)}</div>
        </Panel>

        <Panel padding="none" className="overflow-hidden">
          <div className="flex items-center justify-between border-b border-rule px-5 py-4"><div><h2 className="font-semibold text-ink">Recent sales orders</h2><p className="mt-1 text-xs text-mute">Latest demand activity in the selected scope</p></div><Link href="/sales?scope=all" className="text-xs text-info hover:underline">Open sales <ArrowRight size={12} className="inline" /></Link></div>
          <div className="overflow-x-auto"><table className="w-full min-w-[680px] text-sm"><thead className="bg-paper-3/35 text-left text-[10px] uppercase tracking-[0.12em] text-mute"><tr><th className="px-4 py-3">Order</th><th className="px-3 py-3">Customer</th><th className="px-3 py-3">Status</th><th className="px-3 py-3">Due</th><th className="px-4 py-3 text-right">THB equivalent</th></tr></thead><tbody className="divide-y divide-rule/70">{dashboard.recentSales.map((row) => <tr key={row.id}><td className="px-4 py-3"><Link className="font-mono text-xs font-semibold text-info hover:underline" href={`/sales/by-so/${row.id}`}>{row.number}</Link><div className="mt-1 text-[11px] text-mute">{row.branch}</div></td><td className="px-3 py-3 text-ink-2">{row.customer}</td><td className="px-3 py-3"><Badge tone={statusTone(row.status)} size="sm">{labelStatus(row.status)}</Badge></td><td className={`px-3 py-3 text-xs ${row.dueDate && row.dueDate < new Date().toISOString().slice(0, 10) && !['so_paid', 'rejected'].includes(row.status) ? 'text-critical' : 'text-mute'}`}>{row.dueDate ?? '—'}</td><td className="px-4 py-3 text-right font-mono text-xs">{money(row.amountThb)}</td></tr>)}{!dashboard.recentSales.length && <tr><td colSpan={5} className="px-4 py-10 text-center text-sm text-mute">No sales orders in this scope.</td></tr>}</tbody></table></div>
        </Panel>
      </div>

      <div className="grid gap-5 xl:grid-cols-2">
        <Panel padding="none" className="overflow-hidden"><SectionHead title="Purchasing activity" subtitle="Recent purchase requests and orders" href="/pr" /><div className="divide-y divide-rule/70">{dashboard.recentPurchasing.map((row) => <Link key={`${row.kind}-${row.id}`} href={row.kind === 'pr' ? `/waybill/by-pr/${row.id}` : `/waybill/by-po/${row.id}`} className="flex items-center gap-3 px-4 py-3 transition hover:bg-paper-3/35"><span className={`grid h-8 w-8 shrink-0 place-items-center rounded-lg ${row.kind === 'pr' ? 'bg-accent-soft text-accent' : 'bg-info-soft text-info'}`}>{row.kind.toUpperCase()}</span><span className="min-w-0 flex-1"><span className="block font-mono text-xs font-semibold text-ink">{row.number}</span><span className="mt-1 block truncate text-xs text-mute">{row.party}</span></span><Badge tone={statusTone(row.status)} size="sm">{labelStatus(row.status)}</Badge><span className="w-24 text-right font-mono text-xs text-ink-2">{money(row.amountThb)}</span></Link>)}{!dashboard.recentPurchasing.length && <EmptyRow text="No purchasing activity." />}</div></Panel>
        <Panel padding="none" className="overflow-hidden"><SectionHead title="Posted stock movements" subtitle={`Receipts, shipments, transfers, and adjustments · ${activityDays} days`} href="/inventory" /><div className="divide-y divide-rule/70">{dashboard.recentMovements.map((row) => <div key={row.id} className="flex items-center gap-3 px-4 py-3"><span className="grid h-8 w-8 shrink-0 place-items-center rounded-lg bg-positive-soft text-positive"><Package size={14} /></span><span className="min-w-0 flex-1"><span className="block font-mono text-xs font-semibold text-ink">{row.number}</span><span className="mt-1 block truncate text-xs text-mute">{row.branch} · {row.source}</span></span><Badge tone="positive" size="sm">{labelStatus(row.kind)}</Badge><span className="text-right"><span className="block font-mono text-xs text-ink-2">{money(row.valueThb)}</span><span className="mt-1 block text-[11px] text-mute">{row.date}</span></span></div>)}{!dashboard.recentMovements.length && <EmptyRow text={`No posted movements in the last ${activityDays} days.`} />}</div></Panel>
      </div>

      <div className="flex flex-wrap items-center justify-between gap-2 text-[11px] text-mute"><span>Open metrics use current workflow state; inventory value uses moving weighted-average cost.</span><span>Generated {new Date(dashboard.generatedAt).toLocaleString('en-GB')}</span></div>
      </>}
    </PageLayout>
  </>;
}

function ProjectedOperationsSurface({ dashboard }: { dashboard: OperationsDashboard }) {
  const summary = dashboard.projectedSummary;
  return <div className="space-y-5">
    <section className="grid grid-cols-2 gap-3 lg:grid-cols-4">
      <Kpi label="Projected rows" value={summary?.readRowCount ?? 0} caption="ERPX tenant read model" tone="info" size="sm" />
      <Kpi label="Active exceptions" value={summary?.exceptionCount ?? 0} caption="Operations exception projection" tone={summary?.exceptionCount ? 'caution' : 'positive'} size="sm" />
      <Kpi label="Projection checkpoints" value={summary?.projectionCheckpointCount ?? 0} caption="Durable read progress" tone="accent" size="sm" />
      <Kpi label="Source domains" value={dashboard.projectedSourceCounts.length} caption="Immutable provenance groups" tone="positive" size="sm" />
    </section>

    <Panel padding="none" className="overflow-hidden">
      <div className="border-b border-rule px-5 py-4">
        <div className="flex items-center gap-2"><CheckCircle2 size={16} className="text-positive" /><h2 className="font-semibold text-ink">Projected exception queue</h2></div>
        <p className="mt-1 text-xs text-mute">Read-only ERPX projection · {summary?.holdingRef ?? 'holding unavailable'} · last projected {summary?.latestProjectedAt ? new Date(summary.latestProjectedAt).toLocaleString('en-GB') : '—'}</p>
      </div>
      <div className="overflow-x-auto"><table className="w-full min-w-[860px] text-sm"><thead className="bg-paper-3/35 text-left text-[10px] uppercase tracking-[0.12em] text-mute"><tr><th className="px-4 py-3">Exception</th><th className="px-3 py-3">Source</th><th className="px-3 py-3">Severity</th><th className="px-3 py-3">Status</th><th className="px-3 py-3">Assignee</th><th className="px-4 py-3 text-right">Updated</th></tr></thead><tbody className="divide-y divide-rule/70">{dashboard.projectedExceptions.map((row) => <tr key={row.id}><td className="px-4 py-3"><span className="font-medium text-ink">{row.category}</span><span className="mt-1 block font-mono text-[11px] text-mute">v{row.aggregateVersion} · {row.id}</span></td><td className="px-3 py-3"><span className="text-ink-2">{row.sourceDomain}</span><span className="mt-1 block text-[11px] text-mute">{row.sourceType}</span></td><td className="px-3 py-3"><Badge tone={row.severity === 'critical' || row.severity === 'high' ? 'critical' : row.severity === 'medium' ? 'caution' : 'info'} size="sm">{row.severity}</Badge></td><td className="px-3 py-3"><Badge tone={['resolved', 'closed', 'Resolved', 'Closed'].includes(row.status) ? 'positive' : 'info'} size="sm">{row.status}</Badge></td><td className="px-3 py-3 text-xs text-ink-2">{row.assignedTo ?? 'Unassigned'}</td><td className="px-4 py-3 text-right text-xs text-mute">{new Date(row.updatedAt).toLocaleString('en-GB')}</td></tr>)}{!dashboard.projectedExceptions.length && <tr><td colSpan={6} className="px-4 py-10 text-center text-sm text-mute">No projected exceptions in this scope.</td></tr>}</tbody></table></div>
    </Panel>

    <Panel padding="none" className="overflow-hidden">
      <div className="border-b border-rule px-5 py-4"><h2 className="font-semibold text-ink">Source-domain coverage</h2><p className="mt-1 text-xs text-mute">Counts are derived from projected immutable source references; source transactions remain owned by their domains.</p></div>
      <div className="grid gap-2 p-4 sm:grid-cols-2 lg:grid-cols-4">{dashboard.projectedSourceCounts.map((source) => <div key={source.sourceDomain} className="rounded-xl border border-rule bg-paper/55 px-4 py-3"><span className="block text-xs font-medium text-ink-2">{source.sourceDomain}</span><span className="mt-1 block font-mono text-xl font-semibold text-ink">{source.count}</span></div>)}{!dashboard.projectedSourceCounts.length && <p className="px-1 py-3 text-sm text-mute">No source references have been projected for this holding.</p>}</div>
    </Panel>
  </div>;
}

function Scope({ icon, label, value, tone = 'text-ink' }: { icon: ReactNode; label: string; value: string; tone?: string }) {
  return <div className="flex items-center gap-3 rounded-xl border border-rule bg-paper/55 px-3 py-2.5"><span className="grid h-8 w-8 shrink-0 place-items-center rounded-lg bg-paper-2 text-accent">{icon}</span><span><span className="block text-[10px] font-semibold uppercase tracking-[0.12em] text-mute">{label}</span><span className={`mt-0.5 block font-medium ${tone}`}>{value}</span></span></div>;
}

function QuickLink({ href, icon, label }: { href: string; icon: ReactNode; label: string }) {
  return <Link href={href} className="glass-chip inline-flex items-center gap-2 border border-rule px-3 py-2 text-xs font-medium text-ink-2 transition hover:border-info/40 hover:text-info">{icon}{label}<ArrowRight size={12} /></Link>;
}

function Pipeline({ title, subtitle, icon, stages }: { title: string; subtitle: string; icon: ReactNode; stages: OperationsStage[] }) {
  const active = stages.reduce((sum, item, index) => index === stages.length - 1 ? sum : sum + item.count, 0);
  return <Panel padding="none" className="overflow-hidden"><div className="flex items-start justify-between gap-3 border-b border-rule px-5 py-4"><div><div className="flex items-center gap-2 text-ink">{icon}<h2 className="font-semibold">{title}</h2></div><p className="mt-1 text-xs text-mute">{subtitle}</p></div><Badge tone={active ? 'info' : 'positive'} size="sm">{active} active</Badge></div><div className="grid grid-cols-2 gap-px bg-rule sm:grid-cols-4">{stages.map((item, index) => <Link key={item.key} href={item.href} className="group bg-paper px-4 py-4 transition hover:bg-paper-3"><div className="flex items-center justify-between gap-2"><span className="text-[10px] font-semibold uppercase tracking-[0.12em] text-mute">{index + 1}</span>{index < stages.length - 1 && <ArrowRight size={12} className="text-mute transition group-hover:translate-x-0.5 group-hover:text-info" />}</div><div className="mt-3 font-mono text-2xl font-semibold text-ink">{item.count}</div><div className="mt-1 text-xs font-medium text-ink-2">{item.label}</div><div className="mt-2 font-mono text-[11px] text-mute">{money(item.amountThb, true)}</div></Link>)}</div></Panel>;
}

function AttentionRow({ item }: { item: OperationsAttentionItem }) {
  return <Link href={item.href} className="flex items-center gap-3 px-4 py-3 transition hover:bg-paper-3/35"><span className={`grid h-8 w-8 shrink-0 place-items-center rounded-lg font-mono text-sm font-semibold ${item.tone === 'critical' ? 'bg-critical-soft text-critical' : item.tone === 'caution' ? 'bg-caution-soft text-caution' : item.tone === 'info' ? 'bg-info-soft text-info' : 'bg-paper-3 text-mute'}`}>{item.count}</span><span className="min-w-0 flex-1"><span className="block text-sm font-medium text-ink">{item.label}</span><span className="mt-0.5 block truncate text-xs text-mute">{item.detail}</span></span><ArrowRight size={13} className="text-mute" /></Link>;
}

function SectionHead({ title, subtitle, href }: { title: string; subtitle: string; href: string }) {
  return <div className="flex items-center justify-between gap-3 border-b border-rule px-5 py-4"><div><h2 className="font-semibold text-ink">{title}</h2><p className="mt-1 text-xs text-mute">{subtitle}</p></div><Link href={href} className="text-xs text-info hover:underline">Open <ArrowRight size={12} className="inline" /></Link></div>;
}

function EmptyRow({ text }: { text: string }) {
  return <div className="px-4 py-10 text-center text-sm text-mute">{text}</div>;
}
