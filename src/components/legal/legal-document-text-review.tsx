"use client";
import { useEffect, useState } from "react";
import type { LegalDetail } from "@/lib/legal/model";
import s from "./legal.module.css";

type Review = { id: string; version: number; document_id: string; state: string; created_by: string };
type Chunk = { id: string; text_review_id: string | null; page: number; page_mapping: boolean; section: string; text: string };
export function LegalDocumentTextReview({ detail, call, command, busy, userId, canReview, inspection }: {
  detail: LegalDetail; call: (path: string) => Promise<unknown>; command: (body: Record<string, unknown>) => Promise<boolean>;
  busy: boolean; userId: string; canReview: boolean; inspection: unknown;
}) {
  const [reviews, setReviews] = useState<Review[]>([]);
  const [chunks, setChunks] = useState<Chunk[]>([]);
  const [error, setError] = useState("");
  useEffect(() => {
    let active = true;
    call(`/laws?ruleId=${encodeURIComponent(detail.item.id)}`).then(value => {
      if (active) { const data = value as { textReviews: Review[]; chunks: Chunk[] }; setReviews(data.textReviews); setChunks(data.chunks); }
    }).catch(error => { if (active) setError(error.message); });
    return () => { active = false; };
  }, [call, detail.item.id, detail.item.version]);
  const result = (inspection as { result?: { inspectionId?: string; text?: string; pages?: { page: number; text: string }[] } } | null)?.result;
  const documentId = (inspection as { document_id?: string } | null)?.document_id;
  return <section className={s.detail} aria-label="Reviewed document text"><h3>Text for knowledge and AI</h3>
    <p className={s.small}>Extracted PDF, DOCX or scanned text requires an independent check against the immutable original. Printed page references require verified pagination; DOCX and plain text use logical segments.</p>
    {error && <p role="alert">{error}</p>}
    {reviews.map(review => <article className={s.notice} key={review.id}><strong>{detail.documents.find(document => document.id === review.document_id)?.name ?? "Document text"} · {review.state}</strong>
      {chunks.filter(chunk => chunk.text_review_id === review.id).map(chunk => <blockquote key={chunk.id}><p className={s.small}>{chunk.page_mapping ? "Page" : "Logical segment"} {chunk.page}{chunk.section ? ` · ${chunk.section}` : ""}</p><p className={s.body}>{chunk.text}</p></blockquote>)}
      {canReview && review.state === "pending" && review.created_by !== userId && <form className={s.detail} onSubmit={event => { event.preventDefault(); const values = new FormData(event.currentTarget); void command({ action: "knowledge.document.approve", textReviewId: review.id, textReviewVersion: review.version, textVerified: values.has("textVerified"), reason: values.get("reason") }); }}><label className={s.check}><input name="textVerified" type="checkbox" required />Text, pages and sections checked against the original</label><label className={s.field}>Independent review reason<textarea name="reason" required minLength={3} maxLength={4000} /></label><button className={s.button} disabled={busy}>Approve document text independently</button></form>}
    </article>)}
    <details className={s.event}><summary>Prepare a source passage for review</summary><form key={result?.inspectionId ?? "manual"} className={s.detail} onSubmit={event => {
      event.preventDefault(); const values = new FormData(event.currentTarget);
      const inspected = Boolean(result?.inspectionId && values.get("documentId") === documentId);
      void command({ action: "knowledge.document.import", documentId: values.get("documentId"), extractionMethod: inspected ? "ocr" : "manual", ...(inspected ? { inspectionId: result!.inspectionId } : {}), reason: values.get("reason"), chunks: [{ page: Number(values.get("page")), section: String(values.get("section") ?? ""), text: values.get("text") }] });
    }}>
      <label className={s.field}>Original document<select name="documentId" defaultValue={documentId} required>{detail.documents.map(document => <option key={document.id} value={document.id}>{document.name} · v{document.version}</option>)}</select></label>
      <label className={s.field}>Original page or logical segment<input name="page" type="number" required min={1} max={10000} defaultValue={1} /></label><label className={s.field}>Clause or section<input name="section" maxLength={200} /></label>
      <label className={s.field}>Text checked against the original<textarea name="text" required maxLength={12000} defaultValue={result?.pages?.[0]?.text ?? result?.text?.slice(0, 12000)} /></label>
      <label className={s.field}>Preparation reason<textarea name="reason" required minLength={3} maxLength={4000} /></label><button className={s.button} disabled={busy || !detail.documents.length}>Save pending text review</button>
    </form></details>
  </section>;
}
