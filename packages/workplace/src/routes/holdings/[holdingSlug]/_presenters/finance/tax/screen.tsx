import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { financePresentationQueries } from "@erpx/server-finance/public/presentation-queries";
import {recordCanonicalHref} from "@erpx/feature-records/lib/record-drawer";
import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";
import { Button, Field, Table, TableFrame, TextInput } from "@erpx/ui/components/foundation";


import { listCharts } from "@erpx/server-finance/public/finance";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import { vatPackageLabels, vatReviewLabels, vatTreatmentLabels } from "@erpx/feature-finance/lib/finance-vat-labels";
import { vatWorkspace } from "@erpx/server-finance/public/finance-tax-register";
import { FinanceWorkspace, financeStyles as styles } from "@erpx/feature-finance/components/finance-workspace";
import {
  VatEntryPreparation,
  VatEntryReview,
  VatPackagePreparation,
} from "@erpx/feature-finance/components/vat-register-controls";
import { FormLayout } from "@erpx/ui/components/foundation";

export const dynamic = "force-dynamic";
export default async function TaxPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ chart?: string; filingMonth?: string }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor)),
    q = await searchParams,
    data = await vatWorkspace(actor, { chartId: q.chart, filingMonth: q.filingMonth });
  const monthEnd=new Date(Date.UTC(Number(data.filingMonth.slice(0,4)),Number(data.filingMonth.slice(5,7)),0)).toISOString().slice(0,10);
  const [periods, charts] = await Promise.all([
      (await financePresentationQueries(actor.holdingId)).financeEntrypointQueries().periodEndDates(actor.companyId),
      listCharts(actor),
    ]),
    period = periods.results.find((row) => String(row.end_date).slice(0, 7) === data.filingMonth),
    chart = charts.find((row) => row.id === data.chartId);
  const canPrepare =
      hasPermission(actor, "finance.write") && !actor.branchId && !actor.departmentId,
    canApprove = hasPermission(actor, "finance.approve") && canPrepare;
  return (
    <FinanceWorkspace actor={actor}
      base={base}
      selectedPeriodId={period ? String(period.id) : undefined}
      filingMonth={data.filingMonth}
      context={
        chart
          ? {
              chartId: String(chart.id),
              chartName: String(chart.name),
              currency: String(chart.base_currency),
            }
          : undefined
      }
      active="tax"
      title="Tax"
      description="Source documents and GL, independent review, workpapers and retained external filing evidence."
    >
      {!actor.branchId&&!actor.departmentId&&<p><Link href={canonicalUiHref(`${base}/finance/thai-tax`)}>Thai statutory identities, supplier withholding and retained 50 ทวิ / VAT documents</Link></p>}
      {!data.profiles.some((profile) => profile.statutory_chart_id === data.chartId) ? (
        <section className={styles.panel}>
          <h2>Start with an approved statutory chart</h2>
          <p>
            The preparer and independent reviewer must confirm NPAEs eligibility, a THB statutory
            chart and Company policy before starting the tax register.
          </p>
          <UiActionLink appearance="standard" variant="primary" href={canonicalUiHref(`${base}/finance/setup?step=profile`)}>
            Open Company accounting setup
          </UiActionLink>
          {data.profiles.map((profile) => (
            <p key={String(profile.id)}>
              <Link href={canonicalUiHref(`${base}/finance/tax?chart=${profile.statutory_chart_id}`)}>
                Open approved statutory chart →
              </Link>
            </p>
          ))}
        </section>
      ) : (
        <>
          <FormLayout className="ui-form-grid">
            <TextInput appearance="standard" type="hidden" name="chart" value={data.chartId} />
            <Field appearance="standard">
              Filing month
              <TextInput appearance="standard" name="filingMonth" type="month" defaultValue={data.filingMonth} required />
            </Field>
            <Button appearance="standard" type="submit" variant="secondary">
              Show register
            </Button>
          </FormLayout>
          <section className={styles.panel}>
            <h2>Register to GL reconciliation</h2>
            <p>
              {data.reconciliation?.ready
                ? "Source VAT matches the GL"
                : "Source evidence or differences need review"}{" "}
              · Documents awaiting review: {data.reconciliation?.missing ?? 0} documents
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
                    <th>VAT account</th>
                    <th>Type</th>
                    <th>Cumulative GL</th>
                    <th>Register by accounting date</th>
                    <th>Difference</th>
                  </tr>
                </thead>
                <tbody>
                  {data.reconciliation?.rows.map((r) => (
                    <tr key={`${r.id}-${r.direction}`}>
                      <td>
                        <Link href={recordCanonicalHref(base,{kind:"trial_balance",id:String(r.id),args:{chartId:data.chartId,endDate:monthEnd}})}>{String(r.code)} · {String(r.name)}</Link>
                      </td>
                      <td>{r.direction === "output" ? "Output VAT" : "Input VAT"}</td>
                      <td>{minorToDecimal(Number(r.ledger_minor))}</td>
                      <td>{minorToDecimal(Number(r.register_minor))}</td>
                      <td>{minorToDecimal(Number(r.ledger_minor) - Number(r.register_minor))}</td>
                    </tr>
                  ))}
                </tbody>
              </Table>
            </TableFrame>
          </section>
          {Number(data.reconciliation?.pendingCorrections) > 0 && (
            <p className="alert">
              Correction sets: {Number(data.reconciliation?.pendingCorrections)} sets awaiting
              review. Approve or return them before preparing workpapers or closing the period.
            </p>
          )}
          <details className={styles.disclosure}>
            <summary>Correction sets and history</summary>
            <div>
              <p className="muted">
                Correction sets for this chart, including changes to claim month.
              </p>
              {data.corrections.map((c) => (
                <p key={String(c.id)}>
                  <Link href={canonicalUiHref(`${base}/finance/tax/corrections/${c.id}`)}>
                    {String(c.document_reference)} · {vatReviewLabels[String(c.status)]} · Version{" "}
                    {Number(c.version)}
                  </Link>{" "}
                  · {String(c.reason)}
                </p>
              ))}
              {!data.corrections.length && <p>No correction sets yet.</p>}
            </div>
          </details>
          {canPrepare && data.candidates.length > 0 && (
            <details className={styles.disclosure}>
              <summary>Prepare VAT entry from source</summary>
              <div>
                <VatEntryPreparation base={base} candidates={data.candidates} />
              </div>
            </details>
          )}
          <section className={styles.panel}>
            <h2>VAT register · {data.filingMonth}</h2>
            <p className="muted">
              Document date, accounting date, tax point and claim month remain distinct. Approved
              entries retain their evidence history.
            </p>
            {!data.entries.length && <p>No entries in the selected month.</p>}
            {data.entries.map((e) => {
              const evidence = JSON.parse(String(e.evidence_json));
              return (
                <details className={styles.disclosure} key={String(e.id)}>
                  <summary>
                    {String(e.document_reference)} ·{" "}
                    {e.direction === "output" ? "Output VAT" : "Input VAT"} ·{" "}
                    {minorToDecimal(Number(e.tax_minor))} THB · {vatReviewLabels[String(e.status)]}
                  </summary>
                  <div>
                    <p>
                      Document {String(e.document_date)} · Accounting date{" "}
                      {String(e.accounting_date)} · VAT {String(e.tax_point_date)} · Entitlement{" "}
                      {String(e.claim_month)}
                    </p>
                    <p>
                      Taxable {minorToDecimal(Number(e.taxable_minor))} · VAT{" "}
                      {minorToDecimal(Number(e.tax_minor))} ·{" "}
                      {vatTreatmentLabels[String(e.treatment)]} ·{" "}
                      {vatReviewLabels[String(e.status)]}
                    </p>
                    <details>
                      <summary>Evidence and tax rules</summary>
                      <p>Tax rules and legal sources: {evidence.facts.legalBasis}</p>
                      <p>Tax-point evidence: {evidence.facts.taxPointEvidence}</p>
                      <p>Document evidence: {evidence.facts.documentEvidence}</p>
                      {evidence.originalDocumentReference && (
                        <p>Original reference: {evidence.originalDocumentReference}</p>
                      )}
                      {e.review_reason && <p>Review decision: {String(e.review_reason)}</p>}
                    </details>
                    <div className="ui-form-actions">
                      <Link href={canonicalUiHref(`${base}/finance/tax/entries/${e.id}`)}>
                        Open entry and correction history
                      </Link>
                      <Link href={canonicalUiHref(`${base}/finance/journals/${evidence.source.journal_id}`)}>
                        Open source journal
                      </Link>
                    </div>
                    {e.status === "prepared" && canApprove && e.prepared_by !== actor.userId && (
                      <VatEntryReview base={base} entry={e} />
                    )}
                  </div>
                </details>
              );
            })}
          </section>
          {canPrepare && (
            <details className={styles.disclosure}>
              <summary>Prepare VAT workpaper</summary>
              <div>
                <VatPackagePreparation
                  base={base}
                  chartId={data.chartId}
                  filingMonth={data.filingMonth}
                  packages={data.packages}
                  ready={
                    data.reconciliation?.ready === true &&
                    data.entries.every((e) => e.status !== "prepared")
                  }
                />
              </div>
            </details>
          )}
          <section className={styles.panel}>
            <h2>Workpaper packages and filing evidence</h2>
            {data.packages.map((p) => (
              <p key={String(p.id)}>
                <Link href={canonicalUiHref(`${base}/finance/tax/packages/${p.id}`)}>
                  {String(p.filing_month)} ·{" "}
                  {p.filing_kind === "original" ? "Original" : "Additional"} ·{" "}
                  {vatPackageLabels[String(p.status)] ?? String(p.status)} · Version{" "}
                  {Number(p.version)}
                </Link>
                {p.filing_reference && <> · Receipt reference {String(p.filing_reference)}</>}
              </p>
            ))}
            {!data.packages.length && <p>No workpaper packages prepared yet.</p>}
          </section>
          <p className="muted">
            Supported sources are THB invoices and Commerce returns. Workpapers exclude carried
            credits and additional-filing adjustments. The responsible accountant must check the
            official form before filing. WHT, issued Thai statutory forms and official import files
            remain outside this workflow.
          </p>
        </>
      )}
    </FinanceWorkspace>
  );
}
