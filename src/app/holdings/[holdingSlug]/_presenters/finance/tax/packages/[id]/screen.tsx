import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { InlineNotice } from "@/components/ui/foundation";
import { ActionAnchor as UiActionAnchor } from "@/components/ui/foundation";
import { Table, TableFrame } from "@/components/ui/foundation";
import { RecordLink as Link } from "@/components/records/record-link";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { hasPermission } from "@/lib/identity_access/permissions";
import { minorToDecimal } from "@/lib/finance/finance-money";
import { vatPackageLabels, vatTreatmentLabels } from "@/lib/finance/finance-vat-labels";
import { vatPackageDetail, type TaxRow } from "@/server/modules/finance/public/finance-tax-register";
import { FinanceWorkspace } from "@/components/finance/finance-workspace";
import { VatPackageControls } from "@/components/records/loaders/finance--vat-register-controls";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { Card } from "@/components/ui/foundation";

export const dynamic = "force-dynamic";
export default async function VatPackagePage({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params,
    actor = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(actor)),
    data = await vatPackageDetail(actor, id).catch((error) => pageError(error, holdingSlug)),
    p = data.package,
    result = data.snapshot.result;
  return (
    <FinanceWorkspace actor={actor}
      base={base}
      active="tax"
      title={`VAT workpaper (ภ.พ.30) · ${p.filing_month}`}
      description={`${p.filing_kind === "original" ? "Original" : "Additional"} · ${vatPackageLabels[String(p.status)] ?? p.status} · Version ${p.version}`}
    >
      <Link href={canonicalUiHref(`${base}/finance/tax?filingMonth=${p.filing_month}&chart=${p.chart_id}`)}>
        Back to tax register
      </Link>
      {p.export_hash && (
        <UiActionAnchor appearance="standard"
          variant="secondary"
          href={`${base.replace(canonicalUiHref("/holdings/"), "/api/v1/holdings/")}/finance/vat-packages/${p.id}/export`}
        >
          Download retained workpaper
        </UiActionAnchor>
      )}
      {data.pendingCorrections > 0 && (
        <InlineNotice tone="info" role="status" className="ui-panel">
          VAT correction sets awaiting review: {data.pendingCorrections} sets. Review corrections
          before approving or exporting this workpaper.
        </InlineNotice>
      )}
      {!data.currentRegisterMatches && (
        <InlineNotice tone="info" role="status" className="ui-panel">
          The current register has changed. This package retains its original figures and export.{" "}
          {["filed", "paid"].includes(String(p.status))
            ? "Review differences and prepare an additional filing referencing this package if required."
            : "If not filed, retain this package and prepare a replacement. If the original was filed, record the actual filing receipt evidence."}
        </InlineNotice>
      )}
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Figures from the reviewed register</h2>
        <p>
          Output VAT {minorToDecimal(result.outputVatMinor)} · Input VAT{" "}
          {minorToDecimal(result.inputVatMinor)} · Deductible{" "}
          {minorToDecimal(result.deductibleInputMinor)} THB
        </p>
        <p>Net before carried credits and form adjustments {minorToDecimal(result.netMinor)} THB</p>
        <p className="muted">
          This workpaper supports manual filing and has not been validated for Revenue Department
          import. Additional filings require comparison with prior filed figures and official form
          requirements.
        </p>
        {p.filing_reference && <p>Filing receipt reference {String(p.filing_reference)}</p>}
      </Card>
      {p.original_package_id && data.snapshot.priorFiledResult && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Referenced original filing</h2>
          <Link href={canonicalUiHref(`${base}/finance/tax/packages/${p.original_package_id}`)}>
            Open original package and receipt evidence
          </Link>
          <p>
            Prior figures: Output VAT{" "}
            {minorToDecimal(data.snapshot.priorFiledResult.outputVatMinor)} · Input VAT{" "}
            {minorToDecimal(data.snapshot.priorFiledResult.inputVatMinor)} · Deductible{" "}
            {minorToDecimal(data.snapshot.priorFiledResult.deductibleInputMinor)} THB
          </p>
          <p className="muted">
            Prior and current figures are retained separately. The responsible accountant must
            calculate additional filing figures using the official form requirements.
          </p>
        </Card>
      )}
      {hasPermission(actor, "finance.write") &&
        !actor.branchId &&
        !actor.departmentId &&
        ["prepared", "approved_for_export", "exported"].includes(String(p.status)) && (
          <VatPackageControls
            base={base}
            package={p}
            canApprove={hasPermission(actor, "finance.approve")}
            userId={actor.userId}
            currentReady={data.currentRegisterMatches && data.pendingCorrections === 0}
          />
        )}
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Retained package entries</h2>
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
                <th>Document</th>
                <th>Tax-point date</th>
                <th>Type</th>
                <th>Taxable amount</th>
                <th>VAT</th>
                <th>Reviewer</th>
              </tr>
            </thead>
            <tbody>
              {data.snapshot.register.map((e: TaxRow) => (
                <tr key={String(e.id)}>
                  <td>
                    <Link href={canonicalUiHref(`${base}/finance/tax/entries/${e.id}`)}>
                      {String(e.document_reference)}
                    </Link>
                  </td>
                  <td>{String(e.tax_point_date)}</td>
                  <td>{vatTreatmentLabels[String(e.treatment)] ?? String(e.treatment)}</td>
                  <td>{minorToDecimal(Number(e.taxable_minor))}</td>
                  <td>{minorToDecimal(Number(e.tax_minor))}</td>
                  <td>{String(e.review_reason)}</td>
                </tr>
              ))}
            </tbody>
          </Table>
        </TableFrame>
      </Card>
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Package history and evidence</h2>
        {data.events.map((e) => {
          const evidence = JSON.parse(String(e.evidence_json)),
            label =
              (
                {
                  prepare: "Prepare",
                  approve: "Approve",
                  export: "Export",
                  file: "Record filing",
                  supersede: "Retained as history",
                } as Record<string, string>
              )[String(e.action)] ?? String(e.action);
          return (
            <details key={String(e.id)}>
              <summary>
                Version {Number(e.version)} · {label}
              </summary>
              <p>{evidence.reason}</p>
              {evidence.filingReference && (
                <p>
                  Filing receipt reference: {evidence.filingReference} · Filed date:{" "}
                  {evidence.filedDate}
                </p>
              )}
              {evidence.receiptEvidence && <p>Receipt evidence: {evidence.receiptEvidence}</p>}
            </details>
          );
        })}
      </Card>
    </FinanceWorkspace>
  );
}
