"use client";

import Link from "next/link";
import { useCallback, useEffect, useRef, useState } from "react";
import { handoverSuggestion } from "@/lib/handover";
import { workFetch } from "@/lib/work-fetch";
import type { HandoverView } from "@/server/handover";
import styles from "./handover-composer.module.css";

async function responseData<T>(response: Response): Promise<T> {
  const body = await response.json() as { data?: T; error?: { message?: string } };
  if (!response.ok || body.data === undefined)
    throw new Error(body.error?.message ?? "The handover could not be loaded. Try again.");
  return body.data;
}
function draftKey(base:string,ownerKey:string,date:string){
  return `erpx:handover-draft:${base}:${ownerKey}:${date}`;
}

export function HandoverComposer({ base, onPublished }: { base: string; onPublished: () => Promise<void> }) {
  const [open, setOpen] = useState(false);
  const [view, setView] = useState<HandoverView | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [recipients, setRecipients] = useState<string[]>([]);
  const [search, setSearch] = useState("");
  const [body, setBody] = useState("");
  const [edited, setEdited] = useState(false);
  const [reviewing, setReviewing] = useState(false);
  const [loading, setLoading] = useState(false);
  const [publishing, setPublishing] = useState(false);
  const [error, setError] = useState("");
  const [publishedId, setPublishedId] = useState<string | null>(null);
  const attempt = useRef<{ signature: string; key: string } | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const response = await workFetch(`/api/v1${base}/handover`, { cache: "no-store" });
      const next = await responseData<HandoverView>(response);
      setView(next);
      // A handover never inherits the audience from an earlier post or draft.
      setRecipients([]);
      let ids=next.defaultIds, restoredBody:string|null=null;
      try {
        const saved=sessionStorage.getItem(draftKey(base,next.ownerKey,next.date));
        if(saved){
          const draft=JSON.parse(saved) as {selected?:unknown;body?:unknown};
          if(Array.isArray(draft.selected)) ids=draft.selected.filter((id):id is string=>typeof id==="string"&&next.items.some(item=>item.id===id)).slice(0,5);
          if(typeof draft.body==="string"&&draft.body.length<=3000)restoredBody=draft.body;
        }
      } catch { /* Private in-tab backup is optional. */ }
      setSelected(ids);
      setBody(restoredBody??handoverSuggestion(next.date,next.items.filter(item=>ids.includes(item.id))));
      setEdited(restoredBody!==null);
      setReviewing(false);
      attempt.current = null;
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "The handover could not be loaded.");
    } finally { setLoading(false); }
  }, [base]);

  useEffect(() => {
    const fromHash = () => {
      if (window.location.hash === "#handover") setOpen(true);
    };
    fromHash();
    window.addEventListener("hashchange", fromHash);
    return () => window.removeEventListener("hashchange", fromHash);
  }, []);
  useEffect(() => {
    if (!open || view || loading || error) return;
    const timer = window.setTimeout(() => void load(), 0);
    return () => window.clearTimeout(timer);
  }, [open, view, loading, error, load]);
  useEffect(()=>{
    if(!view||!open)return;
    try { sessionStorage.setItem(draftKey(base,view.ownerKey,view.date),JSON.stringify({selected,body})); }
    catch { /* Publishing remains available without browser storage. */ }
  },[base,view,open,selected,body]);

  function toggleSource(id: string) {
    const next = selected.includes(id) ? selected.filter(value => value !== id)
      : selected.length < 5 ? [...selected, id] : selected;
    setSelected(next);
    setReviewing(false);
    setError("");
    if (!edited && view) setBody(handoverSuggestion(view.date, view.items.filter(item => next.includes(item.id))));
  }

  async function publish() {
    if (!view || !selected.length || !recipients.length || !body.trim() || publishing) return;
    const sources = selected.flatMap(id => {
      const item = view.items.find(candidate => candidate.id === id);
      return item ? [{ id, sourceVersion: item.sourceVersion,kind:item.sourceKind??"staff_task" }] : [];
    });
    const payload = JSON.stringify({ date: view.date, body, sources, recipientIds: recipients });
    if (attempt.current?.signature !== payload)
      attempt.current = { signature: payload, key: crypto.randomUUID() };
    setPublishing(true);
    setError("");
    try {
      const response = await workFetch(`/api/v1${base}/handover`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
        body: payload,
      });
      const result = await responseData<{ id: string }>(response);
      try { sessionStorage.removeItem(draftKey(base,view.ownerKey,view.date)); } catch { /* Optional local backup. */ }
      setPublishedId(result.id);
      setReviewing(false);
      setRecipients([]);
      setView(null);
      setOpen(false);
      await onPublished().catch(()=>{ /* The published post link remains available if the feed refresh fails. */ });
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "The handover could not be published.");
    } finally { setPublishing(false); }
  }

  const selectedPeople = view?.people.filter(person => recipients.includes(person.id)) ?? [];
  const people = view?.people.filter(person => `${person.name} ${person.id}`.toLowerCase().includes(search.toLowerCase())) ?? [];
  return <section id="handover" className={styles.handover} data-open={open} aria-labelledby="handover-title">
    <div className={styles.header}>
      <div><h2 id="handover-title">{open ? "Pass work to the next person" : "Pass on work"}</h2>
        {open && <p>Prepare a shift handover from tasks you can open. Publishing leaves task status unchanged.</p>}</div>
      <button type="button" aria-expanded={open} aria-controls="handover-editor"
        onClick={() => setOpen(value => !value)}>{open ? "Close" : "Draft handover"}</button>
    </div>
    {publishedId && <p className={styles.success} role="status">Handover published. <Link href={`${base}/posts/${publishedId}`}>Open post</Link></p>}
    <div id="handover-editor" hidden={!open} className={styles.editor}>
      {loading && <p role="status">Loading readable tasks…</p>}
      {error && <p className={styles.error} role="alert">{error} <button type="button" onClick={() => void load()}>Refresh sources</button></p>}
      {!loading && view?.unavailable && <p className={styles.notice}>{view.reason}</p>}
      {!loading && view && !view.unavailable && <>
        {!reviewing ? <>
          <p className={styles.note}>Select up to five linked tasks. The recipients must already be able to open every selected task.</p>
          <fieldset className={styles.tasks}><legend>Work to include · {selected.length}/5</legend>
            {view.items.map(item => <label key={item.id} className={styles.choice}>
              <input type="checkbox" checked={selected.includes(item.id)} disabled={!selected.includes(item.id) && selected.length >= 5}
                onChange={() => toggleSource(item.id)} />
              <span><strong>{item.title}</strong><small>{item.kind === "blocked" ? "Needs help" : item.kind === "completed" ? "Completed" : "Still in progress"}{item.planned ? " · In today's plan" : ""} · <Link href={`${base}${item.href}`}>Open task</Link></small></span>
            </label>)}
          </fieldset>
          <label className={styles.message}>Handover message
            <textarea rows={8} maxLength={3000} value={body} onChange={event => {
              setBody(event.target.value); setEdited(true); setReviewing(false);
            }} />
          </label>
          <fieldset className={styles.people}><legend>Who should receive this? · {recipients.length}/20</legend>
            <input type="search" aria-label="Find recipients" value={search} onChange={event => setSearch(event.target.value)} placeholder="Find a colleague" />
            {view.people.length === 0 && <p>No eligible colleagues are available in this work scope.</p>}
            {people.map(person => <label key={person.id} className={styles.person}>
              <input type="checkbox" checked={recipients.includes(person.id)}
                disabled={!recipients.includes(person.id) && recipients.length >= 20}
                onChange={() => { setRecipients(current => current.includes(person.id) ? current.filter(id => id !== person.id) : [...current, person.id]); setReviewing(false); }} />
              {person.name}
            </label>)}
          </fieldset>
          <div className={styles.actions}><button type="button" disabled={!selected.length || !recipients.length || !body.trim() || publishing}
            onClick={() => setReviewing(true)}>Review handover</button></div>
        </> : <div className={styles.review} aria-label="Review handover">
          <h3>Review before publishing</h3>
          <p><strong>Audience:</strong> {selectedPeople.map(person => person.name).join(", ")}</p>
          <p><strong>Linked work:</strong> {selected.length} {selected.length === 1 ? "task" : "tasks"}</p>
          <pre>{body}</pre>
          <p className={styles.note}>Recipients will see this post only while they can open its linked work. Reading or thanking it does not acknowledge the handover.</p>
          <div className={styles.actions}>
            <button type="button" disabled={publishing} onClick={() => setReviewing(false)}>Edit</button>
            <button type="button" disabled={publishing} onClick={() => void publish()}>{publishing ? "Publishing…" : "Publish handover"}</button>
          </div>
        </div>}
      </>}
    </div>
  </section>;
}
