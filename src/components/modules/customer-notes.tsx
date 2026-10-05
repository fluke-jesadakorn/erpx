"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextArea } from "@/components/ui/foundation";

import { TimeText } from "@/components/time/time-preference-context";
import { PreferredDateTimeField } from "@/components/time/preferred-date-time-field";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import type { CustomerNotesView } from "@/server/customer-notes";
import { Card, FormGrid } from "@/components/ui/foundation";
export function CustomerNotes({ base, data, mode = "all" }: { base: string; data: CustomerNotesView; mode?: "all" | "create" | "history" }) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    last = useRef<{ fingerprint: string; key: string } | null>(null);
  async function send(input: unknown, form: HTMLFormElement) {
    if (busy) return;
    setBusy(true);
    setError("");
    const fingerprint = JSON.stringify(input);
    if (last.current?.fingerprint !== fingerprint)
      last.current = { fingerprint, key: crypto.randomUUID() };
    try {
      const r = await workFetch(`/api/v1${base}/customer-notes`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": last.current.key },
          body: JSON.stringify(input),
        }),
        body = await r.json();
      if (!r.ok) {
        if (r.status < 500) last.current = null;
        throw new Error(body.error?.message ?? "Unable to save contact note.");
      }
      last.current = null;
      form.reset();
      window.dispatchEvent(new Event("erpx:hub-saved"));
      window.dispatchEvent(new Event("erpx:record-saved"));
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry this command.");
    } finally {
      setBusy(false);
    }
  }
  const source = data.order
    ? { orderId: data.order.id, expectedVersion: Number(data.order.version) }
    : { customerId: data.customer.id, expectedVersion: Number(data.customer.version) };
  return (
    <>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {mode !== "history" && data.canCreate && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Record contact</h2>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              const form = e.currentTarget,
                f = new FormData(form);
              void send(
                {
                  ...source,
                  channel: f.get("channel"),
                  occurredAt: new Date(String(f.get("occurred"))).getTime(),
                  body: f.get("body"),
                },
                form,
              );
            }}
          >
            <FormGrid as="div" className="ui-form-grid">
              <Field appearance="standard">
                Channel
                <Select appearance="standard" name="channel">
                  <option value="phone">Phone</option>
                  <option value="email">Email</option>
                  <option value="meeting">Meeting</option>
                  <option value="other">Other</option>
                </Select>
              </Field>
              <PreferredDateTimeField appearance="standard" name="occurred" label="Contact time" step={1} required />
              <Field appearance="standard">
                Contact summary
                <TextArea appearance="standard" name="body" required minLength={3} maxLength={4000} />
              </Field>
            </FormGrid>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Save internal note
            </Button>
          </form>
        </Card>
      )}
      {mode !== "create" && data.notes.map((n) => (
        <Card as="section" appearance="standard" className="ui-panel" key={`${n.id}:${n.version}`}>
          <h2>
            {String(n.channel)} · <TimeText appearance="standard" value={Number(n.occurred_at)} /> · {String(n.status)}
          </h2>
          <p style={{ whiteSpace: "pre-wrap" }}>{String(n.body)}</p>
          {n.canChange && (
            <FormDisclosure appearance="standard">
              <summary>Correct or archive</summary>
              <form
                onSubmit={(e) => {
                  e.preventDefault();
                  const form = e.currentTarget,
                    f = new FormData(form);
                  void send(
                    {
                      id: n.id,
                      expectedVersion: Number(n.version),
                      action: f.get("action"),
                      body: f.get("body"),
                      reason: f.get("reason"),
                    },
                    form,
                  );
                }}
              >
                <FormGrid as="div" className="ui-form-grid">
                  <Field appearance="standard">
                    Action
                    <Select appearance="standard" name="action">
                      <option value="correct">Correct note</option>
                      <option value="archive">Archive note</option>
                    </Select>
                  </Field>
                  <Field appearance="standard">
                    Corrected summary
                    <TextArea appearance="standard"
                      name="body"
                      defaultValue={String(n.body)}
                      required
                      minLength={3}
                      maxLength={4000}
                    />
                  </Field>
                  <Field appearance="standard">
                    Reason
                    <TextArea appearance="standard" name="reason" required minLength={3} maxLength={4000} />
                  </Field>
                </FormGrid>
                <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                  Save with history
                </Button>
              </form>
            </FormDisclosure>
          )}
          <details>
            <summary>Change history</summary>
            {data.history
              .filter((h) => h.note_id === n.id)
              .map((h) => (
                <div key={String(h.id)}>
                  <p>
                    Version {String(h.version)} · {String(h.action)} · {String(h.actor_id)} ·{" "}
                    {String(h.reason)}
                  </p>
                  <p style={{ whiteSpace: "pre-wrap" }}>{String(h.body)}</p>
                </div>
              ))}
          </details>
        </Card>
      ))}
      {!data.notes.length && <p>No contact notes for this customer or order.</p>}
    </>
  );
}
