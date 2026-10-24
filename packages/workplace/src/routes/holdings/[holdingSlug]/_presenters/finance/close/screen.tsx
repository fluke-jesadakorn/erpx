import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import {recordCanonicalHref} from "@erpx/feature-records/lib/record-drawer";
import { Table, TableFrame } from "@erpx/ui/components/foundation";
import { notFound } from "next/navigation";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import { FinanceWorkspace, financeStyles as styles } from "@erpx/feature-finance/components/finance-workspace";
import { FxRevaluationControls } from "@erpx/feature-records/components/loaders/finance--fx-revaluation-controls";
import { PeriodControls } from "@erpx/feature-records/components/loaders/modules--ledger-controls";
import { StatusBadge } from "@erpx/ui/components/primitives";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { closeWorkspace } from "@erpx/server-finance/public/finance-workspace";
import {fxRevaluationWorkspace} from '@erpx/server-finance/public/finance';
import { Card } from "@erpx/ui/components/foundation";

export const dynamic = "force-dynamic";
export default async function ClosePage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ periodId?: string }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor)),
    periods = await closeWorkspace(actor);
  const query = await searchParams,
    selected =
      periods.find((period) => period.id === query.periodId) ??
      periods.find((period) => period.state === "closing") ??
      periods.find((period) => period.state === "open") ??
      periods[0],
    visiblePeriods = selected ? [selected] : [];
  if (query.periodId && !periods.some((period) => period.id === query.periodId)) notFound();
  const revaluations = new Map(
    await Promise.all(
      visiblePeriods.map(
        async (period) => [period.id, await fxRevaluationWorkspace(actor, period.id)] as const,
      ),
    ),
  );
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <FinanceWorkspace actor={actor}
        base={base}
        selectedPeriodId={selected?.id}
        active="close"
        title="Period close center"
        description="See blockers, evidence, and close decisions in one place"
      >
        <details className={styles.disclosure}>
          <summary>Period close workflow</summary>
          <div>
            <section className={styles.process}>
              <article>
                <b>Review drafts</b>
                <p className="muted">Documents and journals must be reviewed</p>
              </article>
              <article>
                <b>Reconcile</b>
                <p className="muted">Bank and subledger differences must be explained</p>
              </article>
              <article>
                <b>Review reports</b>
                <p className="muted">Check the trial balance and reports before locking</p>
              </article>
              <article>
                <b>Approve period close</b>
                <p className="muted">An independent approver confirms the evidence</p>
              </article>
            </section>
          </div>
        </details>
        <section className={styles.section}>
          <div className={styles.sectionHead}>
            <div>
              <h2>Accounting periods</h2>
              <p className="muted">Blocker counts come from the company’s current data</p>
            </div>
          </div>
          <TableFrame appearance="standard"
            label="Records"
            className="business-table-wrap"
            tabIndex={0}
            role="region"
            aria-label="Financial table — scroll for more columns"
          >
            <Table className="business-table">
              <thead>
                <tr>
                  <th>Period</th>
                  <th>Dates</th>
                  <th>State</th>
                  <th>Blockers</th>
                </tr>
              </thead>
              <tbody>
                {periods.map((period) => (
                  <tr key={period.id}>
                    <td>
                      <Link
                        href={canonicalUiHref(`${base}/finance/close?periodId=${encodeURIComponent(period.id)}`)}
                        aria-current={selected?.id === period.id ? "page" : undefined}
                      >
                        {period.name}
                      </Link>
                    </td>
                    <td>
                      {period.start_date} – {period.end_date}
                    </td>
                    <td>{period.state}</td>
                    <td>{period.blockers}</td>
                  </tr>
                ))}
              </tbody>
            </Table>
          </TableFrame>
          {visiblePeriods.map((period) => (
            <article className={styles.panel} key={String(period.id)}>
              <div className={styles.sectionHead}>
                <div>
                  <h3>{String(period.name)}</h3>
                  <p className="muted">
                    {String(period.start_date)} – {String(period.end_date)}
                  </p>
                </div>
                <StatusBadge appearance="standard" status={String(period.state)} />
              </div>
              <div className={styles.metrics}>
                <Card as="div" appearance="standard" className="ui-panel">
                  <span className="eyebrow">Draft journals</span>
                  <strong>{Number(period.draft_journals)}</strong>
                </Card>
                <Card as="div" appearance="standard" className="ui-panel">
                  <span className="eyebrow">Unreconciled bank</span>
                  <strong>{Number(period.unmatched_bank)}</strong>
                </Card>
                <Card as="div" appearance="standard" className="ui-panel">
                  <span className="eyebrow">Draft documents</span>
                  <strong>{Number(period.draft_invoices)}</strong>
                </Card>
                <Card as="div" appearance="standard" className="ui-panel">
                  <span className="eyebrow">Open manufacturing</span>
                  <strong>{Number(period.open_manufacturing)}</strong>
                </Card>
                <Card as="div" appearance="standard" className="ui-panel">
                  <span className="eyebrow">Calculations awaiting review</span>
                  <strong>{Number(period.pending_calculations)}</strong>
                </Card>
                <Card as="div" appearance="standard" className="ui-panel">
                  <span className="eyebrow">VAT awaiting review</span>
                  <strong>{Number(period.pending_tax)}</strong>
                </Card>
                <Card as="div" appearance="standard" className="ui-panel"><span className="eyebrow">Due Finance source tasks</span><strong>{Number(period.pending_source_tasks)}</strong><Link href={canonicalUiHref(`${base}/finance`)}>Open Today to prepare or review sources</Link></Card>
              </div>
              {period.reconciliations.length > 0 && (
                <TableFrame appearance="standard"
                  label="Records"
                  className="business-table-wrap"
                  tabIndex={0}
                  role="region"
                  aria-label="Financial table — scroll for more columns"
                >
                  <Table className="business-table">
                    <thead>
                      <tr>
                        <th>Reconciliation</th>
                        <th>Account</th>
                        <th>GL</th>
                        <th>Source</th>
                        <th>Difference</th>
                        <th>Evidence</th>
                      </tr>
                    </thead>
                    <tbody>
                      {period.reconciliations.map((row) => (
                        <tr key={`${row.kind}:${row.accountId}`}>
                          <td>{row.kind}</td>
                          <td>
                            {row.chartName}
                            <br />
                            <Link href={recordCanonicalHref(base,{kind:"trial_balance",id:row.accountId,args:{endDate:String(period.end_date)}})}>{row.accountCode} · {row.accountName}</Link>
                          </td>
                          <td>
                            {row.currency} {(row.ledgerMinor / 100).toFixed(2)}
                          </td>
                          <td>
                            {row.currency} {(row.sourceMinor / 100).toFixed(2)}
                          </td>
                          <td>
                            {row.currency} {(row.differenceMinor / 100).toFixed(2)}
                          </td>
                          <td>
                            {row.kind === "bank"
                              ? row.source.evidenceComplete
                                ? `Statement ${String(row.source.statementNumber)} at period end`
                                : `Statement required at ${String(row.source.periodEnd)}`
                              : `${Number(row.source.invoiceCount ?? 0)} document(s) · ${Number(row.source.foreignOpenCount ?? 0)} foreign item(s) awaiting revaluation · ${Number(row.source.legacyDateCount ?? 0)} document(s) missing dated journal evidence`}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </Table>
                </TableFrame>
              )}
              {period.vat_reconciliation && (
                <section>
                  <h4>Statutory VAT reconciliation</h4>
                  <p className="muted">
                    Reconciliation from the profile effective date to period end ·{" "}
                    {period.vat_reconciliation.ready
                      ? "Reviewed register matches the GL"
                      : "Review documents and resolve differences before close"}
                  </p>
                  <TableFrame appearance="standard"
                    label="Records"
                    className="business-table-wrap"
                    tabIndex={0}
                    role="region"
                    aria-label="Financial table — scroll for more columns"
                  >
                    <Table className="business-table">
                      <thead>
                        <tr>
                          <th>Account</th>
                          <th>Type</th>
                          <th>GL (THB)</th>
                          <th>Register (THB)</th>
                          <th>Difference (THB)</th>
                        </tr>
                      </thead>
                      <tbody>
                        {period.vat_reconciliation.rows.map((row) => (
                          <tr key={`${row.id}:${row.direction}`}>
                            <td>
                              <Link href={recordCanonicalHref(base,{kind:"trial_balance",id:String(row.id),args:{endDate:String(period.end_date)}})}>{String(row.code)} · {String(row.name)}</Link>
                            </td>
                            <td>{row.direction === "output" ? "Output VAT" : "Input VAT"}</td>
                            <td>{(Number(row.ledger_minor) / 100).toFixed(2)}</td>
                            <td>{(Number(row.register_minor) / 100).toFixed(2)}</td>
                            <td>
                              {(
                                (Number(row.ledger_minor) - Number(row.register_minor)) /
                                100
                              ).toFixed(2)}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </Table>
                  </TableFrame>
                </section>
              )}
              {revaluations.get(period.id) && (
                <FxRevaluationControls
                  base={base}
                  data={revaluations.get(period.id)!}
                  canPrepare={hasPermission(actor, "finance.write")}
                  canApprove={hasPermission(actor, "finance.approve")}
                  userId={actor.userId}
                />
              )}
              <p>
                {Number(period.blockers) === 0
                  ? "Automated blocker checks pass; ready for evidence review"
                  : "Period cannot close yet: resolve the items above"}
              </p>
              {hasPermission(actor, "finance.write") && (
                <PeriodControls
                  base={base}
                  period={period}
                  canApprove={hasPermission(actor, "finance.approve")}
                  userId={actor.userId}
                />
              )}
            </article>
          ))}
          {!periods.length && (
            <p className={styles.empty}>
              No accounting periods yet. Configure a period before posting.
            </p>
          )}
        </section>
      </FinanceWorkspace>
    </FeaturePage>
  );
}
