"use client";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { FormDrawer } from "@/components/ui/dialog";
import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, FormGrid, InlineNotice, Select, TextArea, TextInput } from "@/components/ui/foundation";
import { workFetch } from "@/lib/work-fetch";
import { businessDate } from "@/server/modules/pos/public/rules/pos";
import { calculateInvoiceLines } from "@/server/modules/finance/public/rules/finance-invoice-lines";
import { minorToDecimal } from "@/lib/finance-money";
import type { SalesDefaults } from "@/server/modules/finance/public/sales";
import type { FinanceRow } from "@/server/modules/finance/public/finance";
import styles from "./sales-workspace.module.css";
function useSalesCommand(base: string) {
  const router = useRouter(), retry = useRef<{ payload: string; key: string } | null>(null);
  const [pending, setPending] = useState(false), [error, setError] = useState("");
  async function send(path: string, input: unknown, destination?: (data: Record<string, unknown>) => string) {
    if (pending) return false;
    const body = JSON.stringify(input), payload = path + body;
    if (retry.current?.payload !== payload) retry.current = { payload, key: crypto.randomUUID() };
    setPending(true); setError("");
    try {
      const response = await workFetch(`${base.replace("/holdings/", "/api/v1/holdings/")}/finance/sales/${path}`, { method: "POST", headers: { "Content-Type": "application/json", "Idempotency-Key": retry.current.key }, body });
      const result = await response.json(); if (!response.ok) throw new Error(result.error?.message ?? "Unable to save.");
      retry.current = null;
      if (destination) router.push(destination(result.data));
      router.refresh(); return true;
    } catch (error) { setError(error instanceof Error ? error.message : "Unable to save."); return false; }
    finally { setPending(false); }
  }
  return { send, pending, error };
}
export function IssueCustomerReceipt({ base, paymentId }: { base: string; paymentId: string }) {
  const { send, pending, error } = useSalesCommand(base);
  return <><Button appearance="standard" type="button" disabled={pending} onClick={() => void send(`receipts/${paymentId}/issue`, {}, data => `${base}/finance/sales/receipts/${data.id}`)}>Issue receipt</Button>{error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}</>;
}
export function PrintReceipt() { return <Button appearance="standard" type="button" onClick={() => window.print()}>Print / Save as PDF</Button>; }
export function SalesInvoiceEditor({ base, parties, defaults, inline = false }: { base: string; parties: FinanceRow[]; defaults: SalesDefaults[]; inline?: boolean }) {
  const { send, pending, error } = useSalesCommand(base), router = useRouter();
  const [open, setOpen] = useState(true), [currency, setCurrency] = useState(defaults[0]?.currency ?? "THB");
  const [lines, setLines] = useState([{ description: "", quantity: "1", unitPrice: "", taxBps: 700 }]);
  const mapping = defaults.find(row => row.currency === currency);
  let total: ReturnType<typeof calculateInvoiceLines> | null = null;
  try { total = calculateInvoiceLines(lines.map(line => ({ ...line, accountId: "preview" }))); } catch { /* Incomplete drafts have no valid total yet. */ }
  return <FormDrawer appearance="standard" inline={inline} open={open} onClose={() => { setOpen(false); if (!inline) router.replace(`${base}/finance/sales?tab=invoices`); }} title="Invoice completed services" dismissDisabled={pending}>
    <p>For services already completed. For goods or a quotation, use a sales order to retain delivery evidence.</p>
    {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
    <form className={styles.form} onSubmit={event => { event.preventDefault(); const values = new FormData(event.currentTarget); void send("invoices", { partyId: values.get("partyId"), invoiceDate: values.get("invoiceDate"), dueDate: values.get("dueDate"), currency, completionConfirmed: values.get("completionConfirmed") === "on", completionNote: values.get("completionNote"), lines }, data => `${base}/finance/invoices/${data.id}`); }}>
      <FormGrid><Field appearance="standard">Customer<Select appearance="standard" name="partyId" required><option value="">Select customer</option>{parties.filter(row => row.kind === "customer" && row.status === "active").map(row => <option key={String(row.id)} value={String(row.id)}>{String(row.name)}</option>)}</Select></Field>
      <Field appearance="standard">Currency<Select appearance="standard" value={currency} onChange={event => setCurrency(event.target.value)}>{defaults.map(row => <option key={row.id} value={row.currency}>{row.currency}</option>)}</Select></Field>
      <Field appearance="standard">Invoice date<TextInput appearance="standard" name="invoiceDate" type="date" required defaultValue={businessDate()} /></Field><Field appearance="standard">Due date<TextInput appearance="standard" name="dueDate" type="date" required defaultValue={businessDate()} /></Field></FormGrid>
      {lines.map((line, index) => <fieldset key={index} className={styles.line}><legend>Service {index + 1}</legend><Field appearance="standard">Description<TextInput appearance="standard" required maxLength={500} value={line.description} onChange={event => setLines(lines.map((row, i) => i === index ? { ...row, description: event.target.value } : row))} /></Field><FormGrid>
        <Field appearance="standard">Quantity<TextInput appearance="standard" type="number" min="0.001" step="0.001" required value={line.quantity} onChange={event => setLines(lines.map((row, i) => i === index ? { ...row, quantity: event.target.value } : row))} /></Field>
        <Field appearance="standard">Unit price<TextInput appearance="standard" type="number" min="0.01" step="0.01" required value={line.unitPrice} onChange={event => setLines(lines.map((row, i) => i === index ? { ...row, unitPrice: event.target.value } : row))} /></Field>
        <Field appearance="standard">VAT (%)<TextInput appearance="standard" type="number" min="0" max="100" step="0.01" required value={line.taxBps / 100} onChange={event => setLines(lines.map((row, i) => i === index ? { ...row, taxBps: Math.round(Number(event.target.value) * 100) } : row))} /></Field></FormGrid>
        {lines.length > 1 && <Button appearance="standard" type="button" variant="secondary" onClick={() => setLines(lines.filter((_, i) => i !== index))}>Remove service</Button>}
      </fieldset>)}
      <Button appearance="standard" type="button" variant="secondary" disabled={lines.length >= 100} onClick={() => setLines([...lines, { description: "", quantity: "1", unitPrice: "", taxBps: 700 }])}>Add service</Button>
      <div className={styles.total} aria-live="polite">Subtotal {currency} {total ? minorToDecimal(total.subtotalMinor) : "—"}<br />VAT {currency} {total ? minorToDecimal(total.taxMinor) : "—"}<br /><strong>Total {currency} {total ? minorToDecimal(total.totalMinor) : "—"}</strong></div>
      <Field appearance="standard">Completion evidence<TextArea appearance="standard" name="completionNote" required minLength={3} maxLength={2000} placeholder="What was completed, when, and who accepted it?" /></Field>
      <label><input type="checkbox" name="completionConfirmed" required /> These services are completed and ready to invoice.</label>
      <p className="muted">The document number and accounts are filled from approved Company settings. Finance reviews the posting before approval.</p>
      {!mapping && <InlineNotice tone="danger">Approve sales defaults for this currency before invoicing.</InlineNotice>}
      {mapping && !mapping.tax_account_id && lines.some(line => line.taxBps > 0) && <InlineNotice tone="warning">Output VAT is missing from these sales defaults. <a href={`${base}/finance/sales/settings`}>Configure and approve a VAT account</a> before preparing a taxable invoice.</InlineNotice>}
      <Button appearance="standard" clientManaged type="submit" disabled={pending || !mapping || !total || !!lines.find(line => line.taxBps > 0 && !mapping?.tax_account_id)}>{pending ? "Saving…" : "Prepare invoice"}</Button>
    </form>
  </FormDrawer>;
}
export function SalesDefaultsControls({ base, editions, charts, accounts, canPrepare, canReview, userId }: { base: string; editions: SalesDefaults[]; charts: FinanceRow[]; accounts: FinanceRow[]; canPrepare: boolean; canReview: boolean; userId: string }) {
  const { send, pending, error } = useSalesCommand(base), [chartId, setChartId] = useState(String(charts[0]?.id ?? ""));
  const chart = charts.find(row => row.id === chartId), currency = String(chart?.base_currency ?? "THB");
  const eligible = accounts.filter(row => row.chart_id === chartId && row.currency === currency && row.status === "active");
  const picker = (name: string, label: string, rows: FinanceRow[], required = true) => <Field appearance="standard">{label}<Select appearance="standard" name={name} required={required}><option value="">{required ? "Select account" : "No VAT account"}</option>{rows.map(row => <option key={String(row.id)} value={String(row.id)}>{String(row.code)} · {String(row.name)}</option>)}</Select></Field>;
  return <div className={styles.form}>{error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
    {canPrepare && <FormDisclosure appearance="standard" drawerTitle="Configure sales defaults" dismissDisabled={pending} error={error}><summary>Configure sales accounts</summary><form className={styles.form} onSubmit={event => { event.preventDefault(); const values = new FormData(event.currentTarget); void send("defaults", { chartId, currency, receivableAccountId: values.get("receivableAccountId"), revenueAccountId: values.get("revenueAccountId"), ...(values.get("taxAccountId") ? { taxAccountId: values.get("taxAccountId") } : {}), reason: values.get("reason") }); }}>
      <Field appearance="standard">Ledger<Select appearance="standard" value={chartId} required onChange={event => setChartId(event.target.value)}>{charts.filter(row => row.status === "active").map(row => <option key={String(row.id)} value={String(row.id)}>{String(row.name)} · {String(row.base_currency)}</option>)}</Select></Field>
      {picker("receivableAccountId", "Trade receivables", eligible.filter(row => row.control_kind === "receivable"))}{picker("revenueAccountId", "Sales revenue", eligible.filter(row => row.category === "revenue"))}{picker("taxAccountId", "Output VAT", eligible.filter(row => row.category === "liability" && row.control_kind === "none"), false)}
      <Field appearance="standard">Reason<TextArea appearance="standard" name="reason" minLength={10} required /></Field><Button appearance="standard" clientManaged type="submit" disabled={pending}>Prepare for independent review</Button>
    </form></FormDisclosure>}
    {editions.map(row => <article className={styles.line} key={row.id}><strong>{row.currency} · {row.status}</strong><p>{row.reason}</p><p className="muted">Receivables: {String(accounts.find(a => a.id === row.receivable_account_id)?.name ?? row.receivable_account_id)} · Revenue: {String(accounts.find(a => a.id === row.revenue_account_id)?.name ?? row.revenue_account_id)} · VAT: {String(accounts.find(a => a.id === row.tax_account_id)?.name ?? "Not configured")}</p>
      {row.status === "draft" && canReview && row.created_by !== userId && <FormDisclosure appearance="standard" drawerTitle="Review sales defaults" error={error}><summary>Review defaults</summary><form className={styles.form} onSubmit={event => { event.preventDefault(); const values = new FormData(event.currentTarget); void send(`defaults/${row.id}/decide`, { expectedVersion: Number(row.version), action: values.get("action"), reason: values.get("reason") }); }}><Field appearance="standard">Decision<Select appearance="standard" name="action"><option value="approve">Approve</option><option value="reject">Reject</option></Select></Field><Field appearance="standard">Review reason<TextArea appearance="standard" name="reason" minLength={10} required /></Field><Button appearance="standard" clientManaged type="submit" disabled={pending}>Record review</Button></form></FormDisclosure>}
    </article>)}
  </div>;
}
