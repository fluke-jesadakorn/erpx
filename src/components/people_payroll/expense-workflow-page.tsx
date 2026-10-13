import { RecordSections } from "@/components/workplace/record-sections";
import { InlineNotice } from "@/components/ui/foundation";
import { TimeText } from "@/components/people_payroll/time/time-preference-context";
import { FlowPanel } from "@/components/workplace/flow-panel";
import { MoneyAmount } from "@/components/finance/money/currency-provider";
import { ExpensePostingPreview } from "@/components/people_payroll/expense-posting-preview";
import styles from "@/components/people_payroll/expense.module.css";
import { canManagePeople } from "@/server/modules/people_payroll/public/people-directory";
import { administrationAccess } from "@/server/modules/runtime/public/administration";
import { RecordLink as Link } from "@/components/records/record-link";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import {
  canEditExpense,
  type expenseDetail,
  type expenseOptions,
} from "@/server/modules/people_payroll/expense";
import { expenseStageLabels, type ExpenseStage } from "@/lib/people_payroll/expense-workflow";
import { ExpenseEditor, ExpenseReceipts } from "@/components/records/loaders/modules--expense-controls";
import {
  ExpenseAccounting,
  ExpensePayments,
  ExpenseStageActions,
} from "@/components/records/loaders/modules--expense-workflow-controls";
import { ApprovalHistory } from "@/components/workflow/approval-history";
import { ApprovalAdminLink } from "@/components/workflow/approval-admin-link";
import { StatusBadge } from "@/components/ui/primitives";
import { ExpenseAiPanel } from "@/components/records/loaders/modules--expense-ai-panel";
import { FinanceLegacyHeading } from "@/components/records/loaders/finance--finance-record-workspace";
import { Stack, Card } from "@/components/ui/foundation";

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
  const profile = !canManagePeople(actor) ? `${base}/profiles/${encodeURIComponent(String(expense.claimant_user_id))}` : actor.beta
    ? `${base}/people?member=${encodeURIComponent(String(expense.claimant_user_id))}`
    : expense.employee_id
      ? `${base}/people/${expense.employee_id}`
      : `${base}/people?member=${encodeURIComponent(String(expense.claimant_user_id))}`;
  const accounting = JSON.parse(String(expense.accounting_json ?? "{}")) as {
    entryDate?: string;
    note?: string;
  };
  return (
    <Stack as="div" className="ui-stack" data-source="live">
      <FinanceLegacyHeading><Link className="back-link" href={`${base}/expenses`}>
        ← Expense claims
      </Link>
      <header className="ui-section-header">
        <div>
          <p className="eyebrow">{expense.request_kind==="payment"?"Payment request":"Expense claim"}</p>
          <h1>{String(expense.merchant || "New expense claim")}</h1>
          <p className="muted">
            {String(expense.legal_name)}
            {expense.expense_date ? ` · ${expense.expense_date}` : ""}
          </p>
        </div>
        <StatusBadge appearance="standard" status={String(expense.status)} />
      </header></FinanceLegacyHeading>
      <RecordSections evidence={<ExpenseReceipts
        base={base}
        expense={expense}
        attachments={attachments}
        editable={editable}
      />} history={<details className="ui-panel">
        <summary>Completed stages and history</summary>
        <ApprovalHistory approval={approval} />
        {history.map((a) => (
          <p key={String(a.id)}>
            {String(a.action).replaceAll(".", " ")} · {String(a.actor_name)} ·{" "}
            <TimeText appearance="standard" value={Number(a.created_at)} />
          </p>
        ))}
      </details>}>
      {!!expense.spending_template_version&&<p className="muted">Submitted with company spending template version {String(expense.spending_template_version)}.</p>}
      <FlowPanel actor={actor} kind="expense" id={String(expense.id)} base={base}/>
      {["accounts_approval","gl_posting","payment","completed"].includes(stage) && <ExpensePostingPreview actor={actor} expense={expense} base={base}/>}
      {stage !== "draft" && <Card as="section" appearance="standard" className="ui-panel">
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
        {expense.amount_minor != null && expense.tax_minor != null && (
          <p>
            <strong>
              Claim total: <MoneyAmount units={(BigInt(String(expense.amount_minor)) + BigInt(String(expense.tax_minor))).toString()} currency={String(expense.currency)} date={String(expense.expense_date)} legacyScale={2} />
            </strong>
            {expense.outstanding_minor !== null &&
            expense.outstanding_minor !== undefined
              ? <> · Remaining: <MoneyAmount units={String(expense.outstanding_minor)} currency={String(expense.currency)} date={String(expense.expense_date)} legacyScale={2} /></>
              : ""}
          </p>
        )}
        {expense.rejection_reason && (
          <p className="notice">
            Review note: {String(expense.rejection_reason)}
          </p>
        )}
        {missing.length > 0 && (
          <InlineNotice tone="info" className="notice" role="status">
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
          </InlineNotice>
        )}
        {!missing.length && expense.approval_blocked_reason && (
          <InlineNotice tone="info" className="notice" role="status">
            {String(expense.approval_blocked_reason)}
          </InlineNotice>
        )}
        {canRepair && expense.approval_blocked_reason && (
          <ApprovalAdminLink
            base={base}
            kind="expense"
            id={String(expense.id)}
          />
        )}
      </Card>}
      {stage === "draft" && expense.rejection_reason && <p className="notice">Review note: {String(expense.rejection_reason)}</p>}
      <div className={editable ? styles.claimGrid : undefined}>
      <div className={styles.evidenceColumn}>
      {!editable && !["payment", "completed", "cancelled"].includes(stage) && (
        <ExpenseAiPanel compact
          base={base}
          expenseId={String(expense.id)}
          canManageAi={aiAdmin}
          evidenceKey={attachments.map(a => String(a.id)).sort().join(":")}
          version={Number(expense.version)}
        />
      )}
      </div>
      {editable ? (
        <ExpenseEditor base={base} options={options} expense={expense} hasReceipt={attachments.length > 0} receiptAi={{ canManageAi: aiAdmin, evidenceKey: attachments.map(a => String(a.id)).sort().join(":") }} />
      ) : (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Claim details</h2>
          <p>{String(expense.business_purpose)}</p>
          <dl>
            <dt>Expense category</dt>
            <dd>{String(expense.category_name)}</dd>
            <dt>Net amount</dt>
            <dd>{expense.amount_minor == null ? "Not provided" : <MoneyAmount units={String(expense.amount_minor)} currency={String(expense.currency)} date={String(expense.expense_date)} legacyScale={2} />}</dd>
            <dt>Recoverable tax</dt>
            <dd>{expense.tax_minor == null ? "Not provided" : <MoneyAmount units={String(expense.tax_minor)} currency={String(expense.currency)} date={String(expense.expense_date)} legacyScale={2} />}</dd>
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
        </Card>
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
        actions={editable ? permittedActions.filter(action => action !== "submit") : permittedActions}
        missing={missing}
        profileHref={profile}
        isEditable={false}
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
      </RecordSections>
    </Stack>
  );
}
