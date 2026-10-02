"use client";
import { useEffect, useState } from "react";
import type { ExecutedCopy, LegalDetail } from "@/lib/legal/model";
import { workFetch } from "@/lib/work-fetch";
import { LegalEvidenceFields } from "./legal-evidence-fields";
import s from "./legal.module.css";

const checks = [
  ["contentMatches", "Content matches the approved version"],
  ["pagesComplete", "All pages and annexes are present"],
  ["partiesVerified", "All parties and their details are correct"],
  ["signaturesComplete", "Every required signature is present"],
  ["authorityVerified", "Signatory authority evidence is complete and valid"],
] as const;
type Copy = ExecutedCopy & { document_name: string; document_version: number; reviews: unknown[] };
export function LegalExecutedCopies({ detail, api, call, command, busy, userId, canReview, onRefresh }: {
  detail: LegalDetail; api: string; call: (path: string) => Promise<unknown>;
  command: (body: Record<string, unknown>) => Promise<boolean>; busy: boolean; userId: string;
  canReview: boolean; onRefresh: () => Promise<void>;
}) {
  const [copies, setCopies] = useState<Copy[]>([]);
  const [error, setError] = useState("");
  const [uploading, setUploading] = useState(false);
  const [signerCount, setSignerCount] = useState(1);
  useEffect(() => {
    let active = true;
    call(`/items/${detail.item.id}/executed-copies`).then(value => { if (active) setCopies(value as Copy[]); }).catch(error => { if (active) setError(error.message); });
    return () => { active = false; };
  }, [call, detail.item.id, detail.item.version]);
  async function upload(form: HTMLFormElement) {
    setError(""); setUploading(true);
    const values = new FormData(form);
    try {
      const file = values.get("file");
      if (!(file instanceof File) || !file.size || file.size > 10 * 1024 * 1024) throw new Error("Choose a signed PDF or scanned image up to 10 MB.");
      const metadata = {
        approvedDocumentId: values.get("approvedDocumentId"), signedDate: values.get("signedDate"),
        declaredPageCount: Number(values.get("declaredPageCount")),
        parties: String(values.get("parties")).split("\n").map(value => value.trim()).filter(Boolean),
        signers: Array.from({ length: signerCount }, (_, index) => ({ name: String(values.get(`signer:${index}:name`)), party: String(values.get(`signer:${index}:party`)), authorityDocumentIds: values.getAll(`signer:${index}:authority`).map(String) })),
        notes: String(values.get("notes") ?? ""),
      };
      const checksum = Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", await file.arrayBuffer())), byte => byte.toString(16).padStart(2, "0")).join("");
      const storageKey = `erpx:legal-executed:${api}:${userId}:${detail.item.id}`;
      let attempt: { key: string; version: number; metadata: string; checksum: string; filename: string; mediaType: string } | null = null;
      try { attempt = JSON.parse(sessionStorage.getItem(storageKey) ?? "null"); } catch {}
      const text = JSON.stringify(metadata);
      if (attempt && (attempt.metadata !== text || attempt.checksum !== checksum || attempt.filename !== file.name || attempt.mediaType !== file.type)) throw new Error("Retry the original signed file and signing details to recover the previous import.");
      attempt ??= { key: crypto.randomUUID(), version: detail.item.version, metadata: text, checksum, filename: file.name, mediaType: file.type };
      try { sessionStorage.setItem(storageKey, JSON.stringify(attempt)); } catch {}
      const body = new FormData(); body.set("file", file); body.set("expectedVersion", String(attempt.version)); body.set("metadata", attempt.metadata);
      const response = await workFetch(`${api}/items/${detail.item.id}/executed-copies`, { method: "POST", headers: { "Idempotency-Key": attempt.key }, body });
      const result = await response.json();
      if (!response.ok) {
        if (response.status >= 400 && response.status < 500 && ![408, 429].includes(response.status)) { try { sessionStorage.removeItem(storageKey); } catch {} }
        throw new Error(result.error?.message ?? "Unable to import the signed copy. Retry the same file.");
      }
      try { sessionStorage.removeItem(storageKey); } catch {}
      form.reset(); await onRefresh();
    } catch (error) { setError(error instanceof Error ? error.message : "Signed-copy import failed."); }
    finally { setUploading(false); }
  }
  return <section className={s.detail} aria-label="Executed copies">
    <h3>Signed agreements</h3>
    <p className={s.small}>The approved draft and signed original are retained separately. An independent reviewer verifies content, pages, parties, signatures and authority.</p>
    {error && <p role="alert">{error}</p>}
    {copies.map(copy => <article key={copy.id} className={s.notice}>
      <h4>{copy.document_name} · v{copy.document_version} · {copy.state.replaceAll("_", " ")}</h4>
      <p>Signed {copy.metadata.signedDate} · {copy.metadata.declaredPageCount} declared pages · {copy.metadata.parties.join(" / ")}</p>
      <div className={s.actions}><a className={s.button} href={`${api}/documents/${copy.approved_document_id}`}>Approved version</a><a className={s.button} href={`${api}/documents/${copy.document_id}`}>Signed original</a></div>
      <details><summary>Signing details and review evidence</summary><LegalEvidenceFields value={{ signers: copy.metadata.signers, notes: copy.metadata.notes, reviews: copy.reviews, approvedChecksum: copy.approved_hash, signedChecksum: copy.executed_hash }} /></details>
      {canReview && copy.state !== "confirmed" && copy.imported_by !== userId && <form className={s.detail} onSubmit={event => {
        event.preventDefault(); const values = new FormData(event.currentTarget);
        void command({ action: "executed.review", executedCopyId: copy.id, decision: values.get("decision"), reason: values.get("reason"), checks: Object.fromEntries(checks.map(([key]) => [key, values.get(key) === "on"])) });
      }}>
        {checks.map(([key, label]) => <label key={key} className={s.check}><input type="checkbox" name={key} />{label}</label>)}
        <label className={s.field}>Review decision<select name="decision"><option value="changes_required">Changes or missing evidence</option><option value="confirm">Confirm executed copy</option></select></label>
        <label className={s.field}>Review reason and evidence<textarea name="reason" required minLength={10} maxLength={4000} /></label>
        <button className={s.button} disabled={busy || uploading}>Record independent copy review</button>
      </form>}
    </article>)}
    {["approved", "executed", "active"].includes(detail.item.state) && <details className={s.event}>
      <summary>Import a signed original or corrected revision</summary>
      <form className={s.detail} onSubmit={event => { event.preventDefault(); void upload(event.currentTarget); }}>
        <label className={s.field}>Approved document<select name="approvedDocumentId" required>{detail.documents.filter(document => document.locked).map(document => <option key={document.id} value={document.id}>{document.name} · v{document.version}</option>)}</select></label>
        <label className={s.field}>Signing date (Gregorian)<input name="signedDate" type="date" required /></label>
        <label className={s.field}>Parties, one per line<textarea name="parties" required /></label>
        <label className={s.field}>Number of pages, including annexes<input name="declaredPageCount" type="number" min={1} max={1000} required /></label>
        {Array.from({ length: signerCount }, (_, index) => <fieldset key={index} className={s.detail}>
          <legend>Signer {index + 1}</legend>
          <label className={s.field}>Signer name<input name={`signer:${index}:name`} required /></label>
          <label className={s.field}>Party represented<input name={`signer:${index}:party`} required /></label>
          <label className={s.field}>Authority evidence for signer {index + 1}<select multiple name={`signer:${index}:authority`} required>{detail.documents.map(document => <option key={document.id} value={document.id}>{document.name} · v{document.version}</option>)}</select></label>
        </fieldset>)}
        <div className={s.actions}><button className={s.button} type="button" disabled={signerCount >= 30} onClick={() => setSignerCount(signerCount + 1)}>Add signer</button>{signerCount > 1 && <button className={s.button} type="button" onClick={() => setSignerCount(signerCount - 1)}>Remove last signer</button>}</div>
        <label className={s.field}>Notes<textarea name="notes" maxLength={4000} /></label>
        <label className={s.field}>Signed PDF or scanned image<input name="file" type="file" accept=".pdf,.png,.jpg,.jpeg" required /></label>
        <button className={s.button} disabled={busy || uploading || !detail.documents.some(document => document.locked)}>{uploading ? "Importing…" : "Import for independent review"}</button>
      </form>
    </details>}
  </section>;
}
