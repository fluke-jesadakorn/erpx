import { RecordSections } from "@/components/records/loaders/workplace--record-sections";

import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { FinanceRecordWorkspace, FinanceLegacyHeading } from "@/components/records/loaders/finance--finance-record-workspace";
import { BusinessDocumentBranding } from "@/components/brand/business-document";
import { MoneyAmount } from "@/components/money/currency-provider";
import { TimeText } from "@/components/time/time-preference-context";
import { FeaturePage, FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { ApprovalAdminLink } from "@/components/modules/approval-admin-link";
import { ApprovalHistory } from "@/components/modules/approval-history";
import { ExpenseActions, ExpenseEditor, ExpenseReceipts } from "@/components/records/loaders/modules--expense-controls";
import { ExpenseWorkflowPage } from "@/components/modules/expense-workflow-page";
import { StatusBadge } from "@/components/ui/primitives";
import { FlowPanel,documentFlow } from "@/components/workplace/flow-panel";
import { RecordNextAction } from "@/components/workplace/record-next-action";
import { WorkReturn } from "@/components/records/loaders/workplace--work-return";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import {
canAccountExpenses,
canEditExpense,
canPostExpenses,
canReviewExpense,
expenseDetail,
expenseOptions,
} from "@/server/modules/people_payroll/expense";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { expensePageCapabilities } from "@/server/modules/people_payroll/expense";
import { RecordLink as Link } from "@/components/records/record-link";
import { Card } from "@/components/ui/foundation";
export const dynamic = "force-dynamic";
export default async function Expense({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
  searchParams: Promise<{ print?: string; brand?: string }>;
}) {
  const { holdingSlug, id } = await params,
    query = await searchParams,
    printing = query.print === "1",
    actor = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(actor));
  const [data, options] = await Promise.all([
    expenseDetail(actor, id).catch((e) => {
      return pageError(e, holdingSlug);
    }),
    expenseOptions(actor),
  ]);
  const {
      expense,
      attachments,
      history,
      payments,
      decisions,
      approval,
      permittedActions,
    } = data,
    editable = canEditExpense(actor, expense);
  const staffOptions = await expensePageCapabilities(actor);
  if (expense.workflow_version === 2)
    return (
      <FeaturePage className="ui-page-content" data-source="live" data-print-document={printing || undefined}><FinanceRecordWorkspace base={base} active="receivables" title={String(expense.merchant || "New expense claim")} description={`${expense.legal_name} · ${expense.request_kind === "payment" ? "Payment request" : "Expense claim"}`} actions={<StatusBadge appearance="standard" status={String(expense.status)}/>}><WorkReturn base={base}/><BusinessDocumentBranding actor={actor} kind="expense" recordId={id} requestedMode={query.brand} title={expense.request_kind === "payment" ? "Payment request" : "Expense claim"} reference={String(expense.receipt_reference ?? id)} printing={printing}/><ExpenseWorkflowPage
        base={base}
        actor={actor}
        data={data}
        options={options}
        canRepair={staffOptions.canRepair}
      /></FinanceRecordWorkspace></FeaturePage>
    );
  const posting =
    canPostExpenses(actor) &&
    !actor.branchId &&
    !actor.departmentId &&
    ![
      expense.created_by,
      expense.prepared_by,
      expense.claimant_user_id,
    ].includes(actor.userId);
  const flow = await documentFlow(actor,"expense",id);
  return (
    <FeaturePage className="ui-page-content" data-source="live" data-print-document={printing || undefined}><FinanceRecordWorkspace base={base} active="receivables" title={String(expense.merchant)} description={`${expense.legal_name} · ${expense.expense_date} · ${expense.category_name}`} actions={<StatusBadge appearance="standard" status={String(expense.status)}/>}><WorkReturn base={base}/>
      <BusinessDocumentBranding actor={actor} kind="expense" recordId={id} requestedMode={query.brand} title="Expense claim" reference={String(expense.receipt_reference ?? id)} printing={printing}/>
      <FinanceLegacyHeading><Link className="back-link" href={`${base}/expenses`}>
        ← Employee expenses
      </Link>
      <div className="ui-section-header">
        <div>
          <p className="eyebrow">
            Expense claim · {String(expense.receipt_reference ?? id)}
          </p>
          <h1>{String(expense.merchant)}</h1>
          <p className="muted">
            {String(expense.legal_name)} · {String(expense.expense_date)} ·{" "}
            {String(expense.category_name)}
          </p>
        </div>
        <StatusBadge appearance="standard" status={String(expense.status)} />
      </div></FinanceLegacyHeading><RecordSections evidence={<ExpenseReceipts
        base={base}
        expense={expense}
        attachments={attachments}
        editable={editable}
      />} history={<>      <FeatureWidget appearance="standard" id="expenses-detail-review-decisions" title="Review decisions"><Card as="section" appearance="standard" className="ui-panel">

        {decisions.length === 0 ? (
          <p className="muted">No review decisions yet.</p>
        ) : (
          decisions.map((d) => (
            <div className="ui-record-item" key={String(d.id)}>
              <div className="row-main">
                <strong>
                  {String(d.action)} · {String(d.actor_name)}
                </strong>
                <p>{String(d.reason)}</p>
                <span className="muted">
                  Reviewed version {String(d.reviewed_version)}
                </span>
              </div>
            </div>
          ))
        )}
      </Card></FeatureWidget>
      <FeatureWidget appearance="standard" id="expenses-detail-activity" title="Activity"><Card as="section" appearance="standard" className="ui-panel">

        {history.map((a) => (
          <div className="ui-record-item" key={String(a.id)}>
            <div className="row-main">
              <strong>{String(a.action).replaceAll(".", " ")}</strong>
              <div className="muted">
                {String(a.actor_name)} ·{" "}
                <TimeText appearance="standard" value={Number(a.created_at)} />
              </div>
            </div>
          </div>
        ))}
      </Card></FeatureWidget>
      <ApprovalHistory
        approval={approval}
        blockedReason={
          expense.approval_blocked_reason
            ? String(expense.approval_blocked_reason)
            : null
        }
      />
</>}><WidgetGrid pageKey="expenses-detail">
      <section data-critical-notice><RecordNextAction flow={flow}/></section><ExpenseActions
        canCancel={permittedActions.includes("cancel")}
        base={base}
        expense={expense}
        editable={editable}
        reviewable={canReviewExpense(actor, expense)}
        canPost={posting}
        periods={options.periods}
      /><section className="card-grid">
        {[
          ["Net amount", String(expense.amount_minor)],
          ["Recoverable tax", String(expense.tax_minor)],
          [
            "Claim total",
            (BigInt(String(expense.amount_minor)) + BigInt(String(expense.tax_minor))).toString(),
          ],
          [
            "Outstanding",
            expense.cancelled_at
              ? "0"
              : (expense.outstanding_minor ??
                (BigInt(String(expense.amount_minor)) + BigInt(String(expense.tax_minor))).toString()),
          ],
        ].map(([label, value]) => (
          <Card as="article" appearance="standard" className="ui-panel" key={String(label)}>
            <span className="eyebrow">{String(label)}</span>
            <strong>
              <MoneyAmount units={String(value)} currency={String(expense.currency)} date={String(expense.expense_date)} legacyScale={2} />
            </strong>
          </Card>
        ))}
      </section>
      <FeatureWidget appearance="standard" id="expenses-detail-business-purpose" title="Business purpose"><Card as="section" appearance="standard" className="ui-panel">

        <p>
          {String(
            expense.business_purpose ||
              "Complete the purpose before submission.",
          )}
        </p>
        {expense.rejection_reason && (
          <p role="note">
            <strong>Review note:</strong> {String(expense.rejection_reason)}
          </p>
        )}
        {expense.invoice_id &&
          canAccountExpenses(actor) &&
          !actor.branchId &&
          !actor.departmentId && (
            <UiActionLink appearance="standard" variant="secondary"

              href={`${base}/finance/invoices/${expense.invoice_id}`}
            >
              Open payable and reimbursement
            </UiActionLink>
          )}
      </Card></FeatureWidget>
      {editable && (
        <FeatureWidget appearance="standard" id="expenses-detail-expense-details" title="Expense details"><ExpenseEditor
          key={Number(expense.version)}
          base={base}
          enableSubmit={false}
          options={options}
          expense={expense}
        /></FeatureWidget>
      )}

      {staffOptions.canRepair &&
        (expense.status === "submitted" ||
          !!expense.approval_blocked_reason) && (
          <ApprovalAdminLink
            base={base}
            kind="expense"
            id={String(expense.id)}
          />
        )}

      {expense.reversal_journal_id &&
        canAccountExpenses(actor) &&
        !actor.branchId &&
        !actor.departmentId && (
          <Link
            href={`${base}/finance/journals/${expense.reversal_journal_id}`}
          >
            Open reversal journal
          </Link>
        )}
      <FeatureWidget appearance="standard" id="expenses-detail-reimbursements" title="Reimbursements"><Card as="section" appearance="standard" className="ui-panel">

        {payments.length === 0 ? (
          <p className="muted">No reimbursements recorded.</p>
        ) : (
          payments.map((p) => (
            <div className="ui-record-item" key={String(p.id)}>
              <div className="row-main">
                <strong>{String(p.payment_number)}</strong>
                <span className="muted"> · {String(p.payment_date)}</span>
              </div>
              <strong>
                <MoneyAmount units={String(p.amount_minor)} currency={String(p.currency)} date={String(p.payment_date)} legacyScale={2} />
              </strong>
            </div>
          ))
        )}
      </Card></FeatureWidget>
      <FlowPanel flow={flow} actor={actor} kind="expense" id={id} base={base}/>
    </WidgetGrid></RecordSections></FinanceRecordWorkspace></FeaturePage>
  );
}
