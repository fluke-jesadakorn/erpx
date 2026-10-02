"use client";
import Link from "next/link";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { minorToDecimal } from "@/lib/finance-money";
import { workFetch } from "@/lib/work-fetch";
import { payrollErrorMessage, type PayrollApiError } from "@/lib/payroll-errors";
type Row = Record<string, unknown>;
type Options = {
    charts: Row[];
    periods: Row[];
    accounts: Row[];
    mappings?: Row[];
};
type Payload = Record<string, string | number>;
type Preview = {
    lines: Array<{
        accountId: string;
        description: string;
        debit: string;
        credit: string;
        dimensions?: Record<string, string>;
    }>;
    totals: {
        debitMinor: number;
        creditMinor: number;
    };
    categories: Record<string, number>;
};
const mappings: Array<{
    name: string;
    label: string;
    category: string;
    required?: boolean;
}> = [
    { name: "expenseAccountId", label: "Wages expense", category: "expense", required: true },
    { name: "overtimeExpenseAccountId", label: "OT expense", category: "expense" },
    { name: "bonusExpenseAccountId", label: "Bonus expense", category: "expense" },
    { name: "serviceChargeExpenseAccountId", label: "Service charge expense", category: "expense" },
    { name: "employerExpenseAccountId", label: "Employer contribution expense", category: "expense" },
    { name: "netPayableAccountId", label: "Net pay payable", category: "liability", required: true },
    { name: "taxPayableAccountId", label: "Withholding tax payable", category: "liability" },
    { name: "employerContributionAccountId", label: "Employer contributions payable", category: "liability" },
    { name: "socialSecurityPayableAccountId", label: "Social security payable", category: "liability" },
    { name: "advanceReceivableAccountId", label: "Advance receivable", category: "asset" },
    { name: "benefitPayableAccountId", label: "Benefits payable", category: "liability" },
    { name: "otherDeductionPayableAccountId", label: "Other deductions payable", category: "liability" },
];
export function PayrollAccountingPanel({ base, run, options }: {
    base: string;
    run: Row;
    options: Options | null;
    userId: string;
}) {
    const router = useRouter();
    const approvedMapping = options?.mappings?.find(mapping => String(mapping.pay_group_id) === String(run.pay_group_id) && String(mapping.status ?? "approved") === "approved");
    const [chartId, setChartId] = useState(String(approvedMapping?.chart_id ?? options?.charts[0]?.id ?? ""));
    const [manual, setManual] = useState(false);
    const [preview, setPreview] = useState<{
        value: Preview;
        payload: Payload;
    } | null>(null);
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState("");
    const key = useRef<string | null>(null);
    if (!options)
        return <section className="card"><h2>{"Accounting preview"}</h2><p role="status">{"An active chart and open period are required."}</p></section>;
    const accounts = options.accounts.filter(account => String(account.chart_id) === chartId);
    const periodEnd = String(run.period_end);
    const period = options.periods.find(item => String(item.start_date) <= periodEnd && periodEnd <= String(item.end_date)) ?? options.periods[0];
    const entryDate = period && String(period.start_date) > periodEnd ? String(period.start_date) : periodEnd;
    function payload(form: HTMLFormElement): Payload {
        const fields = new FormData(form);
        const result: Payload = { chartId: String(fields.get("chartId")), periodId: String(fields.get("periodId")), entryDate: String(fields.get("entryDate")), expectedVersion: Number(run.version) };
        for (const mapping of mappings) {
            const value = String(fields.get(mapping.name) ?? "");
            if (value)
                result[mapping.name] = value;
        }
        return result;
    }
    async function showPreview(input: Payload) {
        setBusy(true);
        setError("");
        setPreview(null);
        try {
            const response = await workFetch(`/api/v1${base}/people/payroll/runs/${String(run.id)}/accounting-preview`, { method: "POST", headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() }, body: JSON.stringify(input) });
            const body = await response.json() as {
                data?: Preview;
                error?: PayrollApiError;
            };
            if (!response.ok || !body.data)
                throw body.error ?? { message: "Accounting preview is unavailable." };
            if (body.data.totals.debitMinor !== body.data.totals.creditMinor)
                throw { code: "PAYROLL_TOTALS_INVALID", message: "Journal debits and credits do not balance." };
            setPreview({ value: body.data, payload: input });
        }
        catch (cause) {
            setError(payrollErrorMessage(cause, "Accounting preview is unavailable."));
        }
        finally {
            setBusy(false);
        }
    }
    async function post() {
        if (!preview)
            return;
        setBusy(true);
        setError("");
        key.current ??= crypto.randomUUID();
        try {
            const response = await workFetch(`/api/v1${base}/people/payroll/runs/${String(run.id)}/post`, { method: "POST", headers: { "Content-Type": "application/json", "Idempotency-Key": key.current }, body: JSON.stringify(preview.payload) });
            const body = await response.json() as {
                error?: PayrollApiError;
            };
            if (!response.ok)
                throw body.error ?? { message: "Could not post payroll." };
            key.current = null;
            router.refresh();
        }
        catch (cause) {
            setError(payrollErrorMessage(cause, "Could not post payroll."));
        }
        finally {
            setBusy(false);
        }
    }
    return <section className="card" aria-labelledby="payroll-accounting-heading">
    <h2 id="payroll-accounting-heading">{"Accounting preview"}</h2>
    <p className="muted">{"Accrue at work-period end or adjust in an open period."}</p>
    {approvedMapping ? <div className="stacked-list"><p><strong>{"Approved pay-group mapping"}</strong> · version {String(approvedMapping.version)} · {String(options.charts.find(chart => String(chart.id) === String(approvedMapping.chart_id))?.name ?? approvedMapping.chart_id)}</p><p className="muted">{"An open accounting period and work-period end are selected automatically; closed periods use an open adjustment period."}</p><button className="button" disabled={busy} onClick={() => void showPreview({ expectedVersion: Number(run.version), accountMappingId: String(approvedMapping.id) })}>{"Preview journal"}</button><button type="button" className="button secondary" onClick={() => { setManual(!manual); setPreview(null); }}>{manual ? "Close manual mapping" : "Override for this run"}</button></div> : <p className="notice">{"Finance must approve a pay-group mapping before recurring payroll can post automatically."}</p>}
    {error && <p role="alert">{error}</p>}
    {(!approvedMapping || manual) && <form className="business-form" onChange={() => setPreview(null)} onSubmit={event => { event.preventDefault(); void showPreview(payload(event.currentTarget)); }}>
      <label>{"Chart"}<select name="chartId" value={chartId} required onChange={event => setChartId(event.currentTarget.value)}>{options.charts.map(chart => <option value={String(chart.id)} key={String(chart.id)}>{String(chart.name)} · {String(chart.base_currency)}</option>)}</select></label>
      <label>{"Open period"}<select name="periodId" defaultValue={String(period?.id ?? "")} required>{options.periods.map(item => <option value={String(item.id)} key={String(item.id)}>{String(item.name)} · {String(item.start_date)} – {String(item.end_date)}</option>)}</select></label>
      <label>{"Entry date"}<input name="entryDate" type="date" defaultValue={entryDate} required/></label>
      {mappings.map(mapping => <label key={mapping.name}>{mapping.label}<select name={mapping.name} required={mapping.required} defaultValue=""><option value="">{mapping.required ? "Choose account" : "Not used"}</option>{accounts.filter(account => String(account.category) === mapping.category && String(account.currency) === String(run.currency)).map(account => <option value={String(account.id)} key={String(account.id)}>{String(account.code)} · {String(account.name)}</option>)}</select></label>)}
      <button className="button" disabled={busy || !options.charts.length || !options.periods.length}>{"Preview journal"}</button>
    </form>}
    {preview && <div className="stacked-list" aria-live="polite"><h3>{"Journal lines"}</h3><div className="business-table-wrap"><table className="business-table"><thead><tr><th>{"Account"}</th><th>{"Description"}</th><th>{"Unit"}</th><th>{"Debit"}</th><th>{"Credit"}</th></tr></thead><tbody>{preview.value.lines.map((line, index) => <tr key={`${line.accountId}:${index}`}><td>{String(accounts.find(account => String(account.id) === line.accountId)?.code ?? line.accountId)}</td><td>{line.description}</td><td>{[line.dimensions?.branchId, line.dimensions?.departmentId].filter(Boolean).join(" / ") || "—"}</td><td>{line.debit}</td><td>{line.credit}</td></tr>)}</tbody></table></div><p><strong>{"Debit"} {String(run.currency)} {minorToDecimal(preview.value.totals.debitMinor)} = {"Credit"} {String(run.currency)} {minorToDecimal(preview.value.totals.creditMinor)}</strong></p><button className="button" disabled={busy} onClick={() => void post()}>{"Post reviewed journal"}</button></div>}
    <p className="muted">{"Review posted evidence in"} <Link href={`${base}/finance`}>Finance</Link></p>
  </section>;
}
export function PayrollAccountMappingSettings({ base, groups, options, userId }: {
    base: string;
    groups: Row[];
    options: Options;
    userId: string;
}) {
    const router = useRouter();
    const [groupId, setGroupId] = useState(String(groups[0]?.id ?? ""));
    const [chartId, setChartId] = useState(String(options.charts.find(chart => String(chart.base_currency) === String(groups[0]?.currency))?.id ?? options.charts[0]?.id ?? ""));
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState("");
    const attempt = useRef<{
        body: string;
        key: string;
    } | null>(null);
    const accounts = options.accounts.filter(account => String(account.chart_id) === chartId && String(account.currency) === String(groups.find(group => String(group.id) === groupId)?.currency));
    const groupMappings = (options.mappings ?? []).filter(mapping => String(mapping.pay_group_id) === groupId);
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
                throw result.error ?? { message: "Could not save mapping." };
            attempt.current = null;
            router.refresh();
        }
        catch (cause) {
            setError(payrollErrorMessage(cause, "Could not save mapping."));
        }
        finally {
            setBusy(false);
        }
    }
    return <section className="card" aria-labelledby="payroll-mapping-heading"><h2 id="payroll-mapping-heading">{"Payroll account mapping"}</h2><p className="muted">{"One Finance user drafts the mapping; another approves before recurring use."}</p>
    {error && <p role="alert">{error}</p>}
    <label>{"Pay group"}<select value={groupId} onChange={event => { const id = event.currentTarget.value; setGroupId(id); setChartId(String(options.charts.find(chart => String(chart.base_currency) === String(groups.find(group => String(group.id) === id)?.currency))?.id ?? "")); }}>{groups.map(group => <option key={String(group.id)} value={String(group.id)}>{String(group.name)} · {String(group.currency)}</option>)}</select></label>
    {groupMappings.map(mapping => <div className="row" key={String(mapping.id)}><div className="row-main"><strong>{String(mapping.status)} · version {String(mapping.version)}</strong><span>{String(options.charts.find(chart => String(chart.id) === String(mapping.chart_id))?.name ?? mapping.chart_id)}</span></div>{mapping.status === "draft" && mapping.created_by !== userId && <button className="button" disabled={busy} onClick={() => void send(`payroll/account-mappings/${String(mapping.id)}/approve`, { expectedVersion: Number(mapping.version) })}>{"Approve mapping"}</button>}</div>)}
    {groupId && <form className="business-form" onSubmit={event => {
                event.preventDefault();
                const data = new FormData(event.currentTarget);
                const input: Record<string, string> = { chartId: String(data.get("chartId")) };
                for (const mapping of mappings) {
                    const value = String(data.get(mapping.name) ?? "");
                    if (value)
                        input[mapping.name] = value;
                }
                void send(`payroll/pay-groups/${encodeURIComponent(groupId)}/account-mappings`, input);
            }}>
      <label>{"Chart"}<select name="chartId" value={chartId} onChange={event => setChartId(event.currentTarget.value)} required>{options.charts.filter(chart => String(chart.base_currency) === String(groups.find(group => String(group.id) === groupId)?.currency)).map(chart => <option key={String(chart.id)} value={String(chart.id)}>{String(chart.name)}</option>)}</select></label>
      {mappings.map(mapping => <label key={mapping.name}>{mapping.label}<select name={mapping.name} required={mapping.required} defaultValue=""><option value="">{mapping.required ? "Choose account" : "Not used"}</option>{accounts.filter(account => String(account.category) === mapping.category).map(account => <option key={String(account.id)} value={String(account.id)}>{String(account.code)} · {String(account.name)}</option>)}</select></label>)}
      <button className="button" disabled={busy || !options.charts.length}>{"Save mapping draft"}</button>
    </form>}
  </section>;
}
