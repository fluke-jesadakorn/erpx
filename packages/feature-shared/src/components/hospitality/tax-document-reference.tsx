"use client";
import { Button, Field, TextInput } from "@erpx/ui/components/foundation";

import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";

type Document = {
  id: string;
  issuer: string;
  external_reference: string;
  issued_on: string;
  document_total_minor: number | null;
  document_tax_minor: number | null;
  state: string;
  version: number;
  recorded_by: string;
  verified_by: string | null;
  reason: string;
};
type View = {
  invoice: { id: string; currency: string; totalMinor: number; taxMinor: number };
  documents: Document[];
  canRecord: boolean;
  canReview: boolean;
  userId: string;
};

export function TaxDocumentReference({ base, invoiceId, endpoint }: { base: string; invoiceId: string; endpoint?: string }) {
  const [view, setView] = useState<View | null>(null),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false);
  const command = useRef<{ fingerprint: string; key: string } | null>(null);
  const url = endpoint ?? `/api/v1${base}/hospitality-tax-documents`;
  const refresh = useCallback(async () => {
    const response = await workFetch(`${url}?invoiceId=${encodeURIComponent(invoiceId)}`, {
      cache: "no-store",
    });
    const body = await response.json();
    if (!response.ok) throw new Error(body.error?.message ?? "Unable to load document references.");
    setView(body.data);
  }, [url, invoiceId]);
  useEffect(() => {
    const controller = new AbortController();
    void workFetch(`${url}?invoiceId=${encodeURIComponent(invoiceId)}`, {
      cache: "no-store",
      signal: controller.signal,
    })
      .then(async (response) => {
        const body = await response.json();
        if (!response.ok)
          throw new Error(body.error?.message ?? "Unable to load document references.");
        setView(body.data);
      })
      .catch((e) => {
        if (!controller.signal.aborted) setError(e.message);
      });
    return () => controller.abort();
  }, [url, invoiceId]);
  async function submit(input: Record<string, unknown>) {
    const fingerprint = JSON.stringify(input);
    if (command.current?.fingerprint !== fingerprint)
      command.current = { fingerprint, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(url, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": command.current.key },
        body: JSON.stringify(input),
      });
      const body = await response.json();
      if (!response.ok)
        throw new Error(body.error?.message ?? "Unable to save document reference.");
      command.current = null;
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unable to save document reference.");
    } finally {
      setBusy(false);
    }
  }
  const active = view?.documents.find((doc) => doc.state !== "voided");
  return (
    <section aria-label="External tax document reference">
      <h3>External tax document</h3>
      <p>
        ERPX issues an operational receipt only. Record the reference from the hotel’s existing
        tax-invoice process; Finance verifies it against this invoice.
      </p>
      {error && <p role="alert">{error}</p>}
      {!view && !error && <p>Loading document references…</p>}
      {view && (
        <>
          <p>
            Accounting total {(view.invoice.totalMinor / 100).toFixed(2)} {view.invoice.currency} ·
            Tax {(view.invoice.taxMinor / 100).toFixed(2)} {view.invoice.currency}
          </p>
          {active ? (
            <div>
              <strong>
                {active.issuer} · {active.external_reference}
              </strong>
              <p>
                Issued {active.issued_on} · {active.state}
              </p>
              <p>
                External document total{" "}
                {active.document_total_minor === null
                  ? "Not recorded"
                  : (active.document_total_minor / 100).toFixed(2)}{" "}
                {view.invoice.currency} · Tax{" "}
                {active.document_tax_minor === null
                  ? "Not recorded"
                  : (active.document_tax_minor / 100).toFixed(2)}{" "}
                {view.invoice.currency}
              </p>
              {(active.document_total_minor !== view.invoice.totalMinor ||
                active.document_tax_minor !== view.invoice.taxMinor) && (
                <p role="alert">
                  External document figures do not match this invoice. Finance cannot verify this
                  reference.
                </p>
              )}
              {active.state === "recorded" &&
                view.canReview &&
                active.recorded_by !== view.userId && (
                  <Button
                    variant="plain"
                    type="button"
                    disabled={busy}
                    onClick={() =>
                      void submit({
                        action: "verify",
                        invoiceId,
                        documentId: active.id,
                        expectedVersion: active.version,
                      })
                    }
                  >
                    Finance: verify reference
                  </Button>
                )}
              {view.canReview && (
                <form
                  onSubmit={(event: FormEvent<HTMLFormElement>) => {
                    event.preventDefault();
                    const fields = new FormData(event.currentTarget);
                    void submit({
                      action: "void",
                      invoiceId,
                      documentId: active.id,
                      expectedVersion: active.version,
                      reason: String(fields.get("reason") ?? ""),
                    });
                  }}
                >
                  <Field>
                    Correction or void reason{" "}
                    <TextInput name="reason" minLength={3} maxLength={500} required />
                  </Field>
                  <Button clientManaged variant="plain" type="submit" disabled={busy}>
                    Void reference
                  </Button>
                </form>
              )}
            </div>
          ) : view.canRecord ? (
            <form
              onSubmit={(event: FormEvent<HTMLFormElement>) => {
                event.preventDefault();
                const fields = new FormData(event.currentTarget);
                void submit({
                  action: "record",
                  invoiceId,
                  issuer: String(fields.get("issuer") ?? ""),
                  externalReference: String(fields.get("reference") ?? ""),
                  issuedOn: String(fields.get("issuedOn") ?? ""),
                  documentTotalMinor: Math.round(Number(fields.get("documentTotal")) * 100),
                  documentTaxMinor: Math.round(Number(fields.get("documentTax")) * 100),
                  reason: String(fields.get("reason") ?? ""),
                });
              }}
            >
              <Field>
                Verified external issuer{" "}
                <TextInput name="issuer" minLength={2} maxLength={160} required />
              </Field>
              <Field>
                Document reference{" "}
                <TextInput name="reference" minLength={2} maxLength={100} required />
              </Field>
              <Field>
                Issue date <TextInput name="issuedOn" type="date" required />
              </Field>
              <Field>
                External document total{" "}
                <TextInput name="documentTotal" type="number" min="0" step="0.01" required />
              </Field>
              <Field>
                External document tax{" "}
                <TextInput name="documentTax" type="number" min="0" step="0.01" required />
              </Field>
              <Field>
                Evidence note <TextInput name="reason" minLength={3} maxLength={500} required />
              </Field>
              <Button clientManaged variant="plain" type="submit" disabled={busy}>
                Record reference for Finance review
              </Button>
            </form>
          ) : (
            <p>No external document reference recorded.</p>
          )}
          {view.documents
            .filter((doc) => doc.state === "voided")
            .map((doc) => (
              <p key={doc.id}>
                Voided: {doc.issuer} · {doc.external_reference} · {doc.reason}
              </p>
            ))}
        </>
      )}
    </section>
  );
}
