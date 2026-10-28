import {financeProductPresentationOptions} from "@erpx/server-finance/public/product-presentation-request";
import {financeDocumentPostAccess} from "@erpx/server-finance/public/document-purchases";
import {financeDocumentPresentationAccess,financeDocumentFlow} from "@erpx/server-finance/public/record-presentation";
import { financePurchaseControls } from "@erpx/server-finance/public/purchases";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";
import { Table, TableFrame } from "@erpx/ui/components/foundation";
import { FinanceRecordWorkspace, FinanceLegacyHeading } from "@erpx/feature-records/components/loaders/finance--finance-record-workspace";
import { BusinessDocumentBranding } from "@erpx/feature-brand/components/business-document";
import { RecordDocumentPanel } from "@erpx/feature-records/components/loaders/attachments--record-document-panel";
import {
  FeaturePage,
  FeatureTabs,
  FeatureWidget,
  WidgetGrid,
} from "@erpx/feature-shared/components/features/feature-page";
import { JournalAction, JournalEditor } from "@erpx/feature-records/components/loaders/modules--ledger-controls";
import { StatusBadge } from "@erpx/ui/components/primitives";
import { FlowPanel } from "@erpx/workplace/components/flow-panel";
import { RecordNextAction } from "@erpx/workplace/components/record-next-action";
import { WorkReturn } from "@erpx/feature-records/components/loaders/workplace--work-return";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import {journalRecordWorkspace} from "@erpx/server-finance/public/record-presentation";
import {nativeJournalPageWorkspace,nativeJournalPageFailure} from "@erpx/feature-finance/public/server";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { ApiError } from "@erpx/server-runtime/public/http";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
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
  const nativeData = await nativeJournalPageWorkspace(actor, holdingSlug, id).catch(error => {
    const mapped = nativeJournalPageFailure(error);
    if (!mapped) throw error;
    return pageError(new ApiError(mapped.status, mapped.code, mapped.message), holdingSlug);
  });
  const data = nativeData ?? await journalRecordWorkspace(actor,id).catch(error=>pageError(error,holdingSlug));
  const {journal,lines,history}=data;
  const documentPost = ["draft","in_review"].includes(String(journal.status)) ? await financeDocumentPostAccess(actor,"journal",id) : undefined;
  const canApprove = hasPermission(actor, "finance.approve");
  const flow = await financeDocumentFlow(actor, "journal", id);
  return (
    <FeaturePage
      className="ui-page-content"
      data-source="live"
      data-print-document={printing || undefined}
    >
      <FinanceRecordWorkspace sourceHistory={(await financeDocumentPresentationAccess(actor,"journal",id,await financeProductPresentationOptions())).sourceOnly} purchases={await financePurchaseControls(actor)}
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
          <Link className="back-link" href={canonicalUiHref(`${base}/finance/accounting?chart=${journal.chart_id}`)}>
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
            <JournalAction sourceCanPost={documentPost?.canPost}
              nativePost={process.env.ERPX_NATIVE_FINANCE_JOURNAL_POST_CONFIG === undefined ? undefined : { context: actor.authority && actor.iam ? {
                userId: actor.userId, holdingId: actor.holdingId, companyId: actor.companyId,
                branchId: actor.branchId, departmentId: actor.departmentId,
                branchSlug: actor.scope.kind === 'holding' ? null : actor.scope.branchSlug ?? null,
                departmentSlug: actor.scope.kind === 'department' ? actor.scope.departmentSlug : null,
                assignmentId: actor.assignmentId, assignmentVersion: actor.authority.assignmentVersion,
                revision: actor.authority.revision, role: actor.role, assignmentScopeKind: actor.assignmentScope,
                modules: [...actor.modules], permissions: [...actor.iam.permissions],
              } : undefined }}
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
          {journal.status==='draft'&&journal.source_type==='manual'&&hasPermission(actor,'finance.write')&&<FeatureWidget appearance="standard" id="finance-journals-detail-draft-edit" title="Complete or revise draft"><JournalEditor base={base} chart={data.chart} accounts={data.accounts} periods={data.periods} dimensions={data.dimensions} dimensionValues={data.dimensionValues} draft={journal} draftLines={lines}/></FeatureWidget>}
          {history.length>0&&<FeatureWidget appearance="standard" id="finance-journals-detail-draft-history" title="Retained draft history"><ul>{history.map(h=><li key={String(h.id)}><details><summary>Draft version {Number(h.journal_version)} · {String(h.reason)}</summary>{(()=>{const snapshot=JSON.parse(String(h.snapshot_json)) as {journal:{entry_date:string;memo:string;currency:string};lines:Array<{account_id:string;description:string;debit_minor:number;credit_minor:number;dimension_json:string}>};return <><p>{snapshot.journal.entry_date} · {snapshot.journal.currency} · {snapshot.journal.memo}</p><TableFrame appearance="standard" label={`Draft version ${h.journal_version}`}><Table><thead><tr><th>Account</th><th>Description</th><th>Debit</th><th>Credit</th><th>Reporting dimensions</th></tr></thead><tbody>{snapshot.lines.map((l,i)=><tr key={i}><td>{String(data.accounts.find(a=>a.id===l.account_id)?.code??'Historical account')} · {String(data.accounts.find(a=>a.id===l.account_id)?.name??l.account_id)}</td><td>{l.description}</td><td>{minorToDecimal(l.debit_minor)}</td><td>{minorToDecimal(l.credit_minor)}</td><td>{Object.entries(JSON.parse(l.dimension_json) as Record<string,string>).map(([k,v])=>`${String(data.dimensions.find(d=>d.code===k)?.name??k)}: ${v}`).join(' · ')||'—'}</td></tr>)}</tbody></Table></TableFrame></>})()}</details></li>)}</ul></FeatureWidget>}
          {journal.source_id &&
            ["stock_count", "expense_claim", "reversal"].includes(String(journal.source_type)) && (
              <UiActionLink appearance="standard"
                variant="secondary"
                href={canonicalUiHref(`${base}/${journal.source_type === "stock_count" ? "stock-counts" : journal.source_type === "expense_claim" ? "expenses" : "finance/journals"}/${journal.source_id}`)}
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
              href={canonicalUiHref(`${base}/pms/reports?property=${journal.source_id}`)}
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
                        <Link href={canonicalUiHref(`${base}/record-details/finance_account/${encodeURIComponent(String(l.account_id))}`)}>{String(l.code)} · {String(l.name)}</Link>
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
