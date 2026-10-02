"use client";
import Link from "next/link";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { MoneyAmount } from "@/components/money/currency-provider";
import { workFetch } from "@/lib/work-fetch";
import { payrollErrorMessage, type PayrollApiError } from "@/lib/payroll-errors";
type Row = Record<string, unknown>;
export function PayrollPaymentPanel({ base, run, batch, items, canPrepare, canApprove, canExport, canConfirm }: {
    base: string;
    run: Row;
    batch: Row | null;
    items: Row[];
    userId: string;
    canPrepare: boolean;
    canApprove: boolean;
    canExport: boolean;
    canConfirm: boolean;
}) {
    const status = (value: unknown) => {
        const labels: Record<string, string> = { draft: "Draft", approved: "Approved", exported: "Exported", partially_settled: "Partially settled", settled: "Settled", outstanding: "Outstanding", failed: "Rejected", confirmed: "Confirmed" };
        return labels[String(value)] ?? String(value);
    };
    const router = useRouter();
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState("");
    const attempt = useRef<{
        body: string;
        key: string;
    } | null>(null);
    const outstanding = items.filter(item => ["outstanding", "failed"].includes(String(item.status)));
    async function send(path: string, input: unknown) {
        const body = JSON.stringify(input);
        if (attempt.current?.body !== path + body)
            attempt.current = { body: path + body, key: crypto.randomUUID() };
        setBusy(true);
        setError("");
        try {
            const response = await workFetch(`/api/v1${base}/people/${path}`, { method: "POST", headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key }, body });
            const result = await response.json() as {
                error?: PayrollApiError;
            };
            if (!response.ok)
                throw result.error ?? { message: "Could not save payment change." };
            attempt.current = null;
            router.refresh();
        }
        catch (cause) {
            setError(payrollErrorMessage(cause, "Could not save payment change."));
        }
        finally {
            setBusy(false);
        }
    }
    return <section className="card" aria-label="Payroll payment batch">
    <h2>{"Payment batch"}</h2>
    <p className="muted">{"Exported instructions do not transfer funds from this page."}</p>
    {error && <p role="alert">{error}</p>}
    {!batch && canPrepare && String(run.status) === "posted" && <form className="business-form" onSubmit={event => { event.preventDefault(); const form = new FormData(event.currentTarget); void send("payments/batches", { sourceType: "payroll", sourceId: String(run.id), formatCode: form.get("formatCode"), bankAccountId: form.get("bankAccountId") }); }}>
      <p>{"Prepare from posted net payable."}</p>
      <label>{"Export format"}<select name="formatCode" defaultValue="generic-csv"><option value="generic-csv">Generic CSV sandbox</option><option value="iso20022">ISO 20022 sandbox</option></select></label>
      <label>{"Finance bank account ID"}<input name="bankAccountId" required/></label>
      <p className="muted">{"Review bank accounts in"} <Link href={`${base}/finance/bank`}>Finance · Cash & banking</Link></p>
      <button className="button" disabled={busy}>{"Prepare batch"}</button>
    </form>}
    {!batch && !canPrepare && <p className="muted">{"No payment batch yet."}</p>}
    {batch && <>
      <p><strong><MoneyAmount units={String(batch.total_minor)} currency={String(batch.currency)} legacyScale={2}/></strong> · {items.length} {"payees"} · {status(batch.status)}</p>
      <div className="form-actions">
        {canApprove && batch.status === "draft" && <button className="button" disabled={busy} onClick={() => void send(`payments/batches/${batch.id}/approve`, { expectedVersion: Number(batch.version) })}>{"Approve batch"}</button>}
        {canExport && batch.status === "approved" && <button className="button" disabled={busy} onClick={() => void send(`payments/batches/${batch.id}/export`, {})}>{"Export instruction"}</button>}
        {["exported", "partially_settled", "settled"].includes(String(batch.status)) && <Link className="button secondary" href={`/api/v1${base}/people/payments/batches/${batch.id}/file`}>{"Download file"}</Link>}
        {canPrepare && batch.status === "partially_settled" && items.some(item => item.status === "failed") && <button className="button secondary" disabled={busy} onClick={() => void send(`payments/batches/${batch.id}/retry`, { expectedVersion: Number(batch.version) })}>{"Prepare retry for rejected rows"}</button>}
      </div>
      {batch.status === "partially_settled" && <p className="muted">{"A retry requires another approval. The new file includes rejected rows only."}</p>}
      {canConfirm && ["exported", "partially_settled"].includes(String(batch.status)) && outstanding.length > 0 && <form className="business-form" onSubmit={event => {
                    event.preventDefault();
                    const form = new FormData(event.currentTarget);
                    const rows = outstanding.map(item => {
                        const reference = String(item.reference);
                        const status = String(form.get(`status:${reference}`));
                        const bankVersion = Number(form.get(`bankVersion:${reference}`));
                        return { reference, status, confirmationRef: String(form.get(`ref:${reference}`) ?? "") || undefined, ...(status === "confirmed" ? { bankTransactionId: String(form.get(`bankId:${reference}`) ?? "") || undefined, bankVersion: Number.isInteger(bankVersion) && bankVersion > 0 ? bankVersion : undefined } : {}) };
                    });
                    void send(`payments/batches/${batch.id}/confirm`, { rows });
                }}>
        <h3>{"Reconcile bank results"}</h3>
        <p className="muted">{"Confirm only accepted payments with transaction ID and version from"} <Link href={`${base}/finance/bank`}>{"Bank transactions"}</Link>. {"Rejected rows remain payable."}</p>
        {outstanding.map(item => <div className="row" key={String(item.reference)}><div className="row-main"><strong>{String(item.payee_name)}</strong><span>{String(item.reference)} · <MoneyAmount units={String(item.amount_minor)} currency={String(item.currency)} legacyScale={2}/> · {status(item.status)}</span></div>
          <label>{"Result"}<select name={`status:${String(item.reference)}`} defaultValue="confirmed"><option value="confirmed">{"Confirmed"}</option><option value="failed">{"Failed"}</option></select></label>
          <label>{"Bank reference"}<input name={`ref:${String(item.reference)}`} required/></label>
          <label>{"Bank transaction ID"}<input name={`bankId:${String(item.reference)}`}/></label>
          <label>{"Bank transaction version"}<input name={`bankVersion:${String(item.reference)}`} type="number" min="1" step="1"/></label>
        </div>)}
        <button className="button" disabled={busy}>{"Confirm results"}</button>
      </form>}
      {batch.status === "partially_settled" && !outstanding.length && <p role="status">{"Refresh to review remaining items."}</p>}
    </>}
  </section>;
}
