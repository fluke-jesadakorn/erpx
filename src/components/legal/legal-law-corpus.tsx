"use client";
import { useCallback, useEffect, useState } from "react";
import type { LegalData, LegalDetail } from "@/lib/legal/model";
import s from "./legal.module.css";

type Row = { id: string; rule_id: string; source_id?: string; revision_id?: string; state: string; version: number; created_by: string; title?: string; record_title: string; source_url?: string; revision_label?: string; published_on?: string; effective_from?: string; effective_until?: string; applies?: boolean; basis?: string; check_date?: string; error?: string; checked_at?: number };
type Corpus = { sources: Row[]; revisions: Row[]; applicability: Row[]; checks: Row[]; notice: string };
export function LegalLawCorpus({ call, manage, canReview, userId }: { call: (path: string, body?: unknown) => Promise<unknown>; manage: boolean; canReview: boolean; userId: string }) {
  const [data, setData] = useState<Corpus | null>(null);
  const [rules, setRules] = useState<LegalData["items"]>([]);
  const [selected, setSelected] = useState<LegalDetail | null>(null);
  const [ruleId, setRuleId] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [chunkCount, setChunkCount] = useState(1);
  const refresh = useCallback(async () => {
    const [corpus, register] = await Promise.all([call(`/laws${ruleId ? `?ruleId=${encodeURIComponent(ruleId)}` : ""}`), call("?kind=rule")]);
    setData(corpus as Corpus); setRules((register as LegalData).items);
    if (ruleId) setSelected(await call(`/${ruleId}`) as LegalDetail);
  }, [call, ruleId]);
  useEffect(() => { let active = true; queueMicrotask(() => { if (active) void refresh().catch(error => { if (active) setError(error.message); }); }); return () => { active = false; }; }, [refresh]);
  async function command(action: string, values: Record<string, unknown>) {
    if (!selected) return;
    setBusy(true); setError("");
    try { await call("", { action, id: selected.item.id, expectedVersion: selected.item.version, ...values }); await refresh(); }
    catch (error) { setError(error instanceof Error ? error.message : "Unable to record source review."); }
    finally { setBusy(false); }
  }
  return <div className={s.detail}>
    <p>{data?.notice ?? "Law text, dates and Company applicability remain pending until independently reviewed."}</p>
    <p className={s.small}>Dates use the Gregorian calendar. A daily check runs after 06:00 Asia/Bangkok; download or OCR failures require manual review of the official original.</p>
    {error && <p role="alert">{error}</p>}
    <label className={s.field}>Jurisdiction rule<select value={ruleId} onChange={event => { setRuleId(event.target.value); setSelected(null); }}><option value="">All accessible law sources</option>{rules.map(rule => <option key={rule.id} value={rule.id}>{rule.title}</option>)}</select></label>
    {!rules.length && <p>Create a jurisdiction rule in the register, attach its official original, then register a source here.</p>}
    {data?.sources.map(source => <article key={source.id} className={s.event}><strong>{source.title ?? source.record_title}</strong><p><a href={source.source_url} target="_blank" rel="noreferrer">Official source</a> · {source.state}</p></article>)}
    {selected && manage && <details className={s.event}><summary>Register an official source</summary><form className={s.detail} onSubmit={event => { event.preventDefault(); void command("law.source.register", Object.fromEntries(new FormData(event.currentTarget))); }}>
      <label className={s.field}>Source title<input name="title" required minLength={3} maxLength={300} /></label><label className={s.field}>Official HTTPS URL<input name="sourceUrl" type="url" required /></label><Reason /><button className={s.button} disabled={busy}>Register official source</button>
    </form></details>}
    {selected && manage && <details className={s.event}><summary>Import a law revision for independent review</summary><form className={s.detail} onSubmit={event => {
      event.preventDefault(); const values = new FormData(event.currentTarget);
      void command("law.revision.import", { sourceId: values.get("sourceId"), documentId: values.get("documentId"), revisionLabel: values.get("revisionLabel"), publishedOn: values.get("publishedOn"), effectiveFrom: values.get("effectiveFrom"), ...(values.get("effectiveUntil") ? { effectiveUntil: values.get("effectiveUntil") } : {}), extractionMethod: values.get("extractionMethod"), ...(values.get("inspectionId") ? { inspectionId: values.get("inspectionId") } : {}), reason: values.get("reason"), chunks: Array.from({ length: chunkCount }, (_, index) => ({ page: Number(values.get(`page:${index}`)), section: String(values.get(`section:${index}`) ?? ""), text: String(values.get(`text:${index}`)) })) });
    }}>
      <label className={s.field}>Registered official source<select name="sourceId" required>{data?.sources.filter(source => source.rule_id === ruleId).map(source => <option key={source.id} value={source.id}>{source.title}</option>)}</select></label>
      <label className={s.field}>Immutable official original<select name="documentId" required>{selected.documents.map(document => <option key={document.id} value={document.id}>{document.name} · v{document.version}</option>)}</select></label>
      <label className={s.field}>Revision label<input name="revisionLabel" required maxLength={200} /></label>
      <div className={s.grid}><label className={s.field}>Publication date<input name="publishedOn" type="date" required /></label><label className={s.field}>Effective from<input name="effectiveFrom" type="date" required /></label><label className={s.field}>Effective until, exclusive<input name="effectiveUntil" type="date" /></label></div>
      <label className={s.field}>Text extraction<select name="extractionMethod"><option value="manual">Manual transcription checked against original</option><option value="ocr">OCR inspection of this original</option></select></label>
      <label className={s.field}>OCR inspection reference, when using OCR<input name="inspectionId" /></label>
      {Array.from({ length: chunkCount }, (_, index) => <fieldset key={index} className={s.detail}><legend>Source passage {index + 1}</legend><label className={s.field}>Page<input type="number" name={`page:${index}`} min={1} max={10000} required /></label><label className={s.field}>Section or provision<input name={`section:${index}`} maxLength={200} /></label><label className={s.field}>Text from the official original<textarea name={`text:${index}`} required maxLength={12000} /></label></fieldset>)}
      <button type="button" className={s.button} disabled={chunkCount >= 300} onClick={() => setChunkCount(chunkCount + 1)}>Add source passage</button><Reason /><button className={s.button} disabled={busy || !selected.documents.length || !data?.sources.some(source => source.rule_id === ruleId)}>Save pending law revision</button>
    </form></details>}
    {data?.revisions.map(revision => <article className={s.notice} key={revision.id}><h3>{revision.record_title} · {revision.revision_label}</h3><p>{revision.state} · Published {revision.published_on} · Effective {revision.effective_from}{revision.effective_until ? ` until ${revision.effective_until}` : ""}</p>
      {selected?.item.id === revision.rule_id && canReview && revision.state === "pending" && revision.created_by !== userId && <form className={s.detail} onSubmit={event => { event.preventDefault(); const values = new FormData(event.currentTarget); void command("law.revision.approve", { revisionId: revision.id, revisionVersion: revision.version, originalVerified: values.has("originalVerified"), datesVerified: values.has("datesVerified"), textVerified: values.has("textVerified"), reason: values.get("reason") }); }}>
        {[ ["originalVerified", "Original and official publication verified"], ["datesVerified", "Publication, effective and amendment dates verified"], ["textVerified", "Every page and provision checked against the original"] ].map(([key, label]) => <label className={s.check} key={key}><input name={key} type="checkbox" required />{label}</label>)}<Reason /><button className={s.button} disabled={busy}>Approve law revision independently</button>
      </form>}
      {selected?.item.id === revision.rule_id && manage && revision.state === "approved" && <form className={s.detail} onSubmit={event => { event.preventDefault(); const values = new FormData(event.currentTarget); void command("law.applicability.propose", { revisionId: revision.id, applies: values.get("applies") === "true", basis: values.get("basis"), reason: values.get("reason") }); }}>
        <label className={s.field}>Company applicability<select name="applies"><option value="true">Applies to this Company</option><option value="false">Does not apply to this Company</option></select></label><label className={s.field}>Applicability assessment<textarea name="basis" required minLength={10} maxLength={12000} /></label><Reason /><button className={s.button} disabled={busy}>Propose applicability for independent review</button>
      </form>}
    </article>)}
    {data?.applicability.map(assessment => <article className={s.event} key={assessment.id}><strong>{assessment.applies ? "Applicable" : "Not applicable"} · {assessment.state}</strong><p>{assessment.basis}</p>{selected?.item.id === assessment.rule_id && canReview && assessment.state === "pending" && assessment.created_by !== userId && <form className={s.detail} onSubmit={event => { event.preventDefault(); void command("law.applicability.approve", { applicabilityId: assessment.id, applicabilityVersion: assessment.version, reason: new FormData(event.currentTarget).get("reason") }); }}><Reason /><button className={s.button} disabled={busy}>Approve Company applicability independently</button></form>}</article>)}
    <section className={s.detail}><h3>Official source checks</h3>{!data?.checks.length && <p>No source check has completed.</p>}{data?.checks.map(check => <article className={s.event} key={check.id}><strong>{check.record_title}</strong><p>{check.check_date} · {check.state.replaceAll("_", " ")}</p>{check.error && <p role="status">Source check failed: {check.error}. Import and review the official original manually.</p>}</article>)}</section>
  </div>;
}
function Reason() { return <label className={s.field}>Reason and evidence<textarea name="reason" required minLength={3} maxLength={4000} /></label>; }
