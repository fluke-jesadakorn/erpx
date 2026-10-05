"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { TimeText } from "@/components/time/time-preference-context";
import { PreferredDateTimeField } from "@/components/time/preferred-date-time-field";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { hubResultHref } from "@/lib/hub-result";
import { workFetch } from "@/lib/work-fetch";
import type { customerAccessHistory, customerAccessWorkspace } from "@/server/customer-access";
import { Card, FormLayout } from "@/components/ui/foundation";
export function CustomerAccessConsole({
  base,
  data,
  detail,
  selectedOrderId,
}: {
  base: string;
  data?: Awaited<ReturnType<typeof customerAccessWorkspace>>;
  detail?: Awaited<ReturnType<typeof customerAccessHistory>>;
  selectedOrderId?: string;
}) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    last = useRef<{ body: string; key: string } | null>(null);
  async function send(input: unknown) {
    if (busy) return;
    const body = JSON.stringify(input);
    if (last.current?.body !== body) last.current = { body, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const r = await workFetch(`/api/v1${base}/customer-access`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": last.current.key },
          body,
        }),
        result = await r.json();
      if (!r.ok) {
        if (r.status < 500) last.current = null;
        throw new Error(result.error?.message ?? "Unable to save customer access.");
      }
      last.current = null;
      window.dispatchEvent(new Event("erpx:hub-saved"));
      window.dispatchEvent(new Event("erpx:record-saved"));
      router.push(hubResultHref(window.location.href, base, `${base}/customer-access/${result.data.id}`) ?? `${base}/customer-access/${result.data.id}`, { scroll: false });
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry this command.");
    } finally {
      setBusy(false);
    }
  }
  const reason = (
    <Field appearance="standard">
      Authority evidence and reason
      <TextArea appearance="standard" name="reason" minLength={3} maxLength={2000} required />
    </Field>
  );
  return (
    <>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {data && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Request source-bound customer access</h2>
          <FormLayout
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget),
                source = data.sources.find((s) => s.id === f.get("orderId"));
              if (source)
                void send({
                  orderId: source.id,
                  sourceVersion: Number(source.version),
                  email: f.get("email"),
                  expiresAt: new Date(String(f.get("expiresAt"))).getTime(),
                  reason: f.get("reason"),
                });
            }}
          >
            <Field appearance="standard">
              Approved customer document
              <Select appearance="standard" name="orderId" defaultValue={selectedOrderId ?? ""} required>
                <option value="">Choose source</option>
                {data.sources.map((s) => (
                  <option key={String(s.id)} value={String(s.id)}>
                    {String(s.name)} · {String(s.number)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Verified customer email
              <TextInput appearance="standard" type="email" name="email" autoComplete="off" required />
            </Field>
            <PreferredDateTimeField appearance="standard" name="expiresAt" label="Access expires" required />
            {reason}
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Send for independent review
            </Button>
          </FormLayout>
        </Card>
      )}
      {detail && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Access decision</h2>
          <p>
            {String(detail.binding.email)} · {String(detail.binding.status)} · expires{" "}
            <TimeText appearance="standard" value={Number(detail.binding.expires_at)} />
          </p>
          {Boolean(detail.binding.canReview) && (
            <FormLayout
              className="ui-form-grid"
              onSubmit={(e) => {
                e.preventDefault();
                const f = new FormData(e.currentTarget);
                void send({
                  id: detail.binding.id,
                  action: f.get("action"),
                  expectedVersion: Number(detail.binding.version),
                  reason: f.get("reason"),
                });
              }}
            >
              <Field appearance="standard">
                Decision
                <Select appearance="standard" name="action">
                  <option value="approve">Approve access</option>
                  <option value="reject">Reject request</option>
                </Select>
              </Field>
              {reason}
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                Record independent decision
              </Button>
            </FormLayout>
          )}
          {Boolean(detail.binding.canRevoke) &&
            ["pending", "approved"].includes(String(detail.binding.status)) && (
              <FormLayout
                className="ui-form-grid"
                onSubmit={(e) => {
                  e.preventDefault();
                  const f = new FormData(e.currentTarget);
                  void send({
                    id: detail.binding.id,
                    action: "revoke",
                    expectedVersion: Number(detail.binding.version),
                    reason: f.get("reason"),
                  });
                }}
              >
                {reason}
                <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                  Revoke access
                </Button>
              </FormLayout>
            )}
        </Card>
      )}
    </>
  );
}
