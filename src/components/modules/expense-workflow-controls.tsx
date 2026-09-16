"use client";
import styles from "./expense.module.css";
import workflow from "./expense-workflow.module.css";
import { useEffect, useState, type ReactNode } from "react";
import {
  AlertCircle,
  ArrowRight,
  CheckCircle2,
  RotateCcw,
  Send,
  ShieldCheck,
  XCircle,
} from "lucide-react";
import { useHydrated } from "@/components/ui/use-hydrated";
import { minorToDecimal } from "@/lib/finance-money";
import type { ExpenseRow as Row } from "@/server/expenses";
import { ExpenseReceipts, useExpenseCommand } from "./expense-controls";
import { ExpenseAiPanel } from "./expense-ai-panel";
import { useExpenseDraft } from "./use-expense-draft";
import type { ExpenseAiSuggestionEvent } from "@/lib/expense-ai";
import { expenseStages, expenseStageLabels, type ExpenseStage } from "@/lib/expense-workflow";

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
      if (detail.expenseId !== expenseId || detail.paymentId !== paymentId)
        return;
      draft.receiveAi(detail);
    };
    window.addEventListener("erpx:expense-ai", receive);
    return () => window.removeEventListener("erpx:expense-ai", receive);
  });
  const input = (name: keyof T) => ({
    value: draft.fields[name],
    onChange: (
      event: React.ChangeEvent<
        HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement
      >,
    ) =>
      draft.update((previous) => ({ ...previous, [name]: event.target.value })),
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
      const kind =
        expense.workflow_stage === "draft" ? "expense" : "accounting";
      try {
        const raw = sessionStorage.getItem(
          `erpx:${kind}:${base}:${viewerId}:${expense.id}`,
        );
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
    canApprove &&
    expense.workflow_stage === "accounts_review" &&
    !accounting.periodId;
  const blocked = missing.length > 0;
  const total = minorToDecimal(
    Number(expense.amount_minor ?? 0) + Number(expense.tax_minor ?? 0),
  );
  const visibleSteps = expenseStages.filter(
    (s) => !["cancelled", "completed"].includes(s),
  );
  const currentIdx = Math.max(
    0,
    visibleSteps.indexOf(stage as (typeof visibleSteps)[number]),
  );
  if (
    !canSubmit &&
    !canApprove &&
    !canReject &&
    !canCancel &&
    !blocked &&
    !beforeSubmit
  )
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
        <p className={`error ${workflow.nextAlert}`} role="alert">
          <AlertCircle size={15} /> {error || unsaved}
        </p>
      )}

      {blocked && (
        <div className={workflow.checklist} role="status">
          <div className={workflow.checklistHeading}>
            <strong><AlertCircle size={18} aria-hidden="true" /> Before you submit</strong>
            <span>{missing.length} {missing.length === 1 ? "item" : "items"} remaining</span>
          </div>
          <ul>
            {missing.map((m, index) => (
              <li key={m}>
                <span className={workflow.checkNumber} aria-hidden="true">{index + 1}</span>
                <span>{m}</span>
              </li>
            ))}
          </ul>
          <div className={workflow.checklistActions}>
            {profileHref && (
              <a className="button secondary" href={profileHref}>
                Open claimant profile in People <ArrowRight size={14} />
              </a>
            )}
            <span>Fix in Edit claim above, then Save draft.</span>
          </div>
        </div>
      )}
      {beforeSubmit}

      {!blocked && expense.approval_blocked_reason && (
        <p className="notice" role="status">
          {String(expense.approval_blocked_reason)}
        </p>
      )}

      {needsAccounting && (
        <p className={`notice ${workflow.nextAlert}`} role="status">
          <ShieldCheck size={15} /> Select an accounting period and save the
          accounting preparation before confirming Accounts review.
        </p>
      )}

      {showReason && !cancelOnly && (
        <label className={workflow.reason}>
          <span>
            Decision reason{" "}
            <small>
              {reasonLen}/2000 · min 3 characters{canCancel && !canApprove ? " · required to cancel" : " · required to approve, return or cancel"}
            </small>
          </span>
          <textarea
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
        </label>
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
          <button
            className="button"
            disabled={!hydrated || pending || blocked}
            title={blocked ? `Complete ${missing.length} items before submitting` : "Submit for department approval"}
            onClick={() => transition("submit")}
          >
            <Send size={15} /> Submit for department approval
          </button>
        )}
        {canApprove && (
          <button
            className="button"
            disabled={!hydrated || pending || needsReason || !!needsAccounting}
            title={
              needsReason
                ? "Add a decision reason (min 3 characters)"
                : needsAccounting
                  ? "Save accounting preparation first"
                  : approvalLabel
            }
            onClick={() => transition("approve")}
          >
            <ShieldCheck size={15} /> {approvalLabel}
          </button>
        )}
        {canReject && (
          <button
            className="button secondary"
            disabled={!hydrated || pending || needsReason}
            onClick={() => transition("reject")}
          >
            <RotateCcw size={15} /> Return for correction
          </button>
        )}
        {canCancel && !cancelOnly && (
          <button
            className="button secondary"
            disabled={!hydrated || pending || needsReason}
            onClick={() => transition("cancel")}
          >
            <XCircle size={15} /> Cancel claim
          </button>
        )}
      </div>
      {cancelOnly && (
        <details className={workflow.cancelSection}>
          <summary>Need to cancel this claim?</summary>
          <div className={workflow.cancelBody}>
            <label className={workflow.reason}>
              <span>Cancellation reason <small>{reasonLen}/2000</small></span>
              <textarea
                value={reason}
                disabled={!hydrated || pending}
                onChange={(e) => note.update(() => ({ reason: e.target.value }))}
                maxLength={2000}
                placeholder="Explain why this claim is no longer needed…"
              />
              <small>Enter at least 3 characters to cancel this claim.</small>
            </label>
            <button
              className={`button secondary ${workflow.cancelButton}`}
              disabled={!hydrated || pending || needsReason}
              onClick={() => transition("cancel")}
            >
              <XCircle size={15} /> Cancel claim
            </button>
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
    <section className="card">
      <h2>Accounting preparation</h2>
      <p>
        Classify the expense and recoverable tax. The claim total stays{" "}
        {String(expense.currency)}{" "}
        {minorToDecimal(
          Number(expense.amount_minor) + Number(expense.tax_minor),
        )}
        .
      </p>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
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
          <div className="business-form">
            <label>
              Expense category
              <select
                name="categoryAccountId"
                required
                {...form.input("categoryAccountId")}
              >
                {options.accounts.map((a) => (
                  <option key={String(a.id)} value={String(a.id)}>
                    {String(a.name)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Recoverable tax
              <input
                name="tax"
                type="number"
                step="0.01"
                min="0"
                required
                {...form.input("tax")}
              />
            </label>
            <label>
              Posting date
              <input
                name="entryDate"
                type="date"
                min={String(expense.expense_date)}
                {...form.input("entryDate")}
                required
              />
            </label>
            <label>
              Accounting period
              <select name="periodId" required {...form.input("periodId")}>
                <option value="">Select an open period</option>
                {options.periods.map((p) => (
                  <option key={String(p.id)} value={String(p.id)}>
                    {String(p.name)}
                  </option>
                ))}
              </select>
            </label>
          </div>
          <label>
            Accounting note
            <textarea name="note" maxLength={2000} {...form.input("note")} />
          </label>
          <button
            className="button secondary"
            disabled={pending || !hydrated || form.conflict}
          >
            Save accounting preparation
          </button>
        </fieldset>
      </form>
    </section>
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
      amount: draft.amount_minor
        ? minorToDecimal(Number(draft.amount_minor))
        : "",
      reference: String(draft.reference ?? ""),
    },
    String(expense.id),
    String(draft.id),
  );
  const { send, pending, error } = useExpenseCommand(base),
    hydrated = useHydrated(),
    [confirmed, setConfirmed] = useState(false);
  return (
    <section className="card">
      <h3>Record a bank payment</h3>
      <p>
        Make the payment through your bank, then review the slip and confirm the
        amount here.
      </p>
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
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {form.notice}
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          if (form.conflict) return;
          const intent = (e.nativeEvent as SubmitEvent).submitter?.getAttribute(
            "value",
          );
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
          <div className="business-form">
            <label>
              Payment date
              <input name="date" type="date" required {...form.input("date")} />
            </label>
            <label>
              Amount paid ({String(expense.currency)})
              <input
                name="amount"
                type="number"
                step="0.01"
                min="0.01"
                max={minorToDecimal(Number(expense.outstanding_minor))}
                required
                {...form.input("amount")}
              />
            </label>
            <label>
              Bank reference
              <input
                name="reference"
                maxLength={100}
                minLength={2}
                required
                {...form.input("reference")}
              />
            </label>
          </div>
          <label className="check-row">
            <input
              type="checkbox"
              checked={confirmed}
              onChange={(e) => setConfirmed(e.target.checked)}
            />
            I made this payment and checked the amount, recipient and real bank
            slip.
          </label>
          <div className="form-actions">
            <button
              className="button secondary"
              value="save"
              disabled={!hydrated || pending || form.conflict}
            >
              Save payment preparation
            </button>
            <button
              className="button"
              value="confirm"
              disabled={
                !hydrated ||
                pending ||
                form.conflict ||
                !confirmed ||
                !attachments.length
              }
            >
              Confirm bank payment
            </button>
          </div>
        </fieldset>
      </form>
    </section>
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
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {drafts
        .filter((d) => d.status === "confirmed")
        .map((d) => (
          <article className="card" key={String(d.id)}>
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
          </article>
        ))}
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
        <button
          className="button"
          disabled={!hydrated || pending}
          onClick={() =>
            void send(`/${expense.id}/payments`, {
              expectedVersion: expense.version,
            })
          }
        >
          Record bank payment
        </button>
      )}
      {!drafts.length && expense.can_pay !== 1 && (
        <p className="muted">No payments recorded.</p>
      )}
    </section>
  );
}
