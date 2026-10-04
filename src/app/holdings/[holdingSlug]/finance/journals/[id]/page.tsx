import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { Table, TableFrame } from "@/components/ui/foundation";
import {
  FinanceRecordWorkspace,
  FinanceLegacyHeading,
} from "@/components/finance/finance-record-workspace";
import { BusinessDocumentBranding } from "@/components/brand/business-document";
import { RecordDocumentPanel } from "@/components/attachments/record-document-panel";
import {
  FeaturePage,
  FeatureTabs,
  FeatureWidget,
  WidgetGrid,
} from "@/components/features/feature-page";
import { JournalAction } from "@/components/modules/ledger-controls";
import { StatusBadge } from "@/components/ui/primitives";
import { FlowPanel, documentFlow } from "@/components/workplace/flow-panel";
import { RecordNextAction } from "@/components/workplace/record-next-action";
import { WorkReturn } from "@/components/workplace/work-return";
import { minorToDecimal } from "@/lib/finance-money";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { journalDetail, ledgerWorkspace } from "@/server/ledger";
import { pageError } from "@/server/page-error";
import Link from "next/link";
export const dynamic = "force-dynamic";
export default async function JournalPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
  searchParams: Promise<{ print?: string; brand?: string }>;
}) {
  const { holdingSlug, id: routeId } = await params;
  const id = decodeURIComponent(routeId);
  const query = await searchParams,
    printing = query.print === "1";
  const actor = await requireWorkContext(holdingSlug);
  const base = workspaceBase(publicScope(actor));
  const { journal, lines } = await journalDetail(actor, id).catch((error) =>
    pageError(error, holdingSlug),
  );
  const data = await ledgerWorkspace(actor, String(journal.chart_id));
  const canApprove = hasPermission(actor, "finance.approve");
  const flow = await documentFlow(actor, "journal", id);
  return (
    <FeaturePage
      className="ui-page-content"
      data-source="live"
      data-print-document={printing || undefined}
    >
      <FinanceRecordWorkspace
        base={base}
        active="accounting"
        title={String(journal.journal_number)}
        description={`${journal.memo} · ${journal.entry_date} · Prepared by ${journal.created_by_name}`}
        context={{
          chartId: String(data.chart.id),
          chartName: String(data.chart.name),
          currency: String(data.chart.base_currency),
        }}
        actions={<StatusBadge appearance="standard" status={String(journal.status)} />}
      >
        <WorkReturn base={base} />
        <BusinessDocumentBranding
          actor={actor}
          kind="journal"
          recordId={id}
          requestedMode={query.brand}
          title="Journal entry"
          reference={String(journal.journal_number)}
          printing={printing}
        />
        <FinanceLegacyHeading>
          <Link className="back-link" href={`${base}/finance/accounting?chart=${journal.chart_id}`}>
            ← General ledger
          </Link>
          <header className="ui-section-header">
            <div>
              <p className="eyebrow">{String(journal.source_type).replaceAll("_", " ")}</p>
              <h1>{String(journal.journal_number)}</h1>
              <p>{String(journal.memo)}</p>
              <p className="muted">
                {String(journal.entry_date)} · {String(journal.currency)} · Prepared by{" "}
                {String(journal.created_by_name)}
                {journal.posted_by && <> · Posted by {String(journal.posted_by_name)}</>}
              </p>
            </div>
            <StatusBadge appearance="standard" status={String(journal.status)} />
          </header>
        </FinanceLegacyHeading>
        <FeatureTabs appearance="standard" />
        <WidgetGrid pageKey="finance-journals-detail">
          <RecordNextAction flow={flow} />
          <FeatureWidget appearance="standard" id="finance-journals-detail-journal-actions" title="Journal actions">
            <JournalAction
              base={base}
              journal={journal}
              canPost={
                journal.source_type !== "pms" &&
                (flow.canAct || journal.status === "posted") &&
                canApprove &&
                (journal.status === "posted" || journal.created_by !== actor.userId)
              }
              periods={data.periods}
            />
          </FeatureWidget>
          {journal.source_id &&
            ["stock_count", "expense_claim", "reversal"].includes(String(journal.source_type)) && (
              <UiActionLink appearance="standard"
                variant="secondary"
                href={`${base}/${journal.source_type === "stock_count" ? "stock-counts" : journal.source_type === "expense_claim" ? "expenses" : "finance/journals"}/${journal.source_id}`}
              >
                {journal.source_type === "stock_count"
                  ? "Open physical count"
                  : journal.source_type === "expense_claim"
                    ? "Open employee expense"
                    : "Open original journal"}
              </UiActionLink>
            )}
          {journal.source_type === "pms" && (
            <UiActionLink appearance="standard"
              variant="secondary"
              href={`${base}/pms/reports?property=${journal.source_id}`}
            >
              Open PMS subledger
            </UiActionLink>
          )}
          <FeatureWidget appearance="standard" id="finance-journals-detail-journal-lines" title="Journal lines">
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
                    <th>Debit ({String(journal.currency)})</th>
                    <th>Credit ({String(journal.currency)})</th>
                  </tr>
                </thead>
                <tbody>
                  {lines.map((l) => (
                    <tr key={String(l.id)}>
                      <td>
                        {String(l.code)} · {String(l.name)}
                      </td>
                      <td>{minorToDecimal(Number(l.debit_minor))}</td>
                      <td>{minorToDecimal(Number(l.credit_minor))}</td>
                    </tr>
                  ))}
                </tbody>
              </Table>
            </TableFrame>
          </FeatureWidget>
          {!printing && (
            <RecordDocumentPanel appearance="standard"
              base={base}
              targetType="finance_journal"
              targetId={id}
              expectedTargetVersion={Number(journal.version)}
              editable={hasPermission(actor, "finance.write")}
              title="Journal support"
              purposes={[
                {
                  key: "journal_support",
                  label: "Journal support",
                  description:
                    "Attach the calculation, approval source, or other document supporting this journal revision.",
                },
              ]}
            />
          )}{" "}
          {journal.status === "draft" && journal.created_by === actor.userId && (
            <p className="muted">A different finance approver must review and post this journal.</p>
          )}
          <FlowPanel flow={flow} actor={actor} kind="journal" id={id} base={base} />
        </WidgetGrid>
      </FinanceRecordWorkspace>
    </FeaturePage>
  );
}
