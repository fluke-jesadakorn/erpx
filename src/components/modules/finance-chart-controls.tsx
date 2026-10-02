"use client";

import { workFetch } from "@/lib/work-fetch";
import { useMemo, useRef, useState } from "react";
import { useRouter } from "next/navigation";

export type FinanceChart = { id: string | number; name: string | number; base_currency: string | number; accounting_basis?: string | number };
export type FinanceAccount = { id: string | number; code: string | number; name: string | number; category: string | number; currency?: string | number | null; control_kind?: string | number | null; is_control?: string | number | null; status?: string | number | null; version?: string | number | null };
type CsvRow = { code: string; name: string; category: string; currency?: string; controlKind?: string };
const categories = ["asset", "liability", "equity", "revenue", "expense", "other_income", "other_expense"] as const;
const controls = ["none", "receivable", "payable", "bank"] as const;

function useFinanceSetupCommand(base: string) {
  const router = useRouter();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const retryKey = useRef<{ fingerprint: string; value: string } | null>(null);
  async function send(path: string, input: unknown, redirect?: string | ((data: { id?: string | number }) => string)) {
    setPending(true);
    setError("");
    try {
      const fingerprint = `${path}:${JSON.stringify(input)}`;
      const key = retryKey.current?.fingerprint === fingerprint ? retryKey.current.value : crypto.randomUUID();
      retryKey.current = { fingerprint, value: key };
      const response = await workFetch(`${base.replace("/holdings/", "/api/v1/holdings/")}/finance/${path}`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": key },
        body: JSON.stringify(input),
      });
      const body = await response.json() as { data?: { id?: string | number }; error?: { message?: string } };
      if (!response.ok) throw new Error(body.error?.message ?? "Unable to save finance setup.");
      retryKey.current = null;
      if (redirect) router.push(typeof redirect === "function" ? redirect(body.data ?? {}) : redirect);
      else router.refresh();
      return body.data;
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Unable to save finance setup.");
      return null;
    } finally {
      setPending(false);
    }
  }
  return { pending, error, send };
}

function parseCsvLine(line: string): { cells: string[]; error?: string } {
  const cells: string[] = [];
  let current = "";
  let quoted = false;
  let afterQuote = false;
  for (let index = 0; index < line.length; index += 1) {
    const character = line[index];
    if (quoted) {
      if (character === '"') {
        if (line[index + 1] === '"') { current += '"'; index += 1; }
        else { quoted = false; afterQuote = true; }
      } else current += character;
    } else if (afterQuote) {
      if (character === ",") { cells.push(current.trim()); current = ""; afterQuote = false; }
      else if (!/\s/.test(character)) return { cells, error: "Unexpected content after a quoted value." };
    } else if (character === ",") { cells.push(current.trim()); current = ""; }
    else if (character === '"' && current.trim() === "") { current = ""; quoted = true; }
    else current += character;
  }
  if (quoted) return { cells, error: "Unclosed quoted value." };
  cells.push(current.trim());
  return { cells };
}

function parseCsv(value: string): { rows: CsvRow[]; errors: string[] } {
  const lines = value.split(/\r?\n/).map(line => line.trim()).filter(Boolean);
  if (!lines.length) return { rows: [], errors: [] };
  const headerLine = parseCsvLine(lines.shift()!);
  const header = headerLine.cells.map(cell => cell.toLowerCase());
  const required = ["code", "name", "category"];
  const errors = [...(headerLine.error ? [`Header: ${headerLine.error}`] : []), ...required.filter(field => !header.includes(field)).map(field => `Missing required column: ${field}`)];
  if (errors.length) return { rows: [], errors };
  const rows: CsvRow[] = [];
  lines.forEach((line, index) => {
    const parsedLine = parseCsvLine(line);
    if (parsedLine.error) { errors.push(`Row ${index + 2}: ${parsedLine.error}`); return; }
    const cells = parsedLine.cells;
    if (cells.length !== header.length) { errors.push(`Row ${index + 2}: expected ${header.length} column(s), found ${cells.length}.`); return; }
    const row = Object.fromEntries(header.map((key, column) => [key, cells[column] ?? ""]));
    const category = String(row.category ?? "");
    const controlKind = String(row.controlkind ?? row.control_kind ?? "none");
    if (!row.code || !row.name) errors.push(`Row ${index + 2}: code and name are required.`);
    if (!categories.includes(category as typeof categories[number])) errors.push(`Row ${index + 2}: invalid category ${category || "(empty)"}.`);
    if (!controls.includes(controlKind as typeof controls[number])) errors.push(`Row ${index + 2}: invalid controlKind ${controlKind}.`);
    rows.push({ code: String(row.code ?? ""), name: String(row.name ?? ""), category, currency: row.currency ? String(row.currency).toUpperCase() : undefined, controlKind });
  });
  return { rows, errors };
}

export function FinanceChartControls({ base, chart, charts, accounts }: { base: string; chart?: FinanceChart; charts: FinanceChart[]; accounts: FinanceAccount[] }) {
  const { pending, error, send } = useFinanceSetupCommand(base);
  const [csv, setCsv] = useState(`code,name,category,currency,controlKind\n1100,Operating bank,asset,${chart?.base_currency??""},bank\n1200,Trade receivables,asset,${chart?.base_currency??""},receivable`);
  const parsed = useMemo(() => parseCsv(csv), [csv]);
  return <section className="content-stack" aria-label="Chart of accounts management">
    {error && <p className="error" role="alert">{error}</p>}
    <section className="card"><div className="section-head"><div><h2>Charts of accounts</h2><p className="muted">Create a separate reporting boundary for a company or accounting basis. Existing charts and posted journals remain unchanged.</p></div></div>
      <details><summary>Create a chart</summary><form className="business-form" onSubmit={event => { event.preventDefault(); const form = new FormData(event.currentTarget); void send("charts", { name: form.get("name"), baseCurrency: form.get("baseCurrency"), accountingBasis: form.get("accountingBasis") }, data => data.id ? `${base}/finance/setup?step=chart&chart=${encodeURIComponent(String(data.id))}` : `${base}/finance/setup?step=chart`); }}>
        <label>Chart name<input name="name" required maxLength={160} placeholder="Operations ledger" /></label>
        <label>Base currency<input name="baseCurrency" required minLength={3} maxLength={3} defaultValue={String(chart?.base_currency??"")} placeholder="Choose a currency code" pattern="[A-Z]{3}" /></label>
        <label>Accounting basis<input name="accountingBasis" required maxLength={40} defaultValue={String(chart?.accounting_basis??"")} placeholder="Confirmed Company accounting basis" /></label>
        <button className="button" disabled={pending}>{pending ? "Saving…" : "Create chart"}</button>
      </form></details>
      <ul>{charts.map(item => <li key={String(item.id)}>{String(item.name)} · {String(item.base_currency)}{item.id === chart?.id ? " · selected" : ""}</li>)}</ul>
    </section>
    {chart&&<><section className="card"><div className="section-head"><div><h2>Accounts · {String(chart.name)}</h2><p className="muted">Control accounts drive receivables, payables, bank reconciliation and opening schedules.</p></div></div>
      <details><summary>Add an account</summary><form className="business-form" onSubmit={event => { event.preventDefault(); const form = new FormData(event.currentTarget); void send(`charts/${encodeURIComponent(String(chart.id))}/accounts`, { code: form.get("code"), name: form.get("name"), category: form.get("category"), currency: form.get("currency"), controlKind: form.get("controlKind"), isControl: form.get("controlKind") !== "none" }); }}>
        <label>Code<input name="code" required maxLength={40} placeholder="1100" /></label>
        <label>Name<input name="name" required maxLength={160} placeholder="Operating bank" /></label>
        <label>Category<select name="category" defaultValue="asset">{categories.map(value => <option key={value} value={value}>{value}</option>)}</select></label>
        <label>Currency<input name="currency" required minLength={3} maxLength={3} defaultValue={String(chart.base_currency)} /></label>
        <label>Control role<select name="controlKind" defaultValue="none">{controls.map(value => <option key={value} value={value}>{value === "none" ? "None" : value}</option>)}</select></label>
        <button className="button" disabled={pending}>{pending ? "Saving…" : "Add account"}</button>
      </form></details>
      <div className="business-table-wrap"><table className="business-table"><thead><tr><th>Code</th><th>Name</th><th>Category</th><th>Currency</th><th>Control role</th><th>Status</th><th>Change status</th></tr></thead><tbody>{accounts.map(account => { const status = String(account.status ?? "active"); const nextStatus = status === "active" ? "inactive" : "active"; return <tr key={String(account.id)}><td>{String(account.code)}</td><td>{String(account.name)}</td><td>{String(account.category)}</td><td>{String(account.currency ?? chart.base_currency)}</td><td>{String(account.control_kind ?? "none")}</td><td>{status}</td><td><details><summary>Change status</summary><form className="form-actions" onSubmit={event => { event.preventDefault(); const form = new FormData(event.currentTarget); void send(`charts/${encodeURIComponent(String(chart.id))}/accounts/${encodeURIComponent(String(account.id))}/status`, { status: nextStatus, expectedVersion: Number(account.version ?? 1), reason: form.get("reason") }); }}><input name="reason" required minLength={3} maxLength={2000} placeholder="Reason" aria-label={`Reason for ${String(account.code)}`} /><button className="button secondary" disabled={pending}>{pending ? "Saving…" : nextStatus === "active" ? "Activate" : "Deactivate"}</button></form></details></td></tr>; })}</tbody></table></div>
    </section>
    <details className="card"><summary>Import account list</summary><div className="section-head"><div><p className="muted">Preview row-level validation before an idempotent upsert. Required columns: code, name, category. Optional: currency, controlKind.</p></div></div>
      <label>CSV account list<textarea value={csv} onChange={event => setCsv(event.target.value)} rows={7} spellCheck={false} /></label>
      {parsed.errors.length ? <ul className="error" role="alert">{parsed.errors.map(message => <li key={message}>{message}</li>)}</ul> : <p className="muted">Preview: {parsed.rows.length} valid row(s) ready to import.</p>}
      <button className="button" disabled={pending || !parsed.rows.length || parsed.errors.length > 0} onClick={() => void send("charts/import", { chartId: String(chart.id), rows: parsed.rows.map(row => ({ ...row, controlKind: row.controlKind || "none" })) })}>{pending ? "Importing…" : "Import validated accounts"}</button>
    </details>
    <details className="card"><summary>Add accounting period</summary><form className="business-form" onSubmit={event=>{event.preventDefault();const form=new FormData(event.currentTarget);void send("periods",{name:form.get("name"),startDate:form.get("startDate"),endDate:form.get("endDate")});}}><label>Period name<input name="name" required maxLength={100}/></label><label>Start date<input name="startDate" type="date" required/></label><label>End date<input name="endDate" type="date" required/></label><button className="button" disabled={pending}>Add period</button></form></details></>}
  </section>;
}
