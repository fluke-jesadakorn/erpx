"use client";

import { workFetch } from "@/lib/work-fetch";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { minorToDecimal } from "@/lib/finance-money";

type Entity = { company_id: string; reporting_name: string; functional_currency: string; status: string };
type Transaction = { id: string; source_company_id: string; target_company_id: string; reference: string; currency: string; amount_minor: number; status: string; version: number; source_journal_id: string | null; target_journal_id: string | null };
type Workspace = { companyId: string; transactions: Transaction[]; entities: Entity[]; canWrite: boolean; canApprove: boolean; canPay: boolean };

export function IntercompanyConsole({ base, data }: { base: string; data: Workspace }) {
  const router = useRouter(), [busy, setBusy] = useState(false), [error, setError] = useState(""), attempt = useRef<{ signature: string; key: string } | null>(null);
  const endpoint = `${base.replace("/holdings/", "/api/v1/holdings/")}/finance/intercompany`;
  async function send(path: string, input: Record<string, unknown>) {
    const signature = `${path}:${JSON.stringify(input)}`;
    if (attempt.current?.signature !== signature) attempt.current = { signature, key: crypto.randomUUID() };
    setBusy(true); setError("");
    try {
      const response = await workFetch(`${endpoint}${path}`, { method: "POST", headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key }, body: JSON.stringify(input) });
      const body = await response.json() as { error?: { message?: string } };
      if (!response.ok) throw new Error(body.error?.message ?? "Unable to save intercompany item.");
      attempt.current = null; router.refresh();
    } catch (cause) { setError(cause instanceof Error ? cause.message : "Retry the same action."); }
    finally { setBusy(false); }
  }
  const targetEntities = data.entities.filter(entity => entity.company_id !== data.companyId);
  return <section className="card" aria-labelledby="intercompany-title">
    <div className="section-head"><div><h2 id="intercompany-title">Intercompany transactions</h2><p className="muted">Create a source-linked item, then require an independent match, settlement, and elimination decision.</p></div><span className="badge">{data.transactions.length}</span></div>
    {error && <p className="error" role="alert">{error}</p>}
    {data.canWrite && <form className="business-form" onSubmit={event => { event.preventDefault(); const form = new FormData(event.currentTarget); void send("", { targetCompanyId: form.get("targetCompanyId"), reference: form.get("reference"), currency: form.get("currency"), amount: form.get("amount"), sourceJournalId: form.get("sourceJournalId") || undefined, targetJournalId: form.get("targetJournalId") || undefined }); }}>
      <label>Target company<select name="targetCompanyId" required><option value="">Choose company</option>{targetEntities.map(entity => <option key={entity.company_id} value={entity.company_id}>{entity.reporting_name} · {entity.company_id}</option>)}</select></label>
      <label>Reference<input name="reference" required minLength={2} maxLength={120} placeholder="IC-2026-001" /></label>
      <label>Currency<input name="currency" required minLength={3} maxLength={3} defaultValue="THB" /></label>
      <label>Amount<input name="amount" required inputMode="decimal" pattern="[0-9]+(\.[0-9]{1,6})?" /></label>
      <label>Source posted journal<input name="sourceJournalId" placeholder="Optional journal id" /></label>
      <label>Target posted journal<input name="targetJournalId" placeholder="Optional journal id" /></label>
      <button className="button" disabled={busy || !targetEntities.length}>{busy ? "Saving…" : "Create intercompany item"}</button>
    </form>}
    {!targetEntities.length && data.canWrite && <p className="muted">No other active Group Finance entity is configured. An administrator must add the target company before creating an item.</p>}
    {data.transactions.length ? <div className="business-table-wrap"><table className="business-table"><thead><tr><th>Reference</th><th>Companies</th><th>Amount</th><th>Status</th><th>Next action</th></tr></thead><tbody>{data.transactions.map(row => { const action = row.status === "unmatched" && data.canApprove ? "match" : row.status === "matched" && data.canPay ? "settle" : row.status === "settled" && data.canApprove ? "eliminate" : null; return <tr key={row.id}><td>{row.reference}<div className="muted">{row.id}</div></td><td>{row.source_company_id} → {row.target_company_id}</td><td>{row.currency} {minorToDecimal(Number(row.amount_minor))}</td><td>{row.status}</td><td>{action ? <form className="form-actions" onSubmit={event => { event.preventDefault(); const form = new FormData(event.currentTarget); void send(`/${encodeURIComponent(row.id)}/${action}`, { expectedVersion: Number(row.version), reason: form.get("reason") }); }}><input name="reason" required minLength={3} maxLength={2000} placeholder="Evidence and reason" aria-label={`Reason to ${action} ${row.reference}`} /><button className="button secondary" disabled={busy}>{action}</button></form> : row.status === "eliminated" ? "Complete" : "Waiting for an authorized independent reviewer"}</td></tr>; })}</tbody></table></div> : <p className="muted">No intercompany transactions yet.</p>}
  </section>;
}
