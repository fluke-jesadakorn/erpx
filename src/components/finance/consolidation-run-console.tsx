"use client";

import { workFetch } from "@/lib/work-fetch";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";

type Run = {
  id: string;
  presentation_currency: string;
  as_of_date: string;
  status: "draft" | "review" | "approved" | "published";
  source_hash: string;
  version: number;
  created_by: string;
  reviewed_by: string | null;
  approved_by: string | null;
  published_by: string | null;
};

export function ConsolidationRunConsole({ base, runs, canWrite, canApprove }: { base: string; runs: Run[]; canWrite: boolean; canApprove: boolean }) {
  const router = useRouter();
  const [busy, setBusy] = useState(false), [error, setError] = useState("");
  const attempt = useRef<{ signature: string; key: string } | null>(null);
  const endpoint = `${base.replace("/holdings/", "/api/v1/holdings/")}/finance/consolidation-runs`;
  async function send(path: string, input: Record<string, unknown>) {
    const signature = `${path}:${JSON.stringify(input)}`;
    if (attempt.current?.signature !== signature) attempt.current = { signature, key: crypto.randomUUID() };
    setBusy(true); setError("");
    try {
      const response = await workFetch(`${endpoint}${path}`, { method: "POST", headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key }, body: JSON.stringify(input) });
      const body = await response.json() as { error?: { message?: string } };
      if (!response.ok) throw new Error(body.error?.message ?? "Unable to save consolidation run.");
      attempt.current = null; router.refresh();
    } catch (cause) { setError(cause instanceof Error ? cause.message : "Retry the same action."); }
    finally { setBusy(false); }
  }
  return <section className="card" aria-labelledby="consolidation-runs-title">
    <div className="section-head"><div><h2 id="consolidation-runs-title">Consolidation runs</h2><p className="muted">Snapshot closed company periods, require independent review and publish a controlled group result.</p></div><span className="badge">{runs.length}</span></div>
    {error && <p className="error" role="alert">{error}</p>}
    {canWrite && <form className="business-form" onSubmit={event => { event.preventDefault(); const form = new FormData(event.currentTarget); const translationRates = String(form.get("translationRates") ?? "").split(/\r?\n/).map(line => line.trim()).filter(Boolean).map(line => { const [currency, rate] = line.split("=").map(value => value.trim()); return { currency, rate }; }); void send("", { asOfDate: form.get("asOfDate"), presentationCurrency: form.get("presentationCurrency"), translationRates }); }}>
      <label>As-of date<input name="asOfDate" type="date" required /></label>
      <label>Presentation currency<input name="presentationCurrency" minLength={3} maxLength={3} defaultValue="THB" required /></label>
      <label>Translation rates (presentation currency per source currency)<textarea name="translationRates" rows={3} placeholder="USD=35.50\nEUR=38.20" /></label>
      <button className="button" disabled={busy}>{busy ? "Preparing…" : "Prepare consolidation snapshot"}</button>
    </form>}
    {!canWrite && !runs.length && <p className="muted">A Finance preparer must prepare the first consolidation snapshot.</p>}
    {runs.length ? <div className="business-table-wrap"><table className="business-table"><thead><tr><th>As of</th><th>Currency</th><th>Status</th><th>Source snapshot</th><th>Next action</th></tr></thead><tbody>{runs.map(run => {
      const action = run.status === "draft" && canWrite ? "review" : run.status === "review" && canApprove ? "approve" : run.status === "approved" && canApprove ? "publish" : null;
      return <tr key={run.id}><td>{run.as_of_date}<div className="muted">{run.id}</div></td><td>{run.presentation_currency}</td><td>{run.status} · v{run.version}</td><td><code>{String(run.source_hash).slice(0, 16)}</code><div className="muted">Prepared by {run.created_by}</div></td><td>{action ? <form className="form-actions" onSubmit={event => { event.preventDefault(); const form = new FormData(event.currentTarget); void send(`/${encodeURIComponent(run.id)}/${action}`, { expectedVersion: Number(run.version), reason: form.get("reason") }); }}><input name="reason" required minLength={3} maxLength={2000} placeholder="Review evidence" aria-label={`Reason to ${action} ${run.as_of_date}`} /><button className="button secondary" disabled={busy}>{action}</button></form> : run.status === "published" ? "Published" : "Waiting for an authorized independent reviewer"}</td></tr>;
    })}</tbody></table></div> : <p className="muted">No consolidation snapshots yet. Every active Group entity needs a closed period on or before the selected date.</p>}
  </section>;
}
