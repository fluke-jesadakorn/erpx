"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDrawer } from "@/components/ui/dialog";
import { Button, Field, Select, TextInput } from "@/components/ui/foundation";

import { workFetch } from "@/lib/work_projection/work-fetch";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { minorToDecimal } from "@/lib/finance/finance-money";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/files/attachment-panel";
import { Card, Stack, FormGrid, FormLayout } from "@/components/ui/foundation";
type Row = Record<string, string | number | null>;
function useBankCommand(base: string) {
  const router = useRouter(),
    attempt = useRef<{ value: string; key: string } | null>(null);
  const [error, setError] = useState(""),
    [busy, setBusy] = useState(false);
  async function send(path: string, input: unknown) {
    const body = JSON.stringify(input);
    if (attempt.current?.value !== path + body)
      attempt.current = { value: path + body, key: crypto.randomUUID() };
    setError("");
    setBusy(true);
    try {
      const r = await workFetch(`/api/v1${base}/finance/bank/${path}`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
        body,
      });
      const v = (await r.json()) as { error?: { message: string } };
      if (!r.ok) throw new Error(v.error?.message ?? "Could not save.");
      attempt.current = null;
      router.refresh();
      return true;
    } catch (e) {
      setError(e instanceof Error ? e.message : "Connection interrupted. Retry.");
      return false;
    } finally {
      setBusy(false);
    }
  }
  return { send, error, busy };
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
  const router = useRouter(),
    { send, error, busy } = useBankCommand(base);
  const [rows, setRows] = useState([
      { externalId: "", transactionDate: "", description: "", amount: "0.00" },
    ]),
    [open, setOpen] = useState(initialOpen);
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      {!initialOpen && (
        <Button appearance="standard" type="submit" variant="primary" onClick={() => setOpen(!open)}>
          Import bank statement
        </Button>
      )}
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {open && (
        <FormDrawer appearance="standard" inline={initialOpen} open={open} onClose={() => setOpen(false)} title="Import bank statement" dismissDisabled={busy}><Stack as="div" className="ui-stack">{error && <InlineNotice tone="danger" role="alert" className="error">{error}</InlineNotice>}<form
          onSubmit={async (e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget),
              account = accounts.find((a) => a.id === f.get("bankAccount"));
            if (
              await send("statements/import", {
                bankAccount: f.get("bankAccount"),
                statementNumber: f.get("number"),
                statementDate: f.get("date"),
                opening: f.get("opening"),
                closing: f.get("closing"),
                sourceHash: "server-computed",
                rows: rows.map((r) => ({ ...r, currency: account?.currency })),
              })
            ) {
              if (initialOpen) router.push(returnHref ?? `${base}/finance/bank`);
              else setOpen(false);
            }
          }}
        >
          <FormGrid as="div" className="ui-form-grid">
            <Field appearance="standard">
              Bank ledger account
              <Select appearance="standard" name="bankAccount" required>
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
              <TextInput appearance="standard" name="number" required />
            </Field>
            <Field appearance="standard">
              Statement date
              <TextInput appearance="standard" name="date" type="date" required />
            </Field>
            <Field appearance="standard">
              Opening balance
              <TextInput appearance="standard" name="opening" type="number" step="0.01" required />
            </Field>
            <Field appearance="standard">
              Closing balance
              <TextInput appearance="standard" name="closing" type="number" step="0.01" required />
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
                  onClick={() => setRows(rows.filter((_, j) => j !== i))}
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
              onClick={() =>
                setRows([
                  ...rows,
                  { externalId: "", transactionDate: "", description: "", amount: "0.00" },
                ])
              }
            >
              Add statement row
            </Button>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
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
}: {
  base: string;
  transaction: Row;
  payments: Row[];
}) {
  const { send, error, busy } = useBankCommand(base),
    [files, setFiles] = useState<AttachmentPanelChange | null>(null);
  const matches = payments.filter(
    (p) =>
      p.currency === transaction.currency &&
      p.bank_account === transaction.bank_account &&
      (p.kind === "receipt" ? Number(p.amount_minor) : -Number(p.amount_minor)) ===
        Number(transaction.amount_minor),
  );
  return (
    <div>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {!matches.length ? (
        <span className="muted">No posted payment matches this amount, bank and currency.</span>
      ) : (
        <FormLayout
          className="ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            void send(`transactions/${transaction.id}/reconcile`, {
              expectedVersion: transaction.version,
              matchedType: "payment",
              matchedId: f.get("payment"),
              proof: f.get("proof"),
              attachmentSetIds:
                files?.setId && files.readyCount > 0 && files.bindable ? [files.setId] : [],
            });
          }}
        >
          <Field appearance="standard">
            Match payment
            <Select appearance="standard" name="payment" required>
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
            <TextInput appearance="standard" name="proof" required minLength={3} />
          </Field>
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
          <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={busy}>
            Confirm match
          </Button>
        </FormLayout>
      )}
    </div>
  );
}
