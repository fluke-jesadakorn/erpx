"use client";
import {ExpensePaymentCancellation} from './expense-payment-cancellation';
import { ReviewDecisionForm } from "@/components/workplace/review-decision";
import { InlineNotice } from "@/components/ui/foundation";

import { ActionAnchor as UiActionAnchor } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import styles from "@/components/people_payroll/expense.module.css";
import workflow from "@/components/people_payroll/expense-workflow.module.css";
import { useEffect, useState, type ReactNode } from "react";
import {
  AlertCircle,
  ArrowRight,
  CheckCircle2,
  Send,
  ShieldCheck,
  XCircle,
} from "lucide-react";
import { useHydrated } from "@/components/ui/use-hydrated";
import { minorToDecimal } from "@/lib/finance/finance-money";
import type { ExpenseRow as Row } from "@/server/modules/people_payroll/expense";
import { ExpenseReceipts, useExpenseCommand } from "@/components/people_payroll/expense-controls";
import { ExpenseAiPanel } from "@/components/people_payroll/expense-ai-panel";
import { useExpenseDraft } from "@/components/people_payroll/use-expense-draft";
import type { ExpenseAiSuggestionEvent } from "@/lib/people_payroll/expense-ai";
import { expenseStages, expenseStageLabels, type ExpenseStage } from "@/lib/people_payroll/expense-workflow";
import { Card, FormGrid } from "@/components/ui/foundation";

function useAiFields<T extends Record<string, string>>(
  key: string,
  initial: T,
  expenseId: string,
  paymentId: string | null,
) {
  const draft = useExpenseDraft(key, initial);
  useEffect(() => {
    const receive = (event: Event) => {
      const detail = (event as CustomEvent<ExpenseAiSuggestionEvent>).detail;
      if (detail.expenseId !== expenseId || detail.paymentId !== paymentId) return;
      draft.receiveAi(detail);
    };
    window.addEventListener("erpx:expense-ai", receive);
    return () => window.removeEventListener("erpx:expense-ai", receive);
  });
  const input = (name: keyof T) => ({
    value: draft.fields[name],
    onChange: (
      event: React.ChangeEvent<HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement>,
    ) => draft.update((previous) => ({ ...previous, [name]: event.target.value })),
  });
  return { ...draft, input };
}

export function ExpenseStageActions({
  base,
  expense,
  actions,
  viewerId,
  beforeSubmit,
  missing = [],
  profileHref,
  isEditable = false,
}: {
  viewerId: string;
  base: string;
  expense: Row;
  actions: string[];
  beforeSubmit?: ReactNode;
  missing?: string[];
  profileHref?: string;
  isEditable?: boolean;
}) {
  const { send, pending, error } = useExpenseCommand(base),
    hydrated = useHydrated();
  const note = useExpenseDraft(
    `erpx:decision:${base}:${viewerId}:${expense.id}:${expense.workflow_stage}`,
    { reason: "" },
  );
  const reason = note.fields.reason;
  const [unsaved, setUnsaved] = useState("");
  async function transition(action: string) {
    setUnsaved("");
    if (action === "submit" || action === "approve") {
      const kind = expense.workflow_stage === "draft" ? "expense" : "accounting";
      try {
        const raw = sessionStorage.getItem(`erpx:${kind}:${base}:${viewerId}:${expense.id}`);
        if (raw) {
          const draft = JSON.parse(raw) as {
            fields: unknown;
            baseline: string;
          };
          if (JSON.stringify(draft.fields) !== draft.baseline) {
            setUnsaved("Save your edited fields before confirming this step.");
            return;
          }
        }
      } catch {}
    }
    if (
      await send(`/${expense.id}/${action}`, {
        expectedVersion: expense.version,
        ...(action === "submit" ? {} : { reason }),
      })
    )
      note.clear();
  }
  const accounting = JSON.parse(String(expense.accounting_json ?? "{}")) as {
    periodId?: string;
  };
  const stage = String(expense.workflow_stage ?? expense.status) as ExpenseStage;
  const approvalLabel =
    expense.workflow_stage === "accounts_approval"
      ? "Approve accounting → GL review"
      : expense.workflow_stage === "accounts_review"
        ? "Confirm Accounts review"
        : "Approve department review";
  const canSubmit = isEditable || actions.includes("submit");
  const canApprove = actions.includes("approve");
  const canReject = actions.includes("reject");
  const canCancel = actions.includes("cancel");
  const cancelOnly = canCancel && !canApprove && !canReject;
  const showReason = canApprove || canReject || canCancel;
  const reasonLen = reason.trim().length;
  const needsReason = showReason && reasonLen < 3;
  const needsAccounting =
    canApprove && expense.workflow_stage === "accounts_review" && !accounting.periodId;
  const blocked = missing.length > 0;
  const total = minorToDecimal(Number(expense.amount_minor ?? 0) + Number(expense.tax_minor ?? 0));
  const visibleSteps = expenseStages.filter((s) => !["cancelled", "completed"].includes(s));
  const currentIdx = Math.max(0, visibleSteps.indexOf(stage as (typeof visibleSteps)[number]));
  if (!canSubmit && !canApprove && !canReject && !canCancel && !blocked && !beforeSubmit)
    return null;
  const roleText = canApprove
    ? "You are the reviewer for this stage. Verify evidence, then approve or return."
    : canSubmit
      ? "You are the claimant. Complete the checklist, then submit for review."
      : "Review the claim and its evidence. Your confirmation records the decision.";
  return (
    <section className={`card ${workflow.nextAction}`} aria-label="Your next action">
      <div className={workflow.nextHead}>
        <div>
          <p className="eyebrow">
            Your next action · {expenseStageLabels[stage] ?? String(expense.workflow_stage)}
          </p>
          <h2>
            {canApprove
              ? approvalLabel
              : canSubmit && blocked
                ? `Complete ${missing.length} ${missing.length === 1 ? "item" : "items"} to submit`
                : canSubmit
                  ? "Submit for department approval"
                  : "Review and decide"}
          </h2>
          <p className={workflow.nextSub}>{roleText}</p>
        </div>
        {expense.currency && (
          <div className={workflow.nextTotal}>
            <span>Claim total</span>
            <strong>
              {String(expense.currency)} {total}
            </strong>
            <small>{String(expense.merchant || "New claim")}</small>
          </div>
        )}
      </div>

      <ol className={workflow.stepper} aria-label="Approval progress">
        {visibleSteps.slice(0, 5).map((s, i) => (
          <li
            key={s}
            aria-current={i === currentIdx ? "step" : undefined}
            data-state={i < currentIdx ? "done" : i === currentIdx ? "now" : "todo"}
          >
            <span className={workflow.stepMarker} aria-hidden="true">
              {i < currentIdx ? <CheckCircle2 size={17} /> : i + 1}
            </span>
            <span>{expenseStageLabels[s as ExpenseStage]}</span>
          </li>
        ))}
      </ol>

      {(error || unsaved) && (
        <InlineNotice tone="danger" className={`error ${workflow.nextAlert}`} role="alert">
          <AlertCircle size={15} /> {error || unsaved}
        </InlineNotice>
      )}

      {blocked && (
        <InlineNotice tone="info" className={workflow.checklist} role="status">
          <div className={workflow.checklistHeading}>
            <strong>
              <AlertCircle size={18} aria-hidden="true" /> Before you submit
            </strong>
            <span>
              {missing.length} {missing.length === 1 ? "item" : "items"} remaining
            </span>
          </div>
          <ul>
            {missing.map((m, index) => (
              <li key={m}>
                <span className={workflow.checkNumber} aria-hidden="true">
                  {index + 1}
                </span>
                <span>{m}</span>
              </li>
            ))}
          </ul>
          <div className={workflow.checklistActions}>
            {profileHref && (
              <UiActionAnchor appearance="standard" variant="secondary" href={profileHref}>
                Open claimant profile <ArrowRight size={14} />
              </UiActionAnchor>
            )}
            <span>Complete claim details and save your draft. Ask Company HR to resolve employee, department or policy setup.</span>
          </div>
        </InlineNotice>
      )}
      {beforeSubmit}

      {!blocked && expense.approval_blocked_reason && (
        <InlineNotice tone="info" className="notice" role="status">
          {String(expense.approval_blocked_reason)}
        </InlineNotice>
      )}

      {needsAccounting && (
        <InlineNotice tone="info" className={`notice ${workflow.nextAlert}`} role="status">
          <ShieldCheck size={15} /> Select an accounting period and save the accounting preparation
          before confirming Accounts review.
        </InlineNotice>
      )}

      {canCancel && !cancelOnly && (
        <Field appearance="standard" className={workflow.reason}>
          <span>
            Decision reason{" "}
            <small>
              {reasonLen}/2000 · min 3 characters
              {canCancel && !canApprove
                ? " · required to cancel"
                : " · required to approve, return or cancel"}
            </small>
          </span>
          <TextArea appearance="standard"
            value={reason}
            disabled={!hydrated || pending}
            onChange={(e) => note.update(() => ({ reason: e.target.value }))}
            maxLength={2000}
            placeholder={
              canApprove
                ? "e.g. Purpose, amount and receipt verified against policy…"
                : "e.g. Tell the claimant what to fix…"
            }
          />
          {needsReason && (
            <small className={workflow.hint}>
              Add at least 3 characters to enable{" "}
              {[canApprove && "Approve", canReject && "Return", canCancel && "Cancel"]
                .filter(Boolean)
                .join(" / ")}
              .
            </small>
          )}
        </Field>
      )}

      <div className={workflow.actions}>
        <span>
          {canApprove
            ? "Approval records your name, time and version."
            : canSubmit && blocked
              ? "Submit unlocks once the checklist is clear."
              : "Decisions are timestamped and auditable."}
        </span>
        {canSubmit && (
          <Button appearance="standard"
            type="submit"
            variant="primary"

            disabled={!hydrated || pending || blocked}
            title={
              blocked
                ? `Complete ${missing.length} items before submitting`
                : "Submit for department approval"
            }
            onClick={() => transition("submit")}
          >
            <Send size={15} /> Submit for department approval
          </Button>
        )}
        {canCancel && !cancelOnly && (
          <Button appearance="standard"
            type="submit"
            variant="secondary"

            disabled={!hydrated || pending || needsReason}
            onClick={() => transition("cancel")}
          >
            <XCircle size={15} /> Cancel claim
          </Button>
        )}
      </div>
      {(canApprove||canReject)&&<ReviewDecisionForm actions={[...(canApprove?["approve" as const]:[]),...(canReject?["reject" as const]:[])]} busy={!hydrated||pending||!!needsAccounting} note={reason} onNote={reason=>note.update(()=>({reason}))} onConfirm={decision=>void transition(decision)}/>}
      {cancelOnly && (
        <details className={workflow.cancelSection}>
          <summary>Need to cancel this claim?</summary>
          <div className={workflow.cancelBody}>
            <Field appearance="standard" className={workflow.reason}>
              <span>
                Cancellation reason <small>{reasonLen}/2000</small>
              </span>
              <TextArea appearance="standard"
                value={reason}
                disabled={!hydrated || pending}
                onChange={(e) => note.update(() => ({ reason: e.target.value }))}
                maxLength={2000}
                placeholder="Explain why this claim is no longer needed…"
              />
              <small>Enter at least 3 characters to cancel this claim.</small>
            </Field>
            <Button appearance="standard"
              type="submit"
              variant="ghost"
              className={`button secondary ${workflow.cancelButton}`}
              disabled={!hydrated || pending || needsReason}
              onClick={() => transition("cancel")}
            >
              <XCircle size={15} /> Cancel claim
            </Button>
          </div>
        </details>
      )}
    </section>
  );
}
export function ExpenseAccounting({
  base,
  expense,
  options,
}: {
  base: string;
  expense: Row;
  options: { accounts: Row[]; periods: Row[]; selfUserId?: string };
}) {
  const { send, pending, error } = useExpenseCommand(base),
    hydrated = useHydrated();
  const saved = JSON.parse(String(expense.accounting_json ?? "{}")) as {
    entryDate?: string;
    periodId?: string;
    note?: string;
  };
  const form = useAiFields(
    `erpx:accounting:${base}:${options.selfUserId}:${expense.id}`,
    {
      categoryAccountId: String(expense.category_account_id ?? ""),
      tax: minorToDecimal(Number(expense.tax_minor)),
      entryDate: saved.entryDate ?? new Date().toISOString().slice(0, 10),
      periodId: saved.periodId ?? "",
      note: saved.note ?? "",
    },
    String(expense.id),
    null,
  );
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>Accounting preparation</h2>
      <p>
        Classify the expense and recoverable tax. The claim total stays {String(expense.currency)}{" "}
        {minorToDecimal(Number(expense.amount_minor) + Number(expense.tax_minor))}.
      </p>
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      {form.notice}
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          if (form.conflict) return;
          if (
            await send(`/${expense.id}/accounting`, {
              expectedVersion: expense.version,
              ...form.fields,
            })
          )
            form.clear();
        }}
      >
        <fieldset disabled={!hydrated || pending} className={styles.fields}>
          <FormGrid as="div" className="ui-form-grid">
            <Field appearance="standard">
              Expense category
              <Select appearance="standard" name="categoryAccountId" required {...form.input("categoryAccountId")}>
                {options.accounts.map((a) => (
                  <option key={String(a.id)} value={String(a.id)}>
                    {String(a.name)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Recoverable tax
              <TextInput appearance="standard"
                name="tax"
                type="number"
                step="0.01"
                min="0"
                required
                {...form.input("tax")}
              />
            </Field>
            <Field appearance="standard">
              Posting date
              <TextInput appearance="standard"
                name="entryDate"
                type="date"
                min={String(expense.expense_date)}
                {...form.input("entryDate")}
                required
              />
            </Field>
            <Field appearance="standard">
              Accounting period
              <Select appearance="standard" name="periodId" required {...form.input("periodId")}>
                <option value="">Select an open period</option>
                {options.periods.map((p) => (
                  <option key={String(p.id)} value={String(p.id)}>
                    {String(p.name)}
                  </option>
                ))}
              </Select>
            </Field>
          </FormGrid>
          <Field appearance="standard">
            Accounting note
            <TextArea appearance="standard" name="note" maxLength={2000} {...form.input("note")} />
          </Field>
          <Button appearance="standard"
            clientManaged type="submit"
            variant="secondary"

            disabled={pending || !hydrated || form.conflict}
          >
            Save accounting preparation
          </Button>
        </fieldset>
      </form>
    </Card>
  );
}
function PaymentPreparation({
  base,
  expense,
  draft,
  attachments,
  viewerId,
}: {
  viewerId: string;
  base: string;
  expense: Row;
  draft: Row;
  attachments: Row[];
}) {
  const form = useAiFields(
    `erpx:payment:${base}:${viewerId}:${draft.id}`,
    {
      date: String(draft.payment_date ?? ""),
      amount: draft.amount_minor ? minorToDecimal(Number(draft.amount_minor)) : "",
      reference: String(draft.reference ?? ""),
    },
    String(expense.id),
    String(draft.id),
  );
  const { send, pending, error } = useExpenseCommand(base),
    hydrated = useHydrated(),
    [confirmed, setConfirmed] = useState(false);
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h3>Record a bank payment</h3>
      <p>Make the payment through your bank, then review the slip and confirm the amount here.</p>
      <ExpenseReceipts
        base={base}
        expense={expense}
        payment={draft}
        attachments={attachments}
        editable
      />
      <ExpenseAiPanel
        base={base}
        expenseId={String(expense.id)}
        version={Number(draft.version)}
        paymentId={String(draft.id)}
      />
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      {form.notice}
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          if (form.conflict) return;
          const intent = (e.nativeEvent as SubmitEvent).submitter?.getAttribute("value");
          if (
            await send(
              `/${expense.id}/payments/${draft.id}${intent === "confirm" ? "/confirm" : ""}`,
              { expectedVersion: draft.version, ...form.fields },
            )
          )
            form.clear();
        }}
      >
        <fieldset disabled={!hydrated || pending} className={styles.fields}>
          <FormGrid as="div" className="ui-form-grid">
            <Field appearance="standard">
              Payment date
              <TextInput appearance="standard" name="date" type="date" required {...form.input("date")} />
            </Field>
            <Field appearance="standard">
              Amount paid ({String(expense.currency)})
              <TextInput appearance="standard"
                name="amount"
                type="number"
                step="0.01"
                min="0.01"
                max={minorToDecimal(Number(expense.outstanding_minor))}
                required
                {...form.input("amount")}
              />
            </Field>
            <Field appearance="standard">
              Bank reference
              <TextInput appearance="standard"
                name="reference"
                maxLength={100}
                minLength={2}
                required
                {...form.input("reference")}
              />
            </Field>
          </FormGrid>
          <Field appearance="standard" orientation="inline" className="check-row">
            <TextInput appearance="standard"
              type="checkbox"
              checked={confirmed}
              onChange={(e) => setConfirmed(e.target.checked)}
            />
            I made this payment and checked the amount, recipient and real bank slip.
          </Field>
          <div className="ui-form-actions">
            <Button appearance="standard"
              clientManaged type="submit"
              variant="secondary"

              value="save"
              disabled={!hydrated || pending || form.conflict}
            >
              Save payment preparation
            </Button>
            <Button appearance="standard"
              clientManaged type="submit"
              variant="primary"

              value="confirm"
              disabled={!hydrated || pending || form.conflict || !confirmed || !attachments.length}
            >
              Confirm bank payment
            </Button>
          </div>
        </fieldset>
      </form>
    </Card>
  );
}
export function ExpensePayments({
  base,
  expense,
  drafts,
  attachments,
  viewerId,
}: {
  viewerId: string;
  base: string;
  expense: Row;
  drafts: Row[];
  attachments: Row[];
}) {
  const { send, pending, error } = useExpenseCommand(base),
    hydrated = useHydrated();
  const open = drafts.filter((d) => d.status === "draft");
  return (
    <section>
      <h2>Payments</h2>
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      {drafts
        .filter((d) => d.status === "confirmed")
        .map((d) => (
          <Card as="article" appearance="standard" className="ui-panel" key={String(d.id)}>
            <strong>
              {String(d.payment_date)} · {String(expense.currency)}{" "}
              {minorToDecimal(Number(d.amount_minor))}
            </strong>
            <p>Bank reference: {String(d.reference)}</p>
            {attachments
              .filter((a) => a.owner_id === d.id)
              .map((a) => (
                <p key={String(a.id)}>
                  <a href={`/api/v1${base}/attachments/${a.id}/content`}>
                    {String(a.original_name)}
                  </a>
                </p>
              ))}
          </Card>
        ))}
      {drafts.filter(draft=>draft.status==='cancelled').map(draft=><Card as="article" appearance="standard" className="ui-panel" key={`cancelled-${draft.id}`}><strong>Cancelled payment preparation</strong><p>No bank payment was confirmed through this preparation.</p>{draft.cancellation_reason&&<p>{String(draft.cancellation_reason)}</p>}{attachments.filter(file=>file.owner_id===draft.id).map(file=><p key={String(file.id)}><a href={`/api/v1${base}/attachments/${file.id}/content`}>{String(file.original_name)}</a></p>)}</Card>)}
      {drafts.map(draft=><ExpensePaymentCancellation key={`cancel-${draft.id}`} base={base} expense={expense} draft={draft}/>)}
      {expense.can_pay === 1 &&
        open.map((d) => (
          <PaymentPreparation
            viewerId={viewerId}
            key={`${d.id}:${d.version}`}
            base={base}
            expense={expense}
            draft={d}
            attachments={attachments.filter((a) => a.owner_id === d.id)}
          />
        ))}
      {expense.can_pay === 1 && !open.length && (
        <Button appearance="standard"
          type="submit"
          variant="primary"

          disabled={!hydrated || pending}
          onClick={() =>
            void send(`/${expense.id}/payments`, {
              expectedVersion: expense.version,
            })
          }
        >
          Record bank payment
        </Button>
      )}
      {!drafts.length && expense.can_pay !== 1 && <p className="muted">No payments recorded.</p>}
    </section>
  );
}
