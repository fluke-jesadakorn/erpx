"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { ActionAnchor as UiActionAnchor } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { FinanceLegacyHeading } from "@/components/finance/finance-record-workspace";
import Link from "next/link";
import styles from "./supplier-batch.module.css";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import { minorToDecimal } from "@/lib/finance-money";
import type { supplierBatchWorkspace } from "@/server/supplier-payments";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/attachments/attachment-panel";
import { Card, FormLayout, FormGrid } from "@/components/ui/foundation";
type Workspace = Awaited<ReturnType<typeof supplierBatchWorkspace>>;
export function SupplierBatchConsole({ base, data }: { base: string; data: Workspace }) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [selected, setSelected] = useState<string[]>([]),
    [query, setQuery] = useState(""),
    [exported, setExported] = useState<{ content: string; sha256: string } | null>(null),
    [proofs, setProofs] = useState<Record<string, AttachmentPanelChange>>({}),
    retry = useRef<{ fingerprint: string; key: string } | null>(null);
  const b = data.batch,
    canPay = !!b && data.canPay && data.userId !== b.created_by && data.userId !== b.approved_by;
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
        }),
        result = await response.json();
      if (!response.ok) {
        if (response.status < 500) retry.current = null;
        throw new Error(result.error?.message ?? "Command failed.");
      }
      retry.current = null;
      if (result.data.content) setExported(result.data);
      if (!b) router.push(`${base}/finance/supplier-batches/${result.data.id}`);
      else router.refresh();
    } catch (cause) {
      setError(
        cause instanceof Error
          ? cause.message
          : "Response lost. Retry the same action to recover its result.",
      );
    } finally {
      setBusy(false);
    }
  }
  async function recover() {
    if (!b) return;
    try {
      const response = await workFetch(`/api/v1${base}/finance/supplier-batches/${b.id}/export`),
        result = await response.json();
      if (!response.ok) throw new Error(result.error?.message ?? "Export unavailable.");
      setExported(result.data);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Export unavailable.");
    }
  }
  return (
    <div className={b ? `content-stack ${styles.console}` : styles.console}>
      {b && (
        <>
          <FinanceLegacyHeading>
            <Link href={`${base}/finance/supplier-batches`}>← Supplier payment batches</Link>
            <h1>Supplier payment batch</h1>
          </FinanceLegacyHeading>
          <Card as="section" appearance="standard" className="ui-panel">
            <p>
              {String(b.planned_date)} · {String(b.currency)} · {String(b.status)}
            </p>
            <p>
              Prepared by {String(b.created_by)}
              {b.approved_by && <> · Approved by {String(b.approved_by)}</>}
            </p>
            {b.approval_reason && <p>Approval evidence: {String(b.approval_reason)}</p>}
          </Card>
        </>
      )}
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      {!b && data.canCreate && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Prepare supplier instructions</h2>
          <p>
            Select up to 50 posted invoices in one currency and chart. Review bank details before
            approval. Available balances exclude other pending instructions.
          </p>
          <FormLayout
            className="ui-form-grid"
            onSubmit={(event) => {
              event.preventDefault();
              const form = new FormData(event.currentTarget);
              void send("supplier-batches", {
                bankAccountId: form.get("bank"),
                plannedDate: form.get("date"),
                items: data.invoices
                  .filter((i) => selected.includes(String(i.id)))
                  .map((i) => ({
                    invoiceId: String(i.id),
                    expectedVersion: Number(i.version),
                    amount: form.get(`amount-${i.id}`),
                    beneficiaryName: form.get(`name-${i.id}`),
                    beneficiaryAccount: form.get(`account-${i.id}`),
                  })),
              });
            }}
          >
            <Field appearance="standard">
              Company bank ledger
              <Select appearance="standard" name="bank" required>
                <option value="">Select bank ledger</option>
                {data.accounts.map((a) => (
                  <option key={String(a.id)} value={String(a.id)}>
                    {String(a.code)} · {String(a.name)} · {String(a.currency)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Planned payment date
              <TextInput appearance="standard" name="date" type="date" required />
            </Field>
            <Field appearance="standard">
              Find supplier invoices
              <TextInput appearance="standard"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Invoice number or supplier"
              />
            </Field>
            <p className="muted">{selected.length} of 50 selected</p>
            {data.invoices
              .filter(
                (i) =>
                  Number(i.available_minor) > 0 &&
                  (selected.includes(String(i.id)) ||
                    `${i.invoice_number} ${i.party_name}`
                      .toLowerCase()
                      .includes(query.trim().toLowerCase())),
              )
              .map((i) => (
                <fieldset key={String(i.id)}>
                  <legend>
                    <Field appearance="standard" orientation="inline">
                      <TextInput appearance="standard"
                        type="checkbox"
                        disabled={selected.length >= 50 && !selected.includes(String(i.id))}
                        checked={selected.includes(String(i.id))}
                        onChange={(event) =>
                          setSelected((values) =>
                            event.target.checked
                              ? [...values, String(i.id)]
                              : values.filter((id) => id !== i.id),
                          )
                        }
                      />{" "}
                      {String(i.invoice_number)} · {String(i.party_name)} · Available{" "}
                      {String(i.currency)} {minorToDecimal(Number(i.available_minor))}
                    </Field>
                  </legend>
                  <FormGrid as="div" className="ui-form-grid" hidden={!selected.includes(String(i.id))}>
                    <Field appearance="standard">
                      Amount
                      <TextInput appearance="standard"
                        disabled={!selected.includes(String(i.id))}
                        name={`amount-${i.id}`}
                        type="number"
                        step="0.01"
                        min="0.01"
                        max={Number(i.available_minor) / 100}
                        defaultValue={minorToDecimal(Number(i.available_minor))}
                        required
                      />
                    </Field>
                    <Field appearance="standard">
                      Verified beneficiary name
                      <TextInput appearance="standard"
                        disabled={!selected.includes(String(i.id))}
                        name={`name-${i.id}`}
                        defaultValue={String(i.party_name)}
                        required
                        minLength={2}
                        maxLength={160}
                      />
                    </Field>
                    <Field appearance="standard">
                      Verified beneficiary account
                      <TextInput appearance="standard"
                        disabled={!selected.includes(String(i.id))}
                        name={`account-${i.id}`}
                        required
                        minLength={3}
                        maxLength={160}
                      />
                    </Field>
                  </FormGrid>
                </fieldset>
              ))}
            {!data.invoices.length && (
              <p>No eligible posted supplier invoices. Create and post the source invoice first.</p>
            )}
            {data.invoices.length === 200 && (
              <p>Showing the first 200 outstanding invoices by due date.</p>
            )}
            <Button appearance="standard"
              clientManaged type="submit"
              variant="primary"
              disabled={busy || !selected.length || selected.length > 50}
            >
              Reserve selected amounts
            </Button>
          </FormLayout>
        </Card>
      )}
      {b &&
        ["draft", "approved"].includes(String(b.status)) &&
        ((b.status === "draft" && data.canApprove && b.created_by !== data.userId) ||
          (data.canCreate && (b.created_by === data.userId || data.canApprove))) && (
          <Card as="section" appearance="standard" className="ui-panel">
            <h2>Review the batch</h2>
            <FormLayout
              className="ui-form-grid"
              onSubmit={(event) => {
                event.preventDefault();
                const form = new FormData(event.currentTarget);
                void send(`supplier-batches/${b.id}/${form.get("decision")}`, {
                  expectedVersion: Number(b.version),
                  reason: form.get("reason"),
                });
              }}
            >
              <Field appearance="standard">
                Decision
                <Select appearance="standard" name="decision">
                  {b.status === "draft" && data.canApprove && b.created_by !== data.userId && (
                    <option value="approve">Approve sources, amounts and beneficiaries</option>
                  )}
                  {data.canCreate && (b.created_by === data.userId || data.canApprove) && (
                    <option value="cancel">Cancel and release reservations</option>
                  )}
                </Select>
              </Field>
              <Field appearance="standard">
                Review evidence or cancellation reason
                <TextArea appearance="standard" name="reason" minLength={3} maxLength={1000} required />
              </Field>
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                Record decision
              </Button>
            </FormLayout>
          </Card>
        )}
      {b && b.status === "approved" && canPay && (
        <Button appearance="standard"
          type="submit"
          variant="primary"
          disabled={busy}
          onClick={() =>
            void send(`supplier-batches/${b.id}/export`, { expectedVersion: Number(b.version) })
          }
        >
          Export approved instructions
        </Button>
      )}
      {b?.export_hash && (
        <Card as="section" appearance="standard" className="ui-panel">
          <p>
            Export SHA-256 <code>{String(b.export_hash)}</code>
          </p>
          {canPay && (
            <Button appearance="standard" type="submit" variant="secondary" onClick={() => void recover()}>
              Retrieve original export
            </Button>
          )}
        </Card>
      )}
      {exported && (
        <Card as="section" appearance="standard" className="ui-panel">
          <p>
            This CSV is a manual instruction register. Verify the beneficiary in the bank system
            before paying. Export does not post cash or settle invoices.
          </p>
          <UiActionAnchor appearance="standard"
            variant="secondary"
            download="supplier-instructions.csv"
            href={`data:text/csv;charset=utf-8,${encodeURIComponent(exported.content)}`}
          >
            Download CSV
          </UiActionAnchor>
          <p>
            SHA-256 <code>{exported.sha256}</code>
          </p>
        </Card>
      )}
      {b &&
        data.items.map((i) => (
          <Card as="section" appearance="standard" className="ui-panel" key={String(i.id)}>
            <h2>
              <Link href={`${base}/finance/invoices/${i.invoice_id}`}>
                {String(i.invoice_number)}
              </Link>{" "}
              · {String(i.currency)} {minorToDecimal(Number(i.amount_minor))}
            </h2>
            <p>
              {String(i.beneficiary_name)} · Account {String(i.beneficiary_account)} ·{" "}
              {String(i.status)}
            </p>
            {i.status === "failed" && (
              <p>
                Review the source invoice balance and{" "}
                <Link href={`${base}/finance/supplier-batches`}>prepare a new instruction</Link> if
                it is still unpaid.
              </p>
            )}
            {i.evidence && <p>Evidence: {String(i.evidence)}</p>}
            {i.bank_reference && <p>Bank reference: {String(i.bank_reference)}</p>}
            {i.payment_id && (
              <Link href={`${base}/finance/payments/${i.payment_id}`}>Payment and allocation</Link>
            )}
            {["exported", "unknown"].includes(String(i.status)) && canPay && (
              <FormLayout
                className="ui-form-grid"
                onSubmit={(event) => {
                  event.preventDefault();
                  const form = new FormData(event.currentTarget),
                    bank = data.bank.find((t) => t.id === form.get("bank")),
                    proof = proofs[String(i.id)];
                  if (!bank) return;
                  void send(`supplier-instructions/${i.id}/confirm`, {
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
                  Matching bank debit
                  <Select appearance="standard" name="bank" required>
                    <option value="">Select imported bank evidence</option>
                    {data.bank
                      .filter(
                        (t) =>
                          Number(t.amount_minor) === -Number(i.amount_minor) &&
                          String(t.transaction_date) >= String(b.planned_date),
                      )
                      .map((t) => (
                        <option key={String(t.id)} value={String(t.id)}>
                          {String(t.transaction_date)} · {String(t.external_id)} ·{" "}
                          {String(t.description)}
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
                  targetType="finance_supplier_batch"
                  targetId={String(b.id)}
                  expectedTargetVersion={Number(b.version)}
                  componentKind="action"
                  componentRef={`supplier-instruction:${String(i.id)}`}
                  purposeKey="payment_proof"
                  title="Supplier payment proof"
                  description="Attach the bank confirmation for this exact supplier instruction."
                  visibility="Visible only to authorized Company Finance and Treasury reviewers."
                  onChange={(change) =>
                    setProofs((current) => ({ ...current, [String(i.id)]: change }))
                  }
                />
                <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                  Confirm bank debit and settle invoice
                </Button>
              </FormLayout>
            )}
            {["exported", "unknown"].includes(String(i.status)) &&
              (canPay || (data.canApprove && data.userId !== b.exported_by)) && (
                <FormLayout
                  className="ui-form-grid"
                  onSubmit={(event) => {
                    event.preventDefault();
                    const form = new FormData(event.currentTarget);
                    void send(`supplier-instructions/${i.id}/result`, {
                      expectedVersion: Number(i.version),
                      outcome: form.get("outcome"),
                      evidence: form.get("evidence"),
                    });
                  }}
                >
                  <Field appearance="standard">
                    Bank exception
                    <Select appearance="standard" name="outcome">
                      {canPay && (
                        <option value="unknown">Result uncertain — retain reservation</option>
                      )}
                      {data.canApprove && data.userId !== b.exported_by && (
                        <option value="failed">
                          Bank confirmed no payment — release reservation
                        </option>
                      )}
                    </Select>
                  </Field>
                  <Field appearance="standard">
                    Evidence
                    <TextArea appearance="standard" name="evidence" required minLength={3} maxLength={1000} />
                  </Field>
                  <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={busy}>
                    Record bank exception
                  </Button>
                </FormLayout>
              )}
          </Card>
        ))}
    </div>
  );
}
