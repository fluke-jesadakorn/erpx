"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useEffect, useState } from "react";
import type { LegalDetail } from "@/server/modules/legal/public/rules/model";
import s from "./legal.module.css";

type Review = {
  id: string;
  version: number;
  document_id: string;
  state: string;
  created_by: string;
};
type Chunk = {
  id: string;
  text_review_id: string | null;
  page: number;
  page_mapping: boolean;
  section: string;
  text: string;
};
export function LegalDocumentTextReview({
  detail,
  call,
  command,
  busy,
  userId,
  canReview,
  inspection,
}: {
  detail: LegalDetail;
  call: (path: string) => Promise<unknown>;
  command: (body: Record<string, unknown>) => Promise<boolean>;
  busy: boolean;
  userId: string;
  canReview: boolean;
  inspection: unknown;
}) {
  const [reviews, setReviews] = useState<Review[]>([]);
  const [chunks, setChunks] = useState<Chunk[]>([]);
  const [error, setError] = useState("");
  useEffect(() => {
    let active = true;
    call(`/laws?ruleId=${encodeURIComponent(detail.item.id)}`)
      .then((value) => {
        if (active) {
          const data = value as { textReviews: Review[]; chunks: Chunk[] };
          setReviews(data.textReviews);
          setChunks(data.chunks);
        }
      })
      .catch((error) => {
        if (active) setError(error.message);
      });
    return () => {
      active = false;
    };
  }, [call, detail.item.id, detail.item.version]);
  const result = (
    inspection as {
      result?: { inspectionId?: string; text?: string; pages?: { page: number; text: string }[] };
    } | null
  )?.result;
  const documentId = (inspection as { document_id?: string } | null)?.document_id;
  return (
    <section className={s.detail} aria-label="Reviewed document text">
      <h3>Text for knowledge and AI</h3>
      <p className={s.small}>
        Extracted PDF, DOCX or scanned text requires an independent check against the immutable
        original. Printed page references require verified pagination; DOCX and plain text use
        logical segments.
      </p>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {reviews.map((review) => (
        <article className={s.notice} key={review.id}>
          <strong>
            {detail.documents.find((document) => document.id === review.document_id)?.name ??
              "Document text"}{" "}
            · {review.state}
          </strong>
          {chunks
            .filter((chunk) => chunk.text_review_id === review.id)
            .map((chunk) => (
              <blockquote key={chunk.id}>
                <p className={s.small}>
                  {chunk.page_mapping ? "Page" : "Logical segment"} {chunk.page}
                  {chunk.section ? ` · ${chunk.section}` : ""}
                </p>
                <p className={s.body}>{chunk.text}</p>
              </blockquote>
            ))}
          {canReview && review.state === "pending" && review.created_by !== userId && (
            <form
              className={s.detail}
              onSubmit={(event) => {
                event.preventDefault();
                const values = new FormData(event.currentTarget);
                void command({
                  action: "knowledge.document.approve",
                  textReviewId: review.id,
                  textReviewVersion: review.version,
                  textVerified: values.has("textVerified"),
                  reason: values.get("reason"),
                });
              }}
            >
              <Field appearance="standard" orientation="inline" className={s.check}>
                <TextInput appearance="standard" name="textVerified" type="checkbox" required />
                Text, pages and sections checked against the original
              </Field>
              <Field appearance="standard" className={s.field}>
                Independent review reason
                <TextArea appearance="standard" name="reason" required minLength={3} maxLength={4000} />
              </Field>
              <Button appearance="standard" clientManaged type="submit" variant="ghost" className={s.button} disabled={busy}>
                Approve document text independently
              </Button>
            </form>
          )}
        </article>
      ))}
      <FormDisclosure appearance="standard" className={s.event}>
        <summary>Prepare a source passage for review</summary>
        <form
          key={result?.inspectionId ?? "manual"}
          className={s.detail}
          onSubmit={(event) => {
            event.preventDefault();
            const values = new FormData(event.currentTarget);
            const inspected = Boolean(
              result?.inspectionId && values.get("documentId") === documentId,
            );
            void command({
              action: "knowledge.document.import",
              documentId: values.get("documentId"),
              extractionMethod: inspected ? "ocr" : "manual",
              ...(inspected ? { inspectionId: result!.inspectionId } : {}),
              reason: values.get("reason"),
              chunks: [
                {
                  page: Number(values.get("page")),
                  section: String(values.get("section") ?? ""),
                  text: values.get("text"),
                },
              ],
            });
          }}
        >
          <Field appearance="standard" className={s.field}>
            Original document
            <Select appearance="standard" name="documentId" defaultValue={documentId} required>
              {detail.documents.map((document) => (
                <option key={document.id} value={document.id}>
                  {document.name} · v{document.version}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard" className={s.field}>
            Original page or logical segment
            <TextInput appearance="standard" name="page" type="number" required min={1} max={10000} defaultValue={1} />
          </Field>
          <Field appearance="standard" className={s.field}>
            Clause or section
            <TextInput appearance="standard" name="section" maxLength={200} />
          </Field>
          <Field appearance="standard" className={s.field}>
            Text checked against the original
            <TextArea appearance="standard"
              name="text"
              required
              maxLength={12000}
              defaultValue={result?.pages?.[0]?.text ?? result?.text?.slice(0, 12000)}
            />
          </Field>
          <Field appearance="standard" className={s.field}>
            Preparation reason
            <TextArea appearance="standard" name="reason" required minLength={3} maxLength={4000} />
          </Field>
          <Button appearance="standard"
            clientManaged type="submit"
            variant="ghost"
            className={s.button}
            disabled={busy || !detail.documents.length}
          >
            Save pending text review
          </Button>
        </form>
      </FormDisclosure>
    </section>
  );
}
