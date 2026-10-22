import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { Table, TableFrame } from "@erpx/ui/components/foundation";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import { vatReviewLabels, vatTreatmentLabels } from "@erpx/feature-finance/lib/finance-vat-labels";
import { vatCorrectionDetail } from "@erpx/server-finance/public/finance-tax-register";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { FinanceWorkspace } from "@erpx/feature-finance/components/finance-workspace";
import { VatCorrectionReview } from "@erpx/feature-records/components/loaders/finance--vat-register-controls";
import { Card } from "@erpx/ui/components/foundation";

export const dynamic = "force-dynamic";
export default async function VatCorrectionPage({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params,
    actor = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(actor)),
    data = await vatCorrectionDetail(actor, id).catch((error) => pageError(error, holdingSlug)),
    c = data.correction;
  const canReview =
    hasPermission(actor, "finance.write") &&
    hasPermission(actor, "finance.approve") &&
    !actor.branchId &&
    !actor.departmentId &&
    c.prepared_by !== actor.userId;
  return (
    <FinanceWorkspace actor={actor}
      base={base}
      active="tax"
      title="VAT correction set"
      description={`${vatReviewLabels[String(c.status)]} · ${data.entries.length} Entries · Version ${Number(c.version)}`}
    >
      <Link href={canonicalUiHref(`${base}/finance/tax?chart=${c.chart_id}`)}>Back to tax register</Link>
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Reason and review decision</h2>
        <p>{String(c.reason)}</p>
        {c.review_reason && <p>Review decision: {String(c.review_reason)}</p>}
        <p className="muted">
          Review the complete correction set. Original entries and prior exports remain retained.
          Approval does not file externally or create another journal.
        </p>
        {c.status === "prepared" && !canReview && (
          <p>Awaiting a different authorized Finance reviewer.</p>
        )}
      </Card>
      {data.entries.map(({ previous: old, replacement: next }) => {
        const evidence = JSON.parse(String(next.evidence_json));
        return (
          <Card as="section" appearance="standard" className="ui-panel" key={String(next.id)}>
            <h2>
              {String(old.document_reference)} ·{" "}
              {next.source_type !== "invoice"
                ? "Return linked to original"
                : "Original document"}
            </h2>
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
                    <th>Review field</th>
                    <th>Original evidence</th>
                    <th>Replacement entry</th>
                  </tr>
                </thead>
                <tbody>
                  <tr>
                    <th>Document reference</th>
                    <td>{String(old.document_reference)}</td>
                    <td>{String(next.document_reference)}</td>
                  </tr>
                  <tr>
                    <th>Tax-point date</th>
                    <td>{String(old.tax_point_date)}</td>
                    <td>{String(next.tax_point_date)}</td>
                  </tr>
                  <tr>
                    <th>Claim month</th>
                    <td>{String(old.claim_month)}</td>
                    <td>{String(next.claim_month)}</td>
                  </tr>
                  <tr>
                    <th>VAT treatment</th>
                    <td>{vatTreatmentLabels[String(old.treatment)]}</td>
                    <td>{vatTreatmentLabels[String(next.treatment)]}</td>
                  </tr>
                  <tr>
                    <th>Taxable amount (THB)</th>
                    <td>{minorToDecimal(Number(old.taxable_minor))}</td>
                    <td>{minorToDecimal(Number(next.taxable_minor))}</td>
                  </tr>
                  <tr>
                    <th>VAT (THB)</th>
                    <td>{minorToDecimal(Number(old.tax_minor))}</td>
                    <td>{minorToDecimal(Number(next.tax_minor))}</td>
                  </tr>
                  {next.direction === "input" && (
                    <tr>
                      <th>Deductible input VAT (THB)</th>
                      <td>
                        {minorToDecimal(Number(old.deductible_minor))} (
                        {Number(old.deductible_bps) / 100}%)
                      </td>
                      <td>
                        {minorToDecimal(Number(next.deductible_minor))} (
                        {Number(next.deductible_bps) / 100}%)
                      </td>
                    </tr>
                  )}
                </tbody>
              </Table>
            </TableFrame>
            <p>Tax rules and legal sources: {evidence.facts.legalBasis}</p>
            <p>Tax-point evidence: {evidence.facts.taxPointEvidence}</p>
            <p>Document evidence: {evidence.facts.documentEvidence}</p>
            <div className="ui-form-actions">
              <Link href={canonicalUiHref(`${base}/finance/tax/entries/${old.id}`)}>Original evidence</Link>
              <Link href={canonicalUiHref(`${base}/finance/tax/entries/${next.id}`)}>
                Replacement entry and history
              </Link>
              <Link href={canonicalUiHref(`${base}/finance/journals/${evidence.source.journal_id}`)}>
                Source journal
              </Link>
            </div>
          </Card>
        );
      })}
      {c.status === "prepared" && canReview && <VatCorrectionReview base={base} correction={c} />}
    </FinanceWorkspace>
  );
}
