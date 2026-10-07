"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { hubResultHref } from "@/lib/hub-result";
import { workFetch } from "@/lib/work-fetch";
import type {
  customerDuplicateDetail,
  customerDuplicateWorkspace,
} from "@/server/modules/customer_authority/public/customer-duplicates";
import { Card, FormLayout } from "@/components/ui/foundation";
export function CustomerDuplicateConsole({
  base,
  data,
  detail,
}: {
  base: string;
  data?: Awaited<ReturnType<typeof customerDuplicateWorkspace>>;
  detail?: Awaited<ReturnType<typeof customerDuplicateDetail>>;
}) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    last = useRef<{ body: string; key: string } | null>(null);
  async function send(input: unknown) {
    const body = JSON.stringify(input);
    if (last.current?.body !== body) last.current = { body, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const r = await workFetch(`/api/v1${base}/customer-duplicates`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": last.current.key },
          body,
        }),
        result = await r.json();
      if (!r.ok) {
        if (r.status < 500) last.current = null;
        throw new Error(result.error?.message ?? "Unable to save duplicate review.");
      }
      last.current = null;
      window.dispatchEvent(new Event("erpx:hub-saved"));
      window.dispatchEvent(new Event("erpx:record-saved"));
      router.push(hubResultHref(window.location.href, base, `${base}/customer-duplicates/${result.data.id}`) ?? `${base}/customer-duplicates/${result.data.id}`, { scroll: false });
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry this command.");
    } finally {
      setBusy(false);
    }
  }
  const evidence = (
    <Field appearance="standard">
      Evidence and reason
      <TextArea appearance="standard" name="reason" minLength={3} maxLength={2000} required />
    </Field>
  );
  return (
    <>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {data?.canCreate && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Request duplicate review</h2>
          <FormLayout
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void send({
                leftCustomerId: f.get("left"),
                rightCustomerId: f.get("right"),
                evidence: f.get("reason"),
              });
            }}
          >
            <Field appearance="standard">
              First customer
              <Select appearance="standard" name="left" required>
                <option value="">Choose customer</option>
                {data.customers.map((c) => (
                  <option value={String(c.id)} key={String(c.id)}>
                    {String(c.name)} · {String(c.code)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Second customer
              <Select appearance="standard" name="right" required>
                <option value="">Choose customer</option>
                {data.customers.map((c) => (
                  <option value={String(c.id)} key={String(c.id)}>
                    {String(c.name)} · {String(c.code)}
                  </option>
                ))}
              </Select>
            </Field>
            {evidence}
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Send for independent review
            </Button>
          </FormLayout>
        </Card>
      )}
      {detail &&
        (Boolean(detail.row.canReview) ||
          Boolean(detail.row.canMerge) ||
          Boolean(detail.row.canReopen)) && (
          <Card as="section" appearance="standard" className="ui-panel">
            <h2>Identity decision</h2>
            {Boolean(detail.row.canReview) && (
              <FormLayout
                className="ui-form-grid"
                onSubmit={(e) => {
                  e.preventDefault();
                  const f = new FormData(e.currentTarget),
                    action = String(f.get("action"));
                  void send({
                    id: detail.row.id,
                    action,
                    expectedVersion: Number(detail.row.version),
                    ...(action === "duplicate" ? { canonicalCustomerId: f.get("canonical") } : {}),
                    reason: f.get("reason"),
                  });
                }}
              >
                <Field appearance="standard">
                  Decision
                  <Select appearance="standard" name="action">
                    <option value="distinct">Keep as distinct customers</option>
                    <option value="duplicate">Confirm duplicate pair</option>
                  </Select>
                </Field>
                <Field appearance="standard">
                  Canonical record
                  <Select appearance="standard" name="canonical">
                    <option value={String(detail.row.left_customer_id)}>
                      {String(detail.row.left_name)}
                    </option>
                    <option value={String(detail.row.right_customer_id)}>
                      {String(detail.row.right_name)}
                    </option>
                  </Select>
                </Field>
                {evidence}
                <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                  Record independent decision
                </Button>
              </FormLayout>
            )}
            {Boolean(detail.row.canMerge) && (
              <FormLayout
                className="ui-form-grid"
                onSubmit={(e) => {
                  e.preventDefault();
                  if (
                    !window.confirm(
                      "Merge safe source records into the canonical customer? Records with immutable owner evidence will remain blocked.",
                    )
                  )
                    return;
                  const f = new FormData(e.currentTarget);
                  void send({
                    id: detail.row.id,
                    action: "merge",
                    expectedVersion: Number(detail.row.version),
                    reason: f.get("reason"),
                  });
                }}
              >
                <p>
                  <strong>Ready to merge</strong> — eligible orders, invoices, payments, refunds,
                  carts, opportunities, expenses, and POS sales will be repointed to the canonical
                  customer. Source records with immutable owner evidence are blocked and must be
                  corrected first.
                </p>
                {evidence}
                <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                  Merge into canonical customer
                </Button>
              </FormLayout>
            )}
            {Boolean(detail.row.canReopen) && (
              <FormLayout
                className="ui-form-grid"
                onSubmit={(e) => {
                  e.preventDefault();
                  const f = new FormData(e.currentTarget);
                  void send({
                    id: detail.row.id,
                    action: "reopen",
                    expectedVersion: Number(detail.row.version),
                    reason: f.get("reason"),
                  });
                }}
              >
                {evidence}
                <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                  Reopen review
                </Button>
              </FormLayout>
            )}
          </Card>
        )}
    </>
  );
}
