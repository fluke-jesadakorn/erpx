import { pageError } from "@/server/page-error";
import { FlowPanel } from "@/components/workplace/flow-panel";
import { ApprovalHistory } from "@/components/modules/approval-history";
import { ApprovalAdminLink } from "@/components/modules/approval-admin-link";
import { staffSettings } from "@/server/staff-settings";
import { holdingResources } from "@/server/runtime";
import Link from "next/link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import {
  expenseDetail,
  expenseOptions,
  canEditExpense,
  canReviewExpense,
  canPostExpenses,
  canAccountExpenses,
} from "@/server/expenses";
import { workspaceBase } from "@/lib/workspace";
import { minorToDecimal } from "@/lib/finance-money";
import {
  ExpenseEditor,
  ExpenseActions,
  ExpenseReceipts,
} from "@/components/modules/expense-controls";
import { StatusBadge } from "@/components/ui/primitives";
import { ExpenseWorkflowPage } from "@/components/modules/expense-workflow-page";
export const dynamic = "force-dynamic";
export default async function Expense({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params,
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
  const { write } = await holdingResources(actor.holdingId),
    staffOptions = await staffSettings(write, actor);
  if (expense.workflow_version === 2)
    return (
      <ExpenseWorkflowPage
        base={base}
        actor={actor}
        data={data}
        options={options}
        canRepair={staffOptions.canManage}
      />
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
  return (
    <main className="content-stack" data-source="live">
      <Link className="back-link" href={`${base}/expenses`}>
        ← Employee expenses
      </Link>
      <div className="section-head">
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
        <StatusBadge status={String(expense.status)} />
      </div>
      <section className="card-grid">
        {[
          ["Net amount", expense.amount_minor],
          ["Recoverable tax", expense.tax_minor],
          [
            "Claim total",
            Number(expense.amount_minor) + Number(expense.tax_minor),
          ],
          [
            "Outstanding",
            expense.cancelled_at
              ? 0
              : (expense.outstanding_minor ??
                Number(expense.amount_minor) + Number(expense.tax_minor)),
          ],
        ].map(([label, value]) => (
          <article className="card" key={String(label)}>
            <span className="eyebrow">{String(label)}</span>
            <strong>
              {String(expense.currency)} {minorToDecimal(Number(value))}
            </strong>
          </article>
        ))}
      </section>
      <section className="card">
        <h2>Business purpose</h2>
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
            <Link
              className="button secondary"
              href={`${base}/finance/invoices/${expense.invoice_id}`}
            >
              Open payable and reimbursement
            </Link>
          )}
      </section>
      {editable && (
        <ExpenseEditor
          key={Number(expense.version)}
          base={base}
          options={options}
          expense={expense}
        />
      )}
      <ExpenseReceipts
        base={base}
        expense={expense}
        attachments={attachments}
        editable={editable}
      />
      <ApprovalHistory
        approval={approval}
        blockedReason={
          expense.approval_blocked_reason
            ? String(expense.approval_blocked_reason)
            : null
        }
      />
      {staffOptions.canManage &&
        (expense.status === "submitted" ||
          !!expense.approval_blocked_reason) && (
          <ApprovalAdminLink
            base={base}
            kind="expense"
            id={String(expense.id)}
          />
        )}
      <ExpenseActions
        canCancel={permittedActions.includes("cancel")}
        base={base}
        expense={expense}
        editable={editable}
        reviewable={canReviewExpense(actor, expense)}
        canPost={posting}
        periods={options.periods}
      />
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
      <section className="card">
        <h2>Reimbursements</h2>
        {payments.length === 0 ? (
          <p className="muted">No reimbursements recorded.</p>
        ) : (
          payments.map((p) => (
            <div className="row" key={String(p.id)}>
              <div className="row-main">
                <strong>{String(p.payment_number)}</strong>
                <span className="muted"> · {String(p.payment_date)}</span>
              </div>
              <strong>
                {String(p.currency)} {minorToDecimal(Number(p.amount_minor))}
              </strong>
            </div>
          ))
        )}
      </section>
      <section className="card">
        <h2>Review decisions</h2>
        {decisions.length === 0 ? (
          <p className="muted">No review decisions yet.</p>
        ) : (
          decisions.map((d) => (
            <div className="row" key={String(d.id)}>
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
      </section>
      <section className="card">
        <h2>Activity</h2>
        {history.map((a) => (
          <div className="row" key={String(a.id)}>
            <div className="row-main">
              <strong>{String(a.action).replaceAll(".", " ")}</strong>
              <div className="muted">
                {String(a.actor_name)} ·{" "}
                {new Date(Number(a.created_at))
                  .toISOString()
                  .replace("T", " ")
                  .slice(0, 19)}{" "}
                UTC
              </div>
            </div>
          </div>
        ))}
      </section>
      <FlowPanel actor={actor} kind="expense" id={id} base={base}/>
    </main>
  );
}
