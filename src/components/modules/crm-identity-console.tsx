"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import type { crmIdentities } from "@/server/crm-identities";
import { Card, FormLayout } from "@/components/ui/foundation";

export function CRMIdentityConsole({
  base,
  data,
}: {
  base: string;
  data: Awaited<ReturnType<typeof crmIdentities>>;
}) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    last = useRef<{ body: string; key: string } | null>(null);
  async function send(body: Record<string, unknown>) {
    if (busy) return;
    const encoded = JSON.stringify({ ...body, domain: "identity" });
    if (last.current?.body !== encoded) last.current = { body: encoded, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const r = await workFetch(`/api/v1${base}/crm`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": last.current.key },
          body: encoded,
        }),
        j = await r.json();
      if (!r.ok) {
        if (r.status < 500) last.current = null;
        throw new Error(j.error?.message ?? "Unable to save CRM identity.");
      }
      last.current = null;
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry the same action.");
    } finally {
      setBusy(false);
    }
  }
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>Accounts &amp; contacts</h2>
      <p>
        Prospects can be recorded before Finance creates a customer. A separate reviewer must
        confirm a Finance customer or PMS guest link; names and email addresses never link people
        automatically.
      </p>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {data.canCreate && (
        <>
          <FormLayout
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void send({ action: "create-account", kind: f.get("kind"), name: f.get("name") });
            }}
          >
            <h3>New account</h3>
            <Field appearance="standard">
              Type
              <Select appearance="standard" name="kind">
                <option value="business">Business</option>
                <option value="individual">Individual</option>
              </Select>
            </Field>
            <Field appearance="standard">
              Name
              <TextInput appearance="standard" name="name" required minLength={2} />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Create account
            </Button>
          </FormLayout>
          <FormLayout
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void send({
                action: "create-contact",
                accountId: f.get("accountId"),
                name: f.get("name"),
                detail: f.get("detail"),
              });
            }}
          >
            <h3>New contact</h3>
            <Field appearance="standard">
              Account
              <Select appearance="standard" name="accountId" required>
                <option value="">Select account</option>
                {data.accounts.map((a) => (
                  <option key={String(a.id)} value={String(a.id)}>
                    {String(a.name)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Name
              <TextInput appearance="standard" name="name" required minLength={2} />
            </Field>
            <Field appearance="standard">
              Contact detail
              <TextArea appearance="standard" name="detail" required minLength={3} />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Create contact
            </Button>
          </FormLayout>
        </>
      )}
      {data.canReview && (
        <FormLayout
          className="ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget),
              account = data.accounts.find((a) => a.id === f.get("accountId"));
            if (account)
              void send({
                action: "link-finance",
                accountId: account.id,
                customerId: f.get("customerId"),
                expectedVersion: Number(account.version),
                evidence: f.get("evidence"),
              });
          }}
        >
          <h3>Review Finance customer link</h3>
          <Field appearance="standard">
            Prospect account
            <Select appearance="standard" name="accountId" required>
              <option value="">Select account</option>
              {data.accounts
                .filter((a) => !a.finance_party_id)
                .map((a) => (
                  <option key={String(a.id)} value={String(a.id)}>
                    {String(a.name)}
                  </option>
                ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Verified Finance customer
            <Select appearance="standard" name="customerId" required>
              <option value="">Select customer</option>
              {data.customers.map((c) => (
                <option key={String(c.id)} value={String(c.id)}>
                  {String(c.name)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Review evidence
            <TextArea appearance="standard" name="evidence" required minLength={3} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            Confirm link
          </Button>
        </FormLayout>
      )}
      <h3>Recorded accounts</h3>
      {data.accounts.map((a) => (
        <p key={String(a.id)}>
          {String(a.name)} · {String(a.kind)} ·{" "}
          {a.finance_party_id ? `Finance: ${String(a.finance_customer_name)}` : "Prospect"}
        </p>
      ))}
      {!data.accounts.length && <p>No accounts recorded.</p>}
    </Card>
  );
}
