"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDrawer } from "@/components/ui/dialog";
import { Button, Field, Select, TextInput } from "@/components/ui/foundation";

import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { useRouter } from "@/lib/shared/destination-router";
import { minorToDecimal } from "@/lib/finance/finance-money";
import { createFinanceBankCommands, type FinanceBankRequest, type FinanceBankScope } from "@/lib/finance/bank-command";
import { useFinanceActionAllowed } from "./purchase-context";
import { acknowledgeFinanceForm, useFinanceDraftClose, useFinanceDraftScope } from "./finance-draft-boundary";
import { announceWorkChange } from "@/lib/work_projection/work-refresh";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/files/attachment-panel";
import { Card, Stack, FormGrid, FormLayout } from "@/components/ui/foundation";
type Row = Record<string, string | number | null>;
type BankCommandOutcome = { unchanged: boolean; completion: HTMLFormElement | null };
function useBankCommand(scope: FinanceBankScope) {
  const [manager] = useState(() => {
    let current: FinanceBankScope | null = scope;
    return { commands: createFinanceBankCommands(() => current), update(value: FinanceBankScope) { current = value; } };
  });
  const commands = manager.commands;
  useLayoutEffect(() => { manager.update(scope); }, [manager, scope]);
  const [error, setError] = useState(""), [notice, setNotice] = useState(""), [busy, setBusy] = useState(false), [pendingRequest, setPendingRequest] = useState<FinanceBankRequest | null>(null);
  const refreshPending = useCallback(() => {
    try { setPendingRequest(commands.pending()?.request ?? null); }
    catch (cause) { setPendingRequest(null); setError(cause instanceof Error ? cause.message : "Return to the original Bank owner to recover this request."); }
  }, [commands]);
  useEffect(() => {
    let active = true;
    queueMicrotask(() => { if (active) refreshPending(); });
    return () => { active = false; };
  }, [refreshPending, scope.base, scope.ownerKey, scope.source, scope.canRecover]);
  async function complete(operation: Promise<Awaited<ReturnType<ReturnType<typeof createFinanceBankCommands>["submit"]>>>) {
    setBusy(true); setError(""); setNotice("");
    try {
      const result = await operation, acknowledgement = result.acknowledge();
      if (!acknowledgement) throw Error("The original Bank acknowledgement was already consumed. Review the current Bank record.");
      if (acknowledgement.completion) acknowledgeFinanceForm(acknowledgement.completion);
      announceWorkChange();
      setNotice(acknowledgement.unchanged ? "The original Bank request is confirmed." : "The original Bank request is confirmed. Your newer draft is retained for review.");
      return { unchanged: acknowledgement.unchanged, completion: acknowledgement.completion?.form ?? null } satisfies BankCommandOutcome;
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "The original Bank request is not confirmed. Retry it safely.");
      return null;
    } finally { setBusy(false); refreshPending(); }
  }
  function send(request: FinanceBankRequest, form: HTMLFormElement) {
    setError("");
    return complete(commands.submit(request, form));
  }
  function retry(form?: HTMLFormElement) {
    setError("");
    return complete(commands.retry(form));
  }
  return { send, retry, error, notice, busy, pendingRequest, pending: pendingRequest !== null, refreshPending };
}
export function BankImport({
  base,
  accounts,
  initialOpen = false,
  returnHref,
}: {
  base: string;
  accounts: Row[];
  initialOpen?: boolean;
  returnHref?: string;
}) {
  const router = useRouter(), draftScope = useFinanceDraftScope(), canBank = useFinanceActionAllowed("settle-existing", "finance.banking"),
    draftRevision = useRef(0), formRef = useRef<HTMLFormElement>(null), restoredRequest = useRef<string | null>(null);
  const [fields, setFields] = useState({ bankAccount: "", number: "", date: "", opening: "", closing: "" }),
    [rows, setRows] = useState<{ externalId: string; transactionDate: string; description: string; amount: string }[]>([
      { externalId: "", transactionDate: "", description: "", amount: "0.00" },
    ]), [open, setOpen] = useState(initialOpen), [visited, setVisited] = useState(initialOpen), [confirmedWithNewerDraft, setConfirmedWithNewerDraft] = useState(false);
  const requestFromDraft = (): FinanceBankRequest | null => {
    const account = accounts.find(item => String(item.id) === fields.bankAccount);
    if (!account) return null;
    return { kind: "import", input: {
      bankAccount: fields.bankAccount, statementNumber: fields.number, statementDate: fields.date,
      opening: fields.opening, closing: fields.closing, sourceHash: "server-computed",
      rows: rows.map(row => ({ ...row, currency: String(account.currency) })),
    } };
  };
  const command = useBankCommand({ base, ownerKey: "statement-import", source: JSON.stringify([accounts, canBank]), draft: () => draftRevision.current, draftRequest: requestFromDraft, canStart: canBank, canRecover: canBank });
  useEffect(() => {
    const request = command.pendingRequest;
    if (request?.kind !== "import" || draftRevision.current > 0) return;
    const stamp = JSON.stringify(request);
    if (restoredRequest.current === stamp) return;
    restoredRequest.current = stamp;
    setFields({ bankAccount: request.input.bankAccount, number: request.input.statementNumber, date: request.input.statementDate, opening: request.input.opening, closing: request.input.closing });
    setRows(request.input.rows.map(row => ({ externalId: row.externalId, transactionDate: row.transactionDate, description: row.description ?? "", amount: row.amount })));
  }, [command.pendingRequest]);
  const clearDraft = () => {
    setFields({ bankAccount: "", number: "", date: "", opening: "", closing: "" });
    setRows([{ externalId: "", transactionDate: "", description: "", amount: "0.00" }]);
    draftRevision.current = 0; setConfirmedWithNewerDraft(false);
  };
  const closeDraft = useFinanceDraftClose(() => { clearDraft(); setOpen(false); }, { root: () => formRef.current });
  const markDraft = () => { draftRevision.current += 1; };
  const markStructure = () => { markDraft(); formRef.current?.dispatchEvent(new Event("change", { bubbles: true })); };
  async function retryOriginal() {
    const outcome = await command.retry(formRef.current ?? undefined);
    if (!outcome) return;
    if (!outcome.unchanged) { setConfirmedWithNewerDraft(true); return; }
    if (initialOpen) router.push(returnHref ?? canonicalUiHref(`${base}/finance/bank`));
    else { clearDraft(); setOpen(false); router.refresh(); }
  }
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      {!initialOpen && (
        <Button appearance="standard" type="button" variant="primary" onClick={() => { if (open) closeDraft(); else { setVisited(true); setOpen(true); } }}>
          {open ? "Close bank statement import" : "Import bank statement"}
        </Button>
      )}
      {command.error && <InlineNotice tone="danger" role="alert">{command.error}</InlineNotice>}
      {command.notice && <InlineNotice tone="info" role="status">{command.notice}</InlineNotice>}
      {!open && command.pendingRequest?.kind === "import" && <Button appearance="standard" type="button" variant="secondary" disabled={command.busy || !canBank} onClick={() => void retryOriginal()}>Retry original bank import</Button>}
      {open && (
        <FormDrawer appearance="standard" inline={initialOpen} open={open} onClose={closeDraft} title="Import bank statement" dismissDisabled={command.busy} retainFormContents={visited}><Stack as="div" className="ui-stack">
          {command.pendingRequest?.kind === "import" && <InlineNotice tone="info" role="status">This statement request is not confirmed yet. Retry sends the exact original body and key. Any edits below remain separate.</InlineNotice>}
          {command.pendingRequest?.kind === "import" && <Button appearance="standard" type="button" variant="secondary" disabled={command.busy || !canBank} onClick={() => void retryOriginal()}>Retry original bank import</Button>}
          {confirmedWithNewerDraft && <InlineNotice tone="info" role="status">The earlier statement was imported. Your newer fields remain here for review; return to reconciliation before starting another import.</InlineNotice>}
          <form ref={formRef} data-finance-draft={draftScope} onInputCapture={markDraft} onChangeCapture={markDraft}
          onSubmit={async (e) => {
            e.preventDefault();
            const request = requestFromDraft();
            if (!request) return;
            const outcome = await command.send(request, e.currentTarget);
            if (!outcome) return;
            if (!outcome.unchanged) { setConfirmedWithNewerDraft(true); return; }
            if (initialOpen) router.push(returnHref ?? canonicalUiHref(`${base}/finance/bank`));
            else { clearDraft(); setOpen(false); router.refresh(); }
          }}
        >
          <FormGrid as="div" className="ui-form-grid">
            <Field appearance="standard">
              Bank ledger account
              <Select appearance="standard" name="bankAccount" required value={fields.bankAccount} onChange={event => setFields(value => ({ ...value, bankAccount: event.target.value }))}>
                <option value="">Select account</option>
                {accounts.map((a) => (
                  <option value={String(a.id)} key={String(a.id)}>
                    {String(a.code)} · {String(a.name)} · {String(a.currency)} (base{" "}
                    {String(a.base_currency)})
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Statement number
              <TextInput appearance="standard" name="number" required value={fields.number} onChange={event => setFields(value => ({ ...value, number: event.target.value }))} />
            </Field>
            <Field appearance="standard">
              Statement date
              <TextInput appearance="standard" name="date" type="date" value={fields.date} onChange={event => setFields(value => ({ ...value, date: event.target.value }))} required />
            </Field>
            <Field appearance="standard">
              Opening balance
              <TextInput appearance="standard" name="opening" type="number" step="0.01" value={fields.opening} onChange={event => setFields(value => ({ ...value, opening: event.target.value }))} required />
            </Field>
            <Field appearance="standard">
              Closing balance
              <TextInput appearance="standard" name="closing" type="number" step="0.01" value={fields.closing} onChange={event => setFields(value => ({ ...value, closing: event.target.value }))} required />
            </Field>
          </FormGrid>
          {rows.map((r, i) => (
            <FormGrid as="div" className="ui-form-grid" key={i}>
              {(
                [
                  ["externalId", "Bank reference", "text"],
                  ["transactionDate", "Transaction date", "date"],
                  ["description", "Description", "text"],
                  ["amount", "Signed amount", "number"],
                ] as const
              ).map(([key, label, type]) => (
                <Field appearance="standard" key={key}>
                  {label} {i + 1}
                  <TextInput appearance="standard"
                    name={`${key}-${i}`}
                    type={type}
                    step={type === "number" ? "0.01" : undefined}
                    required={key !== "description"}
                    value={r[key]}
                    onChange={(e) =>
                      setRows(rows.map((v, j) => (i === j ? { ...v, [key]: e.target.value } : v)))
                    }
                  />
                </Field>
              ))}
              {rows.length > 1 && (
                <Button appearance="standard"
                  variant="secondary"
                  type="button"
                  onClick={() => { markStructure(); setRows(rows.filter((_, j) => j !== i)); }}
                >
                  Remove row {i + 1}
                </Button>
              )}
            </FormGrid>
          ))}
          <div className="ui-form-actions">
            <Button appearance="standard"
              variant="secondary"
              type="button"
              onClick={() => {
                markStructure();
                setRows([
                  ...rows,
                  { externalId: "", transactionDate: "", description: "", amount: "0.00" },
                ]);
              }}
            >
              Add statement row
            </Button>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={command.busy || command.pending || confirmedWithNewerDraft || !canBank}>
              Validate and import
            </Button>
          </div>
          <p className="muted">
            Receipts are positive; withdrawals are negative. For foreign accounts, the confirmed
            company manual rate for the statement date converts the closing balance to the ledger
            functional currency.
          </p>
        </form></Stack></FormDrawer>
      )}
    </Card>
  );
}
export function BankMatch({
  base,
  transaction,
  payments,
  canWrite,
  canApprove,
}: {
  base: string;
  transaction: Row;
  payments: Row[];
  canWrite: boolean;
  canApprove: boolean;
}) {
  const router = useRouter(), draftScope = useFinanceDraftScope(), canBank = useFinanceActionAllowed("settle-existing", "finance.banking") && canWrite && canApprove,
    draftRevision = useRef(0), formRef = useRef<HTMLFormElement>(null), [files, setFiles] = useState<AttachmentPanelChange | null>(null),
    [selectedPayment, setSelectedPayment] = useState(""), [proof, setProof] = useState(""), [confirmedWithNewerDraft, setConfirmedWithNewerDraft] = useState(false);
  const requestFromDraft = (): FinanceBankRequest | null => selectedPayment && proof.trim() ? { kind: "reconcile", id: String(transaction.id), input: {
    expectedVersion: Number(transaction.version), matchedType: "payment", matchedId: selectedPayment, proof,
    attachmentSetIds: files?.setId && files.readyCount > 0 && files.bindable ? [files.setId] : [],
  } } : null;
  const command = useBankCommand({ base, ownerKey: `transaction:${String(transaction.id)}`, source: JSON.stringify([transaction, payments, canBank]), draft: () => draftRevision.current, draftRequest: requestFromDraft, canStart: canBank && transaction.match_status !== "matched", canRecover: canBank });
  const markStructure = () => {
    const form = formRef.current;
    if (form) form.dispatchEvent(new Event("change", { bubbles: true }));
    else draftRevision.current += 1;
  };
  const pendingForTransaction = command.pendingRequest?.kind === "reconcile" && command.pendingRequest.id === String(transaction.id);
  useEffect(() => {
    const request = command.pendingRequest;
    if (request?.kind !== "reconcile" || request.id !== String(transaction.id) || draftRevision.current > 0) return;
    setSelectedPayment(request.input.matchedId); setProof(request.input.proof);
  }, [command.pendingRequest, transaction.id]);
  const matches = payments.filter(
    (p) =>
      p.currency === transaction.currency &&
      p.bank_account === transaction.bank_account &&
      (p.kind === "receipt" ? Number(p.amount_minor) : -Number(p.amount_minor)) ===
        Number(transaction.amount_minor),
  );
  async function retryOriginal() {
    const outcome = await command.retry(formRef.current ?? undefined);
    if (!outcome) return;
    if (outcome.unchanged) router.refresh();
    else setConfirmedWithNewerDraft(true);
  }
  const matched = transaction.match_status === "matched";
  if (matched && !pendingForTransaction) return <div>{command.error && <InlineNotice tone="danger" role="alert">{command.error}</InlineNotice>}<span className="muted">This bank movement is already matched.</span></div>;
  if (matched && pendingForTransaction) return <div>
    {command.error && <InlineNotice tone="danger" role="alert">{command.error}</InlineNotice>}
    {command.notice && <InlineNotice tone="info" role="status">{command.notice}</InlineNotice>}
    <InlineNotice tone="info" role="status">The original match request is still awaiting its native receipt. Recovery resends its exact body and key.</InlineNotice>
    <Button appearance="standard" type="button" variant="secondary" disabled={command.busy || !canBank} onClick={() => void retryOriginal()}>Retry original bank match</Button>
  </div>;
  return (
    <div>
      {command.error && <InlineNotice tone="danger" role="alert">{command.error}</InlineNotice>}
      {command.notice && <InlineNotice tone="info" role="status">{command.notice}</InlineNotice>}
      {pendingForTransaction && <InlineNotice tone="info" role="status">The original match request is not confirmed yet. Retry resends the exact body and key; edits below remain separate.</InlineNotice>}
      {pendingForTransaction && <Button appearance="standard" type="button" variant="secondary" disabled={command.busy || !canBank} onClick={() => void retryOriginal()}>Retry original bank match</Button>}
      {confirmedWithNewerDraft && <InlineNotice tone="info" role="status">The earlier match was saved. Your newer evidence remains here for review; close this review to load the current Bank movement.</InlineNotice>}
      {!matches.length ? (
        <span className="muted">No posted payment matches this amount, bank and currency.</span>
      ) : (
        <FormLayout
          ref={formRef}
          data-finance-draft={draftScope}
          onInputCapture={() => { draftRevision.current += 1; }}
          onChangeCapture={() => { draftRevision.current += 1; }}
          className="ui-form-grid"
          onSubmit={async (e) => {
            e.preventDefault();
            const request = requestFromDraft();
            if (!request) return;
            const outcome = await command.send(request, e.currentTarget);
            if (!outcome) return;
            if (outcome.unchanged) router.refresh();
            else setConfirmedWithNewerDraft(true);
          }}
        >
          <Field appearance="standard">
            Match payment
            <Select appearance="standard" name="payment" required value={selectedPayment} onChange={event => setSelectedPayment(event.target.value)}>
              <option value="">Review and select</option>
              {matches.map((p) => (
                <option value={String(p.id)} key={String(p.id)}>
                  {String(p.payment_number)} · {String(p.payment_date)} ·{" "}
                  {minorToDecimal(Number(p.amount_minor))}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Review evidence
            <TextInput appearance="standard" name="proof" required minLength={3} value={proof} onChange={event => setProof(event.target.value)} />
          </Field>
          <div
            onClickCapture={event => {
              if (!(event.target instanceof Element)) return;
              const button = event.target.closest<HTMLButtonElement>("button");
              if (!button) return;
              const label = button.textContent?.trim();
              const action = button.getAttribute("aria-label");
              if (action?.startsWith("Retry upload for ") || label === "Record sanitization review" || label === "Withdraw file" || label === "Attach clean files to this record") markStructure();
            }}
            onDropCapture={event => { if (event.dataTransfer.files.length > 0) markStructure(); }}
          >
            <AttachmentPanel appearance="standard"
              base={base}
              targetType="finance_bank_transaction"
              targetId={String(transaction.id)}
              expectedTargetVersion={Number(transaction.version)}
              componentKind="action"
              componentRef="reconcile"
              purposeKey="reconciliation_workpaper"
              title="Reconciliation workpaper"
              description="Attach the bank matching workpaper or other evidence reviewed for this match."
              visibility="Visible only to authorized Company Finance reviewers."
              onChange={setFiles}
            />
          </div>
          <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={command.busy || command.pending || confirmedWithNewerDraft || !canBank}>
            Confirm match
          </Button>
        </FormLayout>
      )}
    </div>
  );
}
