"use client";
import Link from "next/link";
import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { decimalToMinor, minorToDecimal } from "@/lib/finance-money";
import { workFetch } from "@/lib/work-fetch";
import { payrollErrorMessage, type PayrollApiError } from "@/lib/payroll-errors";
type Obligation = {
    kind: "withholding_tax" | "social_security";
    accountId: string | null;
    accruedMinor: number;
    remittedMinor: number;
    outstandingMinor: number;
};
type Liabilities = {
    runId: string;
    runVersion: number;
    currency: string;
    obligations: Obligation[];
};
export function PayrollLiabilitiesPanel({ base, runId, version, production }: {
    base: string;
    runId: string;
    version: number;
    production: boolean;
    userId: string;
}) {
    const router = useRouter();
    const [data, setData] = useState<Liabilities | null>(null);
    const [error, setError] = useState("");
    const [busy, setBusy] = useState(false);
    const attempt = useRef<{
        body: string;
        key: string;
    } | null>(null);
    useEffect(() => {
        let active = true;
        void workFetch(`/api/v1${base}/people/payroll/runs/${encodeURIComponent(runId)}/liabilities`, { cache: "no-store" }).then(async (response) => {
            const result = await response.json() as {
                data?: Liabilities;
                error?: PayrollApiError;
            };
            if (!response.ok || !result.data)
                throw result.error ?? { message: "Payroll liabilities are unavailable." };
            if (active)
                setData(result.data);
        }).catch(cause => {
            if (active)
                setError(payrollErrorMessage(cause, "Payroll liabilities are unavailable."));
        });
        return () => { active = false; };
    }, [base, runId, version]);
    async function remit(form: HTMLFormElement, kind: Obligation["kind"]) {
        if (!data)
            return;
        const fields = new FormData(form);
        let amountMinor: number;
        try {
            amountMinor = decimalToMinor(String(fields.get("amount") ?? ""));
        }
        catch {
            setError("Enter an amount with at most two decimal places.");
            return;
        }
        const input = { kind, amountMinor, expectedRunVersion: data.runVersion, bankAccountId: String(fields.get("bankAccountId") ?? ""), bankTransactionId: String(fields.get("bankTransactionId") ?? ""), bankVersion: Number(fields.get("bankVersion")), bankReference: String(fields.get("bankReference") ?? "") };
        const body = JSON.stringify(input), path = `payroll/runs/${encodeURIComponent(runId)}/remit`;
        if (attempt.current?.body !== body)
            attempt.current = { body, key: crypto.randomUUID() };
        setBusy(true);
        setError("");
        try {
            const response = await workFetch(`/api/v1${base}/people/${path}`, { method: "POST", headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key }, body });
            const result = await response.json() as {
                error?: PayrollApiError;
            };
            if (!response.ok)
                throw result.error ?? { message: "Could not record remittance." };
            attempt.current = null;
            router.refresh();
            const refresh = await workFetch(`/api/v1${base}/people/payroll/runs/${encodeURIComponent(runId)}/liabilities`, { cache: "no-store" });
            if (refresh.ok) {
                const updated = await refresh.json() as {
                    data: Liabilities;
                };
                setData(updated.data);
            }
        }
        catch (cause) {
            setError(payrollErrorMessage(cause, "Could not record remittance."));
        }
        finally {
            setBusy(false);
        }
    }
    const labels: Record<Obligation["kind"], string> = { withholding_tax: "Withholding tax", social_security: "Social security" };
    return <section className="card" aria-labelledby="payroll-liabilities-heading"><h2 id="payroll-liabilities-heading">{"Statutory liabilities"}</h2>
    <p className="muted">{"Balances remain payable until bank evidence confirms remittance."}</p>
    {error && <p role="alert">{error}</p>}
    {!data && !error && <p className="muted">{"Loading liabilities…"}</p>}
    {data?.obligations.map(obligation => <article className="row" key={obligation.kind}><div className="row-main"><strong>{labels[obligation.kind]}</strong><span>{"Accrued"} {data.currency} {minorToDecimal(obligation.accruedMinor)} · {"Remitted"} {data.currency} {minorToDecimal(obligation.remittedMinor)}</span><strong>{"Outstanding"} {data.currency} {minorToDecimal(obligation.outstandingMinor)}</strong></div>
      {obligation.outstandingMinor > 0 && !production && <form className="business-form" onSubmit={event => { event.preventDefault(); void remit(event.currentTarget, obligation.kind); }}><label>{"Amount"} ({data.currency})<input name="amount" type="number" min="0.01" step="0.01" max={minorToDecimal(obligation.outstandingMinor)} defaultValue={minorToDecimal(obligation.outstandingMinor)} required/></label><label>{"Bank account ID"}<input name="bankAccountId" required/></label><label>{"Bank transaction ID"}<input name="bankTransactionId" required/></label><label>{"Bank transaction version"}<input name="bankVersion" type="number" min="1" step="1" required/></label><label>{"Bank reference"}<input name="bankReference" required/></label><button className="button secondary" disabled={busy}>{"Record remittance"}</button></form>}
    </article>)}
    <p className="muted">{"Select an imported debit transaction from"} <Link href={`${base}/finance/bank`}>Finance · Cash & banking</Link> {"before recording."}</p>
  </section>;
}
