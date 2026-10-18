"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useEffect, useRef,useState } from "react";
import type { ExecutedCopy, LegalDetail } from "@/server/modules/legal/public/rules/model";
import { workFetch } from "@/lib/work_projection/work-fetch";
import {acknowledgeLegalCommand} from '@/lib/shared/legal/command-completion';
import {legalDocumentUploadAcknowledged} from '@/lib/legal/document-upload-client';
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
type ImportAttempt={key:string;version:number;metadata:string;checksum:string;filename:string;mediaType:string;uncertain?:boolean};
export function LegalExecutedCopies({
  detail,
  api,
  call,
  command,
  busy,
  userId,
  contextKey,
  canReview,
  onRefresh,
}: {
  detail: LegalDetail;
  api: string;
  call: (path: string) => Promise<unknown>;
  command: (body: Record<string, unknown>) => Promise<boolean>;
  busy: boolean;
  userId: string;
  contextKey:string;
  canReview: boolean;
  onRefresh: () => Promise<void>;
}) {
  const [copies, setCopies] = useState<Copy[]>([]);
  const [error, setError] = useState("");
  const [uploading, setUploading] = useState(false);
  const [signerCount, setSignerCount] = useState(1);
  const [pendingImport,setPendingImport]=useState(false),attemptRef=useRef<ImportAttempt|null>(null),uploadingRef=useRef(false),storageKey=`${contextKey}:executed:${detail.item.id}`;
  const settlement=!detail.purchases||detail.purchases['legal.contracts']?.settleExisting===true&&detail.purchases['legal.documents']?.settleExisting===true;
  useEffect(()=>{const frame=requestAnimationFrame(()=>{try{setPendingImport(Boolean(attemptRef.current||sessionStorage.getItem(storageKey)));}catch{setPendingImport(Boolean(attemptRef.current));}});return()=>cancelAnimationFrame(frame);},[storageKey]);
  useEffect(() => {
    let active = true;
    call(`/items/${detail.item.id}/executed-copies`)
      .then((value) => {
        if (active) setCopies(value as Copy[]);
      })
      .catch((error) => {
        if (active) setError(error.message);
      });
    return () => {
      active = false;
    };
  }, [call, detail.item.id, detail.item.version]);
  async function upload(form: HTMLFormElement) {
    if(uploadingRef.current||busy)return;if(!settlement&&!pendingImport){setError('The retained executed-copy action is currently unavailable. Your draft is retained.');return;}uploadingRef.current=true;
    const values = new FormData(form);
    const completion = { form, formEditVersion: form.getAttribute("data-erpx-edit-version") };
    setError("");
    setUploading(true);
    let acknowledged=false;try {
      const file = values.get("file");
      if (!(file instanceof File) || !file.size || file.size > 10 * 1024 * 1024)
        throw new Error("Choose a signed PDF or scanned image up to 10 MB.");
      const metadata = {
        approvedDocumentId: values.get("approvedDocumentId"),
        signedDate: values.get("signedDate"),
        declaredPageCount: Number(values.get("declaredPageCount")),
        parties: String(values.get("parties"))
          .split("\n")
          .map((value) => value.trim())
          .filter(Boolean),
        signers: Array.from({ length: signerCount }, (_, index) => ({
          name: String(values.get(`signer:${index}:name`)),
          party: String(values.get(`signer:${index}:party`)),
          authorityDocumentIds: values.getAll(`signer:${index}:authority`).map(String),
        })),
        notes: String(values.get("notes") ?? ""),
      };
      const checksum = Array.from(
        new Uint8Array(await crypto.subtle.digest("SHA-256", await file.arrayBuffer())),
        (byte) => byte.toString(16).padStart(2, "0"),
      ).join("");
      let attempt=attemptRef.current;
      try {
        attempt ??= JSON.parse(sessionStorage.getItem(storageKey) ?? "null");
      } catch {}
      const text = JSON.stringify(metadata);
      if (
        attempt &&
        (attempt.metadata !== text ||
          attempt.checksum !== checksum ||
          attempt.filename !== file.name ||
          attempt.mediaType !== file.type)
      )
        throw new Error(
          "Retry the original signed file and signing details to recover the previous import.",
        );
      attempt ??= {
        key: crypto.randomUUID(),
        version: detail.item.version,
        metadata: text,
        checksum,
        filename: file.name,
        mediaType: file.type,
      };
      attempt.uncertain=true;
      attemptRef.current=attempt;setPendingImport(true);
      try {
        sessionStorage.setItem(storageKey, JSON.stringify(attempt));
      } catch {}
      const body = new FormData();
      body.set("file", file);
      body.set("expectedVersion", String(attempt.version));
      body.set("metadata", attempt.metadata);
      const response = await workFetch(`${api}/items/${detail.item.id}/executed-copies`, {
        method: "POST",
        headers: { "Idempotency-Key": attempt.key },
        body,
      }, completion,'caller');
      const result = await response.json();
      if (!response.ok) {
        if (
          response.status >= 400 &&
          response.status < 500 &&
          ![408, 429].includes(response.status)&&!(attempt.uncertain&&[401,403,404,409].includes(response.status))
        ) {
          try {
            sessionStorage.removeItem(storageKey);
          } catch {}
          attemptRef.current=null;setPendingImport(false);
        }
        throw new Error(
          result.error?.message ?? "Unable to import the signed copy. Retry the same file.",
        );
      }
      if(!legalDocumentUploadAcknowledged(result?.data,{id:detail.item.id,version:attempt.version},true))throw new Error('The response did not acknowledge this exact signed original. Retry the same file and signing details.');
      acknowledged=true;acknowledgeLegalCommand(completion);
      attemptRef.current=null;setPendingImport(false);
      try {
        sessionStorage.removeItem(storageKey);
      } catch {}
      if (completion.formEditVersion === form.getAttribute("data-erpx-edit-version")) form.reset();
      await onRefresh();
    } catch (error) {
      setError(`${acknowledged?'The signed original was saved, but its refreshed view is unavailable. ':''}${error instanceof Error ? error.message : "Signed-copy import failed."}`);
    } finally {
      uploadingRef.current=false;
      setUploading(false);
    }
  }
  return (
    <section className={s.detail} aria-label="Executed copies">
      <h3>Signed agreements</h3>
      <p className={s.small}>
        The approved draft and signed original are retained separately. An independent reviewer
        verifies content, pages, parties, signatures and authority.
      </p>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {copies.map((copy) => (
        <article key={copy.id} className={s.notice}>
          <h4>
            {copy.document_name} · v{copy.document_version} · {copy.state.replaceAll("_", " ")}
          </h4>
          <p>
            Signed {copy.metadata.signedDate} · {copy.metadata.declaredPageCount} declared pages ·{" "}
            {copy.metadata.parties.join(" / ")}
          </p>
          <div className={s.actions}>
            <a data-slot="button" className={['ui-standard-control', (s.button)].filter(Boolean).join(' ')} href={`${api}/documents/${copy.approved_document_id}`}>
              Approved version
            </a>
            <a data-slot="button" className={['ui-standard-control', (s.button)].filter(Boolean).join(' ')} href={`${api}/documents/${copy.document_id}`}>
              Signed original
            </a>
          </div>
          <details>
            <summary>Signing details and review evidence</summary>
            <LegalEvidenceFields
              value={{
                signers: copy.metadata.signers,
                notes: copy.metadata.notes,
                reviews: copy.reviews,
                approvedChecksum: copy.approved_hash,
                signedChecksum: copy.executed_hash,
              }}
            />
          </details>
          {canReview && copy.state !== "confirmed" && copy.imported_by !== userId && (
            <form
              className={s.detail}
              onSubmit={(event) => {
                event.preventDefault();
                const values = new FormData(event.currentTarget);
                void command({
                  action: "executed.review",
                  executedCopyId: copy.id,
                  decision: values.get("decision"),
                  reason: values.get("reason"),
                  checks: Object.fromEntries(
                    checks.map(([key]) => [key, values.get(key) === "on"]),
                  ),
                });
              }}
            >
              {checks.map(([key, label]) => (
                <Field appearance="standard" orientation="inline" key={key} className={s.check}>
                  <TextInput appearance="standard" type="checkbox" name={key} />
                  {label}
                </Field>
              ))}
              <Field appearance="standard" className={s.field}>
                Review decision
                <Select appearance="standard" name="decision">
                  <option value="changes_required">Changes or missing evidence</option>
                  <option value="confirm">Confirm executed copy</option>
                </Select>
              </Field>
              <Field appearance="standard" className={s.field}>
                Review reason and evidence
                <TextArea appearance="standard" name="reason" required minLength={10} maxLength={4000} />
              </Field>
              <Button appearance="standard"
                clientManaged type="submit"
                variant="ghost"
                className={s.button}
                disabled={busy || uploading||!settlement}
              >
                Record independent copy review
              </Button>
            </form>
          )}
        </article>
      ))}
      {["approved", "executed", "active"].includes(detail.item.state) && (
        <FormDisclosure appearance="standard" className={s.event} dismissDisabled={busy||uploading} error={error}>
          <summary>Import a signed original or corrected revision</summary>
          <form
            className={s.detail}
            onSubmit={(event) => {
              event.preventDefault();
              void upload(event.currentTarget);
            }}
          >
            <Field appearance="standard" className={s.field}>
              Approved document
              <Select appearance="standard" name="approvedDocumentId" required>
                {detail.documents
                  .filter((document) => document.locked)
                  .map((document) => (
                    <option key={document.id} value={document.id}>
                      {document.name} · v{document.version}
                    </option>
                  ))}
              </Select>
            </Field>
            <Field appearance="standard" className={s.field}>
              Signing date (Gregorian)
              <TextInput appearance="standard" name="signedDate" type="date" required />
            </Field>
            <Field appearance="standard" className={s.field}>
              Parties, one per line
              <TextArea appearance="standard" name="parties" required />
            </Field>
            <Field appearance="standard" className={s.field}>
              Number of pages, including annexes
              <TextInput appearance="standard" name="declaredPageCount" type="number" min={1} max={1000} required />
            </Field>
            {Array.from({ length: signerCount }, (_, index) => (
              <fieldset key={index} className={s.detail}>
                <legend>Signer {index + 1}</legend>
                <Field appearance="standard" className={s.field}>
                  Signer name
                  <TextInput appearance="standard" name={`signer:${index}:name`} required />
                </Field>
                <Field appearance="standard" className={s.field}>
                  Party represented
                  <TextInput appearance="standard" name={`signer:${index}:party`} required />
                </Field>
                <Field appearance="standard" className={s.field}>
                  Authority evidence for signer {index + 1}
                  <Select appearance="standard" multiple name={`signer:${index}:authority`} required>
                    {detail.documents.map((document) => (
                      <option key={document.id} value={document.id}>
                        {document.name} · v{document.version}
                      </option>
                    ))}
                  </Select>
                </Field>
              </fieldset>
            ))}
            <div className={s.actions}>
              <Button appearance="standard"
                variant="ghost"
                className={s.button}
                type="button"
                disabled={signerCount >= 30}
                onClick={() => setSignerCount(signerCount + 1)}
              >
                Add signer
              </Button>
              {signerCount > 1 && (
                <Button appearance="standard"
                  variant="ghost"
                  className={s.button}
                  type="button"
                  onClick={() => setSignerCount(signerCount - 1)}
                >
                  Remove last signer
                </Button>
              )}
            </div>
            <Field appearance="standard" className={s.field}>
              Notes
              <TextArea appearance="standard" name="notes" maxLength={4000} />
            </Field>
            <Field appearance="standard" className={s.field}>
              Signed PDF or scanned image
              <TextInput appearance="standard" name="file" type="file" accept=".pdf,.png,.jpg,.jpeg" required />
            </Field>
            <Button appearance="standard"
              clientManaged type="submit"
              variant="ghost"
              className={s.button}
              disabled={busy || uploading || !settlement&&!pendingImport || !detail.documents.some((document) => document.locked)}
            >
              {uploading ? "Importing…" : "Import for independent review"}
            </Button>
          </form>
        </FormDisclosure>
      )}
    </section>
  );
}
