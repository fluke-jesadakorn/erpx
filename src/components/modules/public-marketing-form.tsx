"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, TextArea, TextInput } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import { Card, FormLayout } from "@/components/ui/foundation";
export function PublicMarketingForm({
  holdingSlug,
  formId,
}: {
  holdingSlug: string;
  formId: string;
}) {
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [done, setDone] = useState(false),
    key = useRef<string | null>(null);
  async function submit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (busy) return;
    setBusy(true);
    setError("");
    const f = new FormData(e.currentTarget),
      query = new URLSearchParams(window.location.search);
    const body = {
      contactName: f.get("name"),
      contactDetail: f.get("detail"),
      brief: f.get("brief"),
      consentAccepted: f.get("consent") === "on",
      source: (query.get("utm_source") ?? "Direct web form").slice(0, 120),
      campaignId: query.get("campaignId") || undefined,
    };
    key.current ??= crypto.randomUUID();
    try {
      const r = await fetch(
          `/api/public/marketing/${encodeURIComponent(holdingSlug)}/${encodeURIComponent(formId)}`,
          {
            method: "POST",
            headers: { "Content-Type": "application/json", "Idempotency-Key": key.current },
            body: JSON.stringify(body),
          },
        ),
        j = await r.json();
      if (!r.ok) throw new Error(j.error?.message ?? "Submission failed.");
      key.current = null;
      setDone(true);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Try again.");
    } finally {
      setBusy(false);
    }
  }
  if (done)
    return (
      <Card as="section" appearance="standard" className="ui-panel" role="status">
        <h2>Thank you</h2>
        <p>Your message is awaiting review.</p>
      </Card>
    );
  return (
    <FormLayout className="ui-panel ui-form-grid" onSubmit={submit}>
      <Field appearance="standard">
        Name
        <TextInput appearance="standard" name="name" required minLength={2} />
      </Field>
      <Field appearance="standard">
        How can we contact you?
        <TextArea appearance="standard" name="detail" required minLength={3} />
      </Field>
      <Field appearance="standard">
        How can we help?
        <TextArea appearance="standard" name="brief" required minLength={3} />
      </Field>
      <Field appearance="standard" orientation="inline">
        <TextInput appearance="standard" name="consent" type="checkbox" required /> I agree to be contacted about this
        request.
      </Field>
      <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
        {busy ? "Submitting…" : "Send request"}
      </Button>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
    </FormLayout>
  );
}
