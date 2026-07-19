import { Suspense } from 'react';
import { headers } from 'next/headers';
import Link from 'next/link';
import { redirect, notFound } from 'next/navigation';
import {
  loadWaybillRailContext,
  loadApproversByStage,
  loadActedUsersByStage,
  loadSalesArtifacts,
  loadWaybillEvents,
} from '@/waybill/queries';
import { loadActor } from '@/server/guard';
import { loadVisionModels } from '@/ai/loadVisionModels';
import { hasPermission, loadActivePermSession, type PermSession } from '@folio-lib/perm/server';
import { PERM } from '@folio-lib/perm/taxonomy';
import { STAGE_TO_PERM, stageRoles } from '@/perm';
import { getSecondaryLocale } from '@/server/locale';
import { pipsForDomain, pipIndex, domainForOrigin } from '@/waybill/derive';
import { verifyEventChain } from '@/waybill/events';
import { WaybillStepCards } from '@/components/waybill/WaybillStepCards';
import { WaybillTimelineBigPicture } from '@/components/waybill/WaybillTimelineBigPicture';
import { WaybillAuditSection } from '@/components/waybill/WaybillAuditSection';
import { WaybillHeader } from '@/components/waybill/WaybillHeader';
import { InlineActionForm } from '@/components/waybill/InlineActionForm';
import { ExportPdfButton } from '@/components/waybill/ExportPdfButton';
import { DecisionBar } from '@/components/waybill/DecisionBar';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { NoPermissionView } from '@/components/NoPermissionView';
import { SalesPipPanel } from './_components/SalesPipPanel';
import { SalesExtractPanel, type SoItemRow } from '@/components/waybill/SalesExtractPanel';
import { query } from '@/db';
import {
  allocateSalesReceiptAction,
  issueSalesInvoiceAction,
  mapSalesProductAction,
  refundSalesCreditAction,
  rejectSalesOrderAction,
  reserveSalesStockAction,
  returnSalesStockAction,
  shipSalesStockAction,
} from './_actions';

export const dynamic = 'force-dynamic';

interface PageProps {
  params: Promise<{ id: string }>;
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}

function asString(v: string | string[] | undefined): string | null {
  if (Array.isArray(v)) return v[0] ?? null;
  return v ?? null;
}

function canActOnSalesStage(session: PermSession, roleName: string, stage: string): boolean {
  if (hasPermission(session, PERM.admin.system.bypass)) return true;
  const stagePerm = STAGE_TO_PERM[stage];
  if (stagePerm && hasPermission(session, stagePerm)) return true;
  const roles = stageRoles(stage);
  return roles.includes(roleName);
}

export default async function SalesDetailPage({ params, searchParams }: PageProps) {
  const { id } = await params;
  const sp = await searchParams;

  const h = await headers();
  const permOut = await loadActivePermSession(
    new Request('http://internal', { headers: h as unknown as HeadersInit }),
  );
  if (!permOut || !hasPermission(permOut.session, PERM.tile.sales.view)) {
    return (
      <>
        <BreadcrumbSetter crumbs={[{ label: 'Hub', href: '/' }, { label: 'Sales', href: '/sales' }, { label: id, href: `/sales/${id}` }]} />
        <PageLayout title={id} subtitle={permOut?.session.user.name ?? undefined}>
          <NoPermissionView
            kind="locked"
            actor={permOut ? (permOut.session.user as any) : null}
            attemptedPath={`/sales/${id}`}
            reason={permOut ? 'tile:sales:view required.' : 'Sign in to view this page.'}
          />
        </PageLayout>
      </>
    );
  }

  const actor = await loadActor();
  if (!actor) redirect('/login');

  const ctx = await loadWaybillRailContext(id);
  if (!ctx || ctx.waybill.origin !== 'so') notFound();

  const wb = ctx.waybill;
  const action = asString(sp.action);

  const salesArtifacts = await loadSalesArtifacts(wb);

  const [approversByStage, actedUsersByStage, visionModels, locale, events, integrity, soItemsRes, arSlipRes, productsRes, warehousesRes, officialRes, arRes, creditRes] = await Promise.all([
    loadApproversByStage(wb.id),
    loadActedUsersByStage(wb.id),
    loadVisionModels(actor.id),
    getSecondaryLocale(),
    loadWaybillEvents(wb.id),
    verifyEventChain(wb.id),
    query<SoItemRow & { product_id: number | null; reserved_qty: number; shipped_qty: number; invoiced_qty: number; returned_qty: number }>(
      `SELECT id, description, qty::float AS qty, unit_price::float AS unit_price,
              vat_amount::float AS vat_amount, line_total::float AS line_total,
              mapped_revenue_account_code, confidence_score::float AS confidence_score,
              product_id, reserved_qty::float, shipped_qty::float, invoiced_qty::float, returned_qty::float
         FROM so_items
        WHERE sales_order_id = $1
        ORDER BY id ASC`,
      [wb.origin_id],
    ),
    query<{ ar_slip_id: number | null }>(
      `SELECT ar_slip_id FROM sales_orders WHERE id = $1`,
      [wb.origin_id],
    ),
    query<{ id: string; sku: string; name: string }>(`SELECT id::text, sku, name FROM inventory.products WHERE active ORDER BY sku`),
    query<{ id: string; code: string; name: string }>(`SELECT id::text, code, name FROM inventory.warehouses WHERE active ORDER BY code`),
    query<{ id: string; journal_no: string; status: string; posting_date: string; description: string; source_type: string; total: string }>(`SELECT j.id::text, j.journal_no, j.status, j.posting_date::text, j.description, j.source_type, sum(l.debit_thb)::text AS total FROM finance.journals j JOIN finance.journal_lines l ON l.journal_id = j.id WHERE (j.source_type IN ('sales_invoice','ar_receipt','sales_credit_note') AND j.source_id = $1) OR (j.source_type = 'ar_refund' AND j.source_id IN (SELECT d.id::text FROM finance.ar_documents d JOIN finance.commercial_documents c ON c.id = d.document_id WHERE c.source_type = 'sales_return' AND c.source_id LIKE $1 || ':%')) OR (j.source_type = 'inventory_movement' AND j.metadata->>'businessSourceType' IN ('sales_order_line','sales_return_line') AND j.metadata->>'businessSourceId' IN (SELECT id::text FROM so_items WHERE sales_order_id = $2)) GROUP BY j.id ORDER BY j.posting_date, j.id`, [String(wb.origin_id), wb.origin_id]),
    query<{ id: string; document_no: string; currency_code: string; open_foreign: string; open_thb: string; status: string }>(`SELECT d.id::text, d.document_no, d.currency_code, d.open_foreign::text, d.open_thb::text, d.status FROM finance.ar_documents d JOIN finance.commercial_documents c ON c.id = d.document_id WHERE d.document_type = 'invoice' AND c.source_type = 'sales_order' AND c.source_id = $1 ORDER BY d.id DESC LIMIT 1`, [String(wb.origin_id)]),
    query<{ id: string; document_no: string; currency_code: string; open_foreign: string; open_thb: string; status: string }>(`SELECT d.id::text, d.document_no, d.currency_code, d.open_foreign::text, d.open_thb::text, d.status FROM finance.ar_documents d JOIN finance.commercial_documents c ON c.id = d.document_id WHERE d.document_type = 'credit_note' AND c.source_type = 'sales_return' AND c.source_id LIKE $1 || ':%' AND d.status IN ('open','partially_paid') ORDER BY d.id DESC LIMIT 1`, [String(wb.origin_id)]),
  ]);

  const soItems = soItemsRes.rows.map((r) => ({
    ...r,
    qty: Number(r.qty),
    unit_price: Number(r.unit_price),
    vat_amount: Number(r.vat_amount),
    line_total: Number(r.line_total),
    confidence_score: r.confidence_score == null ? null : Number(r.confidence_score),
  }));
  const existingArSlipId = arSlipRes.rows[0]?.ar_slip_id ?? null;

  const actorRole = actor.role_name;
  const session = permOut.session;

  const canPostSalesGlVat = hasPermission(session, 'finance:gl:post::allow');
  const canPostSalesGlSettlement = hasPermission(session, 'finance:gl:post::allow');
  const canConfirmSalesGl = hasPermission(session, 'finance:gl:confirm::allow');
  const canSettle = hasPermission(session, 'finance:sales:settle::allow');
  const canOperateInventory = hasPermission(session, 'inventory:stock:ship::allow')
    && hasPermission(session, 'finance:journal:prepare::allow')
    && hasPermission(session, 'finance:journal:approve::allow');
  const canAllocateReceipt = hasPermission(session, 'finance:journal:prepare::allow')
    && hasPermission(session, 'finance:journal:approve::allow');
  const invoiceable = soItems.some((item) => ((item.product_id ? item.shipped_qty : item.qty) - item.invoiced_qty) > 0);
  const canIssueInvoice = invoiceable
    && hasPermission(session, 'finance:sales:invoice::allow')
    && hasPermission(session, 'finance:journal:prepare::allow')
    && hasPermission(session, 'finance:journal:approve::allow');

  const canRecordSalesPayment =
    hasPermission(session, PERM.admin.system.bypass) ||
    hasPermission(session, 'finance:sales:settle::allow') ||
    ['finance', 'account_officer', 'account_supervisor', 'accounting_manager', 'cfo', 'ceo'].includes(actorRole);

  const rejectionEvent = ctx.events.find((e) => e.kind === 'so-rejected') ?? null;
  const rejectionReason = (rejectionEvent?.payload as { reason?: string } | null)?.reason ?? null;

  const pipsAll = pipsForDomain(domainForOrigin(wb.origin));
  const curIdxAll = pipIndex(domainForOrigin(wb.origin), wb.current_stage);
  const stepsDone = curIdxAll < 0 ? 0 : curIdxAll;
  const stepsTotal = pipsAll.filter((p) => p.key !== 'rejected').length;
  const progressPct = wb.status === 'completed' ? 100
    : stepsTotal > 0 ? Math.round((stepsDone / stepsTotal) * 100) : 0;
  const statusTone = wb.status === 'completed' ? { ring: 'from-positive/50 to-info/40', chip: 'bg-positive text-positive border-positive/40', dot: 'bg-positive', label: 'Completed' }
    : wb.status === 'rejected' ? { ring: 'from-critical/60 to-critical/20', chip: 'bg-critical text-critical border-critical/40', dot: 'bg-critical', label: 'Rejected' }
      : { ring: 'from-info/60 to-accent/40', chip: 'bg-info text-info border-info/40', dot: 'bg-info animate-pulse', label: 'In progress' };

  const canAct = canActOnSalesStage(session, actorRole, wb.current_stage);

  return (
    <>
      <BreadcrumbSetter
        crumbs={[
          { label: 'Hub', href: '/' },
          { label: 'Sales', href: '/sales' },
          { label: wb.id, href: `/waybill/${wb.id}` },
        ]}
      />
      <PageLayout
        title={wb.id}
        subtitle={`SO · ${wb.vendor_name ?? '—'}`}
        actions={
          <ExportPdfButton
            waybillId={wb.id}
            attachments={ctx.attachments}
            defaultSections={{ cover: true, rail: true, audit: true, attachments: true }}
          />
        }
      >
        <section className="space-y-6 font-sans">
          <WaybillHeader
            wb={wb}
            originLabel={`SO-${wb.origin_id}`}
            originHref="/sales"
            actor={actor}
            isRejected={wb.status === 'rejected'}
            statusTone={statusTone}
            stepsDone={stepsDone}
            stepsTotal={stepsTotal}
            progressPct={progressPct}
            activeActorName={ctx.activeActorName}
          />

          <DecisionBar
            waybillId={wb.id}
            currentStage={wb.current_stage}
            status={wb.status}
            amount={wb.total_amount ?? null}
            vendorName={wb.vendor_name ?? null}
            canAct={canAct}
            canFinalApprove={false}
            canSettle={canSettle}
            canConfirmGl={canConfirmSalesGl}
            isFinalApproval={wb.current_stage === 'so_invoiced'}
            isAwaitingDisbursement={wb.current_stage === 'so_invoiced'}
            isDisbursed={wb.current_stage === 'so_paid'}
            isRejected={wb.status === 'rejected'}
            actorRole={actor.role_name ?? null}
          />

          {action === 'reject' && canAct && !rejectionReason && wb.status === 'open' && (
            <InlineActionForm kind="reject" waybillId={wb.id} stage={wb.current_stage} locale={locale} submitAction={rejectSalesOrderAction} />
          )}

          <WaybillTimelineBigPicture
            waybillId={wb.id}
            domain="sales"
            currentStage={wb.current_stage}
            status={wb.status as 'open' | 'completed' | 'rejected' | 'reversed' | 'superseded'}
            events={ctx.events}
            attachments={ctx.attachments}
            amountTHB={wb.total_amount ? Number(wb.total_amount) : null}
            locale={locale}
            canAct={canAct && wb.status !== 'rejected'}
            canAttach={false}
            canSettle={canSettle}
            canFinalApprove={false}
            originId={wb.origin_id}
            approversByStage={approversByStage}
            currentUserId={actor.id}
            visionModels={visionModels}
            canConfirmGl={canConfirmSalesGl}
            hasGlConfirmed={false}
          />

          <Suspense fallback={<div className="space-y-4">
            {Array.from({ length: 3 }).map((_, i) => (
              <div key={i} className="h-48 animate-pulse rounded-md border border-rule/60 bg-paper-2/50" aria-hidden />
            ))}
          </div>}>
            <WaybillStepCards
              wb={wb}
              waybillId={wb.id}
              currentStage={wb.current_stage}
              status={wb.status}
              events={ctx.events}
              attachments={ctx.attachments}
              approversByStage={approversByStage}
              actedUsersByStage={actedUsersByStage}
              expensePicture={null}
              hasGlConfirmed={false}
              artifacts={salesArtifacts as never}
              actorCanSeeGlLines={canConfirmSalesGl}
              originId={wb.origin_id}
              visionModels={visionModels}
              actions={{
                canAct,
                canAttach: false,
                canSettle,
                canFinalApprove: false,
                canConfirmGl: canConfirmSalesGl,
                canReCall: false,
                canSaveAccrual: false,
                canPostAccrual: canPostSalesGlVat,
                canConfirmAccrual: false,
                canPostSettlement: canPostSalesGlSettlement,
                canConfirmSettlement: canConfirmSalesGl,
              }}
              flags={{
                isSubmitter: actor.id === wb.submitter_id,
                isFinalApproval: wb.current_stage === 'so_invoiced',
                isDisbursed: wb.current_stage === 'so_paid',
              }}
              rejection={{ reason: rejectionReason, actor: null }}
              ui={{ action, actionStage: null, locale }}
            />
          </Suspense>

          {salesArtifacts ? (
            <SalesPipPanel
              waybillId={wb.id}
              artifacts={salesArtifacts}
              currentStage={wb.current_stage}
            />
          ) : null}

          <section className="panel-elevated overflow-hidden">
            <div className="border-b border-rule px-5 py-4"><div className="flex flex-wrap items-center justify-between gap-3"><div><h2 className="text-lg font-bold">Fulfillment and stock</h2><p className="text-sm text-ink-2">Map SKUs, reserve available stock, ship partially, and return against the original moving-average cost.</p></div><Link className="glass-chip px-3 py-1 text-sm" href="/inventory">Open inventory</Link></div></div>
            <div className="divide-y divide-rule">{soItems.map((item) => <div key={item.id} className="p-4"><div className="flex flex-wrap items-start justify-between gap-3"><div><div className="font-medium">{item.description}</div><div className="mt-1 font-mono text-xs text-mute">Ordered {item.qty} · Reserved {item.reserved_qty} · Shipped {item.shipped_qty} · Returned {item.returned_qty}</div></div>{hasPermission(session, 'inventory:stock:ship::allow') && <form action={mapSalesProductAction} className="flex gap-2"><input type="hidden" name="lineId" value={item.id} /><input type="hidden" name="salesOrderId" value={wb.origin_id} /><select className="rounded-md border border-rule bg-paper px-2 py-1 text-sm" name="productId" defaultValue={item.product_id ?? ''} required><option value="">Map product</option>{productsRes.rows.map((product) => <option key={product.id} value={product.id}>{product.sku} · {product.name}</option>)}</select><button className="glass-chip px-3 py-1 text-sm">Save</button></form>}</div>{item.product_id && canOperateInventory && <div className="mt-3 grid gap-3 lg:grid-cols-3"><form action={reserveSalesStockAction} className="rounded-md border border-rule bg-paper-2 p-3"><div className="text-xs font-bold uppercase text-mute">Reserve</div><input type="hidden" name="lineId" value={item.id} /><input type="hidden" name="salesOrderId" value={wb.origin_id} /><select className="field" name="warehouseId">{warehousesRes.rows.map((warehouse) => <option key={warehouse.id} value={warehouse.id}>{warehouse.code}</option>)}</select><input className="field" name="quantity" type="number" min="0.000001" max={item.qty - item.reserved_qty} step="0.000001" placeholder="Quantity" required /><input className="field" name="lotId" type="number" placeholder="Lot ID if required" /><button className="action-button mt-2">Reserve</button></form><form action={shipSalesStockAction} className="rounded-md border border-rule bg-paper-2 p-3"><div className="text-xs font-bold uppercase text-mute">Ship</div><input type="hidden" name="lineId" value={item.id} /><input type="hidden" name="requestKey" value={crypto.randomUUID()} /><select className="field" name="warehouseId">{warehousesRes.rows.map((warehouse) => <option key={warehouse.id} value={warehouse.id}>{warehouse.code}</option>)}</select><input className="field" name="movementDate" type="date" defaultValue={new Date().toISOString().slice(0, 10)} required /><input className="field" name="quantity" type="number" min="0.000001" max={item.qty - item.shipped_qty} step="0.000001" placeholder="Quantity" required /><input className="field" name="lotId" type="number" placeholder="Lot ID if required" /><button className="action-button mt-2">Ship & post COGS</button></form><form action={returnSalesStockAction} className="rounded-md border border-rule bg-paper-2 p-3"><div className="text-xs font-bold uppercase text-mute">Return</div><input type="hidden" name="lineId" value={item.id} /><input type="hidden" name="requestKey" value={crypto.randomUUID()} /><select className="field" name="warehouseId">{warehousesRes.rows.map((warehouse) => <option key={warehouse.id} value={warehouse.id}>{warehouse.code}</option>)}</select><input className="field" name="movementDate" type="date" defaultValue={new Date().toISOString().slice(0, 10)} required /><input className="field" name="quantity" type="number" min="0.000001" max={item.shipped_qty - item.returned_qty} step="0.000001" placeholder="Quantity" required /><input className="field" name="lotId" type="number" placeholder="Lot ID if required" /><button className="action-button mt-2">Return & reverse COGS</button></form></div>}</div>)}</div>
          </section>

          <section className="panel-elevated overflow-hidden"><div className="flex flex-wrap items-center justify-between gap-3 border-b border-rule px-5 py-4"><div><h2 className="text-lg font-bold">Accounting effect</h2><p className="text-sm text-ink-2">Official immutable journals generated by shipment, invoice, receipt, and return events.</p></div>{wb.current_stage === 'so_invoiced' && canIssueInvoice && <form action={issueSalesInvoiceAction}><input type="hidden" name="waybillId" value={wb.id} /><button className="action-button">Invoice newly fulfilled quantity</button></form>}</div><div className="divide-y divide-rule">{officialRes.rows.map((journal) => <Link key={journal.id} className="flex flex-wrap items-center justify-between gap-3 px-5 py-3 hover:bg-paper-2" href={`/ledger/${journal.id}`}><div><div className="font-mono font-bold text-accent">{journal.journal_no}</div><div className="text-sm text-ink">{journal.description}</div><div className="text-xs text-mute">{journal.posting_date} · {journal.source_type}</div></div><div className="text-right"><div className="font-mono">THB {Number(journal.total).toLocaleString('en-US', { minimumFractionDigits: 2 })}</div><div className="text-xs font-bold text-positive">{journal.status}</div></div></Link>)}{!officialRes.rows.length && <p className="p-5 text-sm text-mute">No official accounting effects yet. Draft proposals remain outside reports.</p>}</div>{arRes.rows[0] && canAllocateReceipt && ['open','partially_paid'].includes(arRes.rows[0].status) && <form action={allocateSalesReceiptAction} className="grid gap-3 border-t border-rule p-5 sm:grid-cols-2 lg:grid-cols-5"><input type="hidden" name="salesOrderId" value={wb.origin_id} /><input type="hidden" name="requestKey" value={crypto.randomUUID()} /><label className="text-sm">Receipt date<input className="field" name="allocationDate" type="date" defaultValue={new Date().toISOString().slice(0, 10)} required /></label><label className="text-sm">Amount {arRes.rows[0].currency_code}<input className="field" name="foreignAmount" type="number" min="0.01" max={arRes.rows[0].open_foreign} step="0.01" required /></label><label className="text-sm">WHT THB<input className="field" name="whtAmountThb" type="number" min="0" step="0.01" defaultValue="0" /></label><label className="text-sm">FX to THB<input className="field" name="fxRate" type="number" min="0.0000000001" step="0.0000000001" defaultValue={arRes.rows[0].currency_code.trim() === 'THB' ? '1' : ''} /></label><button className="action-button self-end">Allocate partial receipt</button><div className="sm:col-span-2 lg:col-span-5 text-xs text-mute">{arRes.rows[0].document_no} · Open {Number(arRes.rows[0].open_foreign).toLocaleString()} {arRes.rows[0].currency_code} / THB {Number(arRes.rows[0].open_thb).toLocaleString()}</div></form>}</section>

          {creditRes.rows[0] && canAllocateReceipt && (
            <section className="panel-elevated p-5">
              <h2 className="text-lg font-bold">Customer refund</h2>
              <p className="mt-1 text-sm text-ink-2">Settle the open credit note through bank and recognize realized FX.</p>
              <form action={refundSalesCreditAction} className="mt-4 grid gap-3 sm:grid-cols-2 lg:grid-cols-5">
                <input type="hidden" name="salesOrderId" value={wb.origin_id} />
                <input type="hidden" name="arDocumentId" value={creditRes.rows[0].id} />
                <input type="hidden" name="requestKey" value={crypto.randomUUID()} />
                <label className="text-sm">Refund date<input className="field" name="refundDate" type="date" defaultValue={new Date().toISOString().slice(0, 10)} required /></label>
                <label className="text-sm">Amount {creditRes.rows[0].currency_code}<input className="field" name="foreignAmount" type="number" min="0.01" max={Math.abs(Number(creditRes.rows[0].open_foreign))} step="0.01" required /></label>
                <label className="text-sm">FX to THB<input className="field" name="fxRate" type="number" min="0.0000000001" step="0.0000000001" defaultValue={creditRes.rows[0].currency_code.trim() === 'THB' ? '1' : ''} /></label>
                <button className="action-button self-end">Post partial refund</button>
                <div className="self-end text-xs text-mute">{creditRes.rows[0].document_no} · Credit {Math.abs(Number(creditRes.rows[0].open_foreign)).toLocaleString()} {creditRes.rows[0].currency_code}</div>
              </form>
            </section>
          )}

          <section className="space-y-3">
            <header className="flex items-baseline justify-between">
              <h2 className="text-sm font-mono uppercase tracking-widest text-ink-2">
                Record to GL
              </h2>
              <span className="text-xs font-mono text-mute">
                stage · {wb.current_stage}
              </span>
            </header>
            <SalesExtractPanel
              lang={locale as 'en' | 'th' | 'de'}
              waybillId={wb.id}
              soId={wb.origin_id}
              soItems={soItems}
              existingArSlipId={existingArSlipId}
              canRecord={canRecordSalesPayment}
            />
          </section>

          <WaybillAuditSection
          waybillId={wb.id}
          events={events}
          integrity={integrity}
          locale={locale}
        />
        </section>
      </PageLayout>
    </>
  );
}
