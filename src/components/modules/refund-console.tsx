"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { ActionAnchor as UiActionAnchor } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { FinanceLegacyHeading } from "@/components/finance/finance-record-workspace";
import { RecordLink as Link } from "@/components/records/record-link";
import { workFetch } from "@/lib/work-fetch";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import type { refundWorkspace } from "@/server/modules/finance/public/refunds";
import { minorToDecimal } from "@/lib/finance-money";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/attachments/attachment-panel";
import { Stack, Card, FormLayout } from "@/components/ui/foundation";
type Workspace = Awaited<ReturnType<typeof refundWorkspace>>;
export function RefundConsole({ base, data }: { base: string; data: Workspace }) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [exported, setExported] = useState<{ content: string; sha256: string } | null>(null),
    [proofs, setProofs] = useState<Record<string, AttachmentPanelChange>>({}),
    retry = useRef<{ fingerprint: string; key: string } | null>(null);
  const row = data.owner,
    money = (value: unknown) => `${row.currency} ${minorToDecimal(Number(value))}`;
  async function send(path: string, input: unknown) {
    if (busy) return;
    setBusy(true);
    setError("");
    const fingerprint = JSON.stringify({ path, input });
    if (retry.current?.fingerprint !== fingerprint)
      retry.current = { fingerprint, key: crypto.randomUUID() };
    try {
      const response = await workFetch(`/api/v1${base}/finance/${path}`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": retry.current.key },
        body: JSON.stringify(input),
      });
      const body = await response.json();
      if (!response.ok) {
        if (response.status < 500) retry.current = null;
        throw new Error(body.error?.message ?? "Command failed.");
      }
      retry.current = null;
      if (body.data.content) setExported(body.data);
      router.refresh();
    } catch (cause) {
      setError(
        cause instanceof Error
          ? cause.message
          : "The response was lost. Retry the same action to recover its result.",
      );
    } finally {
      setBusy(false);
    }
  }
  async function recover(id: string) {
    setError("");
    try {
      const response = await workFetch(`/api/v1${base}/finance/refund-instructions/${id}/export`);
      const body = await response.json();
      if (!response.ok) throw new Error(body.error?.message ?? "Export unavailable.");
      setExported(body.data);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Export unavailable.");
    }
  }
  const reserved = data.instructions
      .filter((i) => ["prepared", "exported", "unknown"].includes(String(i.status)))
      .reduce((total, i) => total + Number(i.amount_minor), 0),
    available = Number(row.outstanding_minor) - reserved;
  return (
    <Stack as="div" className="ui-stack">
      <FinanceLegacyHeading>
        <Link href={`${base}/finance/refunds`}>← Return refunds</Link>
        <h1>{String(row.return_number)} · Refund obligation</h1>
      </FinanceLegacyHeading>
      <Card as="section" appearance="standard" className="ui-panel">
        <p>
          {String(row.party_name)} ·{" "}
          {row.direction === "outgoing" ? "Pay customer" : "Receive from supplier"} ·{" "}
          {String(row.status)}
        </p>
        <p>
          Total {money(row.amount_minor)} · Remaining {money(row.outstanding_minor)} · Reserved{" "}
          {money(reserved)}
        </p>
        <Link href={`${base}/finance/invoices/${row.invoice_id}`}>Original invoice</Link> ·{" "}
        <Link href={`${base}/commerce/orders/${row.order_id}`}>
          Return and goods movement evidence
        </Link>
        <p>
          Created by {String(row.created_by)}
          {row.approved_by && <> · Approved by {String(row.approved_by)}</>}
        </p>
        {row.approved_by && (
          <p>
            Approved counterparty: {String(row.beneficiary_name)} · Account{" "}
            {String(row.beneficiary_account)}
            <br />
            {String(row.approval_reason)}
          </p>
        )}
      </Card>
      {error && (
        <InlineNotice tone="danger" className="error" role="alert">
          {error}
        </InlineNotice>
      )}
      {row.status === "pending" && data.canApprove && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Independent approval</h2>
          <FormLayout
            className="ui-form-grid"
            onSubmit={(event) => {
              event.preventDefault();
              const form = new FormData(event.currentTarget);
              void send(`refunds/${row.id}/approve`, {
                expectedVersion: Number(row.version),
                beneficiaryName: form.get("name"),
                beneficiaryAccount: form.get("account"),
                reason: form.get("reason"),
              });
            }}
          >
            <Field appearance="standard">
              Counterparty name
              <TextInput appearance="standard"
                name="name"
                required
                minLength={2}
                maxLength={160}
                defaultValue={String(row.party_name)}
              />
            </Field>
            <Field appearance="standard">
              Verified counterparty bank account
              <TextInput appearance="standard" name="account" required minLength={3} maxLength={160} />
            </Field>
            <Field appearance="standard">
              Source and beneficiary review evidence
              <TextArea appearance="standard" name="reason" required minLength={3} maxLength={1000} />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Approve refund obligation
            </Button>
          </FormLayout>
        </Card>
      )}
      {row.status === "approved" && data.canPay && available > 0 && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Prepare partial or final instruction</h2>
          <FormLayout
            className="ui-form-grid"
            onSubmit={(event) => {
              event.preventDefault();
              const form = new FormData(event.currentTarget);
              void send(`refunds/${row.id}/instructions`, {
                expectedVersion: Number(row.version),
                amount: form.get("amount"),
                plannedDate: form.get("date"),
                bankAccount: form.get("cash"),
                cashAccountId: form.get("cash"),
                ...(form.get("gain") ? { fxGainAccountId: form.get("gain") } : {}),
                ...(form.get("loss") ? { fxLossAccountId: form.get("loss") } : {}),
              });
            }}
          >
            <Field appearance="standard">
              Amount ({String(row.currency)})
              <TextInput appearance="standard"
                name="amount"
                type="number"
                required
                min="0.01"
                max={available / 100}
                step="0.01"
                defaultValue={minorToDecimal(available)}
              />
            </Field>
            <Field appearance="standard">
              Planned payment date
              <TextInput appearance="standard"
                name="date"
                type="date"
                required
                min={String(row.return_date)}
                defaultValue={new Date().toISOString().slice(0, 10)}
              />
            </Field>
            <Field appearance="standard">
              Bank GL account
              <Select appearance="standard" name="cash" required>
                <option value="">Select account</option>
                {data.accounts.map((a) => (
                  <option key={String(a.id)} value={String(a.id)}>
                    {String(a.code)} · {String(a.name)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Realized FX gain account
              <Select appearance="standard" name="gain">
                <option value="">Select when foreign cash may differ from the credit</option>
                {data.fxAccounts.filter((account) => account.category === "other_income").map((account) => (
                  <option key={String(account.id)} value={String(account.id)}>{String(account.code)} · {String(account.name)}</option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Realized FX loss account
              <Select appearance="standard" name="loss">
                <option value="">Select when foreign cash may differ from the credit</option>
                {data.fxAccounts.filter((account) => account.category === "other_expense").map((account) => (
                  <option key={String(account.id)} value={String(account.id)}>{String(account.code)} · {String(account.name)}</option>
                ))}
              </Select>
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Reserve amount and prepare instruction
            </Button>
          </FormLayout>
        </Card>
      )}
      {data.canPay && data.instructions.some((i) => i.status === "prepared") && (
        <Button appearance="standard"
          type="submit"
          variant="secondary"
          disabled={busy}
          onClick={() =>
            void send("refund-instructions/export", {
              instructions: data.instructions
                .filter((i) => i.status === "prepared")
                .map((i) => ({ id: String(i.id), expectedVersion: Number(i.version) })),
            })
          }
        >
          Export prepared instructions
        </Button>
      )}
      {exported && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Manual bank instructions</h2>
          <p>
            This CSV is an instruction register, not a bank-specific upload format. The treasury
            officer must verify details in the bank system before paying.
          </p>
          <UiActionAnchor appearance="standard"
            variant="secondary"
            href={`data:text/csv;charset=utf-8,${encodeURIComponent(exported.content)}`}
            download="refund-instructions.csv"
          >
            Download CSV
          </UiActionAnchor>
          <p>
            SHA-256 <code>{exported.sha256}</code>
          </p>
        </Card>
      )}
      <h2>Instructions and bank evidence</h2>
      {!data.instructions.length && <p>No instructions prepared yet.</p>}
      {data.instructions.map((i) => (
        <Card as="section" appearance="standard" className="ui-panel" key={String(i.id)}>
          <h3>
            {money(i.amount_minor)} · {String(i.status)}
          </h3>
          <p>
            {String(i.planned_date)} · Company bank {String(i.bank_account)}
          </p>
          {i.export_hash && (
            <p>
              Export SHA-256 <code>{String(i.export_hash)}</code>
              {data.canPay && (
                <Button appearance="standard"
                  type="submit"
                  variant="secondary"
                  onClick={() => void recover(String(i.id))}
                >
                  Retrieve export
                </Button>
              )}
            </p>
          )}
          {i.evidence && <p>Evidence: {String(i.evidence)}</p>}
          {i.bank_reference && <p>Bank reference: {String(i.bank_reference)}</p>}
          {i.payment_id && (
            <Link href={`${base}/finance/bank`}>Matched bank payment · {String(i.payment_id)}</Link>
          )}
          {["exported", "unknown"].includes(String(i.status)) && data.canPay && (
            <FormLayout
              className="ui-form-grid"
              onSubmit={(event) => {
                event.preventDefault();
                const form = new FormData(event.currentTarget),
                  bank = data.bank.find((b) => b.id === form.get("bank")),
                  proof = proofs[String(i.id)];
                if (!bank) return;
                void send(`refund-instructions/${i.id}/confirm`, {
                  expectedVersion: Number(i.version),
                  bankTransactionId: String(bank.id),
                  bankVersion: Number(bank.version),
                  periodId: form.get("period"),
                  bankReference: form.get("reference"),
                  evidence: form.get("evidence"),
                  attachmentSetIds:
                    proof?.setId && proof.readyCount > 0 && proof.bindable ? [proof.setId] : [],
                });
              }}
            >
              <Field appearance="standard">
                Imported bank transaction
                <Select appearance="standard" name="bank" required>
                  <option value="">Select matching bank evidence</option>
                  {data.bank
                    .filter(
                      (b) =>
                        b.bank_account === i.bank_account &&
                        Number(b.amount_minor) ===
                          (row.direction === "outgoing"
                            ? -Number(i.amount_minor)
                            : Number(i.amount_minor)) &&
                        String(b.transaction_date) >= String(i.planned_date),
                    )
                    .map((b) => (
                      <option key={String(b.id)} value={String(b.id)}>
                        {String(b.transaction_date)} · {String(b.external_id)} ·{" "}
                        {String(b.description)}
                      </option>
                    ))}
                </Select>
              </Field>
              <Field appearance="standard">
                Open posting period
                <Select appearance="standard" name="period" required>
                  <option value="">Select period</option>
                  {data.periods.map((p) => (
                    <option key={String(p.id)} value={String(p.id)}>
                      {String(p.name)}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field appearance="standard">
                Actual bank reference
                <TextInput appearance="standard" name="reference" required minLength={2} maxLength={160} />
              </Field>
              <Field appearance="standard">
                Verification evidence
                <TextArea appearance="standard" name="evidence" required minLength={3} maxLength={1000} />
              </Field>
              <AttachmentPanel appearance="standard"
                base={base}
                targetType="finance_refund_obligation"
                targetId={String(row.id)}
                expectedTargetVersion={Number(row.version)}
                componentKind="action"
                componentRef={`refund-instruction:${String(i.id)}`}
                purposeKey="refund_proof"
                title="Refund bank proof"
                description="Attach the bank confirmation or refund payment proof for this exact instruction."
                visibility="Visible only to authorized Finance and Treasury reviewers."
                onChange={(change) =>
                  setProofs((current) => ({ ...current, [String(i.id)]: change }))
                }
              />
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                Confirm bank result and post refund
              </Button>
            </FormLayout>
          )}
          {((data.canPay && ["prepared", "exported"].includes(String(i.status))) ||
            (data.canReviewException &&
              i.prepared_by !== data.userId &&
              ["exported", "unknown"].includes(String(i.status)))) && (
            <FormLayout
              className="ui-form-grid"
              onSubmit={(event) => {
                event.preventDefault();
                const form = new FormData(event.currentTarget);
                void send(`refund-instructions/${i.id}/exception`, {
                  expectedVersion: Number(i.version),
                  outcome: form.get("outcome"),
                  evidence: form.get("evidence"),
                });
              }}
            >
              <Field appearance="standard">
                Exception outcome
                <Select appearance="standard" name="outcome" required>
                  {i.status === "prepared" && data.canPay && (
                    <option value="cancelled">Cancel before export</option>
                  )}
                  {i.status === "exported" && data.canPay && (
                    <option value="unknown">Bank result uncertain — keep amount reserved</option>
                  )}
                  {data.canReviewException &&
                    i.prepared_by !== data.userId &&
                    ["exported", "unknown"].includes(String(i.status)) && (
                      <option value="failed">Bank confirmed failure — release reservation</option>
                    )}
                </Select>
              </Field>
              <Field appearance="standard">
                Evidence and reason
                <TextArea appearance="standard" name="evidence" required minLength={3} maxLength={1000} />
              </Field>
              <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={busy}>
                Record exception
              </Button>
            </FormLayout>
          )}
        </Card>
      ))}
    </Stack>
  );
}
