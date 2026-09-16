import { FlowPanel } from "@/components/workplace/flow-panel";
import { ExpensePostingPreview } from "./expense-posting-preview";
import styles from "./expense.module.css";
import { administrationAccess } from "@/server/administration";
import Link from "next/link";
import type { WorkContext } from "@/server/authorization";
import {
  canEditExpense,
  type expenseDetail,
  type expenseOptions,
} from "@/server/expenses";
import { expenseStageLabels, type ExpenseStage } from "@/lib/expense-workflow";
import { minorToDecimal } from "@/lib/finance-money";
import { ExpenseEditor, ExpenseReceipts } from "./expense-controls";
import {
  ExpenseAccounting,
  ExpensePayments,
  ExpenseStageActions,
} from "./expense-workflow-controls";
import { ApprovalHistory } from "./approval-history";
import { ApprovalAdminLink } from "./approval-admin-link";
import { StatusBadge } from "@/components/ui/primitives";
import { ExpenseAiPanel } from "./expense-ai-panel";

export async function ExpenseWorkflowPage({
  base,
  actor,
  data,
  options,
  canRepair,
}: {
  base: string;
  actor: WorkContext;
  data: Awaited<ReturnType<typeof expenseDetail>>;
  options: Awaited<ReturnType<typeof expenseOptions>>;
  canRepair: boolean;
}) {
  const {
    expense,
    attachments,
    approval,
    history,
    paymentDrafts,
    paymentAttachments,
    permittedActions,
  } = data;
  const aiAdmin = (await administrationAccess(actor)).manageAiProviders;
  const editable = canEditExpense(actor, expense),
    stage = String(expense.workflow_stage) as ExpenseStage;
  const missing = JSON.parse(
    String(expense.setup_requirements_json ?? "[]"),
  ) as string[];
  const profile = actor.beta
    ? `${base}/people?member=${encodeURIComponent(String(expense.claimant_user_id))}`
    : expense.employee_id
      ? `${base}/people/${expense.employee_id}`
      : `${base}/people?member=${encodeURIComponent(String(expense.claimant_user_id))}`;
  const accounting = JSON.parse(String(expense.accounting_json ?? "{}")) as {
    entryDate?: string;
    note?: string;
  };
  return (
    <main className="content-stack" data-source="live">
      <Link className="back-link" href={`${base}/work?type=expense`}>
        ← Work · Expense claims
      </Link>
      <header className="section-head">
        <div>
          <p className="eyebrow">{expense.request_kind==="payment"?"Payment request":"Expense claim"}</p>
          <h1>{String(expense.merchant || "New expense claim")}</h1>
          <p className="muted">
            {String(expense.legal_name)}
            {expense.expense_date ? ` · ${expense.expense_date}` : ""}
          </p>
        </div>
        <StatusBadge status={String(expense.status)} />
      </header>
      {!!expense.spending_template_version&&<p className="muted">Submitted with company spending template version {String(expense.spending_template_version)}.</p>}
      <FlowPanel actor={actor} kind="expense" id={String(expense.id)} base={base}/>
      {["accounts_approval","gl_posting","payment","completed"].includes(stage) && <ExpensePostingPreview actor={actor} expense={expense} base={base}/>}
      {stage !== "draft" && <section className="card">
        <p className="eyebrow">Current stage</p>
        <h2>{expenseStageLabels[stage]}</h2>
        <p>
          {stage === "payment"
              ? "An independent Treasury operator records each bank payment and its slip."
              : stage === "completed"
                ? "The claim has been paid in full."
                : stage === "cancelled"
                  ? "This claim is cancelled."
                  : `Responsible queue: ${String(expense.current_approver_name ?? expenseStageLabels[stage])}.`}
        </p>
        {expense.amount_minor && (
          <p>
            <strong>
              Claim total: {String(expense.currency)}{" "}
              {minorToDecimal(
                Number(expense.amount_minor) + Number(expense.tax_minor),
              )}
            </strong>
            {expense.outstanding_minor !== null &&
            expense.outstanding_minor !== undefined
              ? ` · Remaining: ${minorToDecimal(Number(expense.outstanding_minor))}`
              : ""}
          </p>
        )}
        {expense.rejection_reason && (
          <p className="notice">
            Review note: {String(expense.rejection_reason)}
          </p>
        )}
        {missing.length > 0 && (
          <div className="notice" role="status">
            <strong>Before you submit</strong>
            <ul>
              {missing.map((m) => (
                <li key={m}>{m}</li>
              ))}
            </ul>
            <Link href={profile}>Open claimant profile in People</Link>
            <p className="muted">
              An owner or HR administrator can complete organization setup. Your
              draft is saved.
            </p>
          </div>
        )}
        {!missing.length && expense.approval_blocked_reason && (
          <p className="notice" role="status">
            {String(expense.approval_blocked_reason)}
          </p>
        )}
        {canRepair && expense.approval_blocked_reason && (
          <ApprovalAdminLink
            base={base}
            kind="expense"
            id={String(expense.id)}
          />
        )}
      </section>}
      {stage === "draft" && expense.rejection_reason && <p className="notice">Review note: {String(expense.rejection_reason)}</p>}
      <div className={editable ? styles.claimGrid : undefined}>
      <div className={styles.evidenceColumn}>
      <ExpenseReceipts
        base={base}
        expense={expense}
        attachments={attachments}
        editable={editable}
      />
      {!editable && !["payment", "completed", "cancelled"].includes(stage) && (
        <ExpenseAiPanel
          base={base}
          expenseId={String(expense.id)}
          canManageAi={aiAdmin}
          evidenceKey={attachments.map(a => String(a.id)).sort().join(":")}
          version={Number(expense.version)}
        />
      )}
      </div>
      {editable ? (
        <ExpenseEditor base={base} options={options} expense={expense} receiptAi={{ canManageAi: aiAdmin, evidenceKey: attachments.map(a => String(a.id)).sort().join(":") }} />
      ) : (
        <section className="card">
          <h2>Claim details</h2>
          <p>{String(expense.business_purpose)}</p>
          <dl>
            <dt>Expense category</dt>
            <dd>{String(expense.category_name)}</dd>
            <dt>Net amount</dt>
            <dd>{minorToDecimal(Number(expense.amount_minor))}</dd>
            <dt>Recoverable tax</dt>
            <dd>{minorToDecimal(Number(expense.tax_minor))}</dd>
            <dt>Receipt number</dt>
            <dd>{String(expense.receipt_reference)}</dd>
            {accounting.entryDate && (
              <>
                <dt>Posting date</dt>
                <dd>{accounting.entryDate}</dd>
              </>
            )}
          </dl>
          {accounting.note && <p>{accounting.note}</p>}
        </section>
      )}
      </div>
      {stage === "accounts_review" && expense.can_decide === 1 && (
        <ExpenseAccounting
          key={Number(expense.version)}
          base={base}
          expense={expense}
          options={options}
        />
      )}
      <ExpenseStageActions
        viewerId={actor.userId}
        base={base}
        expense={expense}
        actions={permittedActions}
        missing={missing}
        profileHref={profile}
        isEditable={canEditExpense(actor, expense)}
      />
      {["payment", "completed"].includes(stage) && (
        <ExpensePayments
          viewerId={actor.userId}
          base={base}
          expense={expense}
          drafts={paymentDrafts}
          attachments={paymentAttachments}
        />
      )}
      <details className="card">
        <summary>Completed stages and history</summary>
        <ApprovalHistory approval={approval} />
        {history.map((a) => (
          <p key={String(a.id)}>
            {String(a.action).replaceAll(".", " ")} · {String(a.actor_name)} ·{" "}
            {new Date(Number(a.created_at))
              .toISOString()
              .slice(0, 16)
              .replace("T", " ")}{" "}
            UTC
          </p>
        ))}
      </details>
    </main>
  );
}
