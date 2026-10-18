"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea } from "@/components/ui/foundation";

import { PreferredDateTimeField } from "@/components/people_payroll/time/preferred-date-time-field";
import { useRef, useState } from "react";
import { useRouter } from "@/lib/shared/destination-router";
import { workFetch } from "@/lib/work_projection/work-fetch";
import type { supportSessionDetail, supportSessionWorkspace } from "@/server/modules/support/public/support-sessions";
import { FormLayout } from "@/components/ui/foundation";

export function SupportSessionConsole({
  base,
  data,
  detail,
}: {
  base: string;
  data?: Awaited<ReturnType<typeof supportSessionWorkspace>>;
  detail?: Awaited<ReturnType<typeof supportSessionDetail>>;
}) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    last = useRef<{ key: string; body: string } | null>(null);
  async function send(input: unknown) {
    if (busy) return;
    setBusy(true);
    setError("");
    const body = JSON.stringify(input);
    if (last.current?.body !== body) last.current = { key: crypto.randomUUID(), body };
    try {
      const response = await workFetch(`/api/v1${base}/support/sessions`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": last.current.key },
          body,
        }),
        result = await response.json();
      if (!response.ok) {
        if (response.status < 500) last.current = null;
        throw new Error(result.error?.message ?? "Unable to record support access.");
      }
      last.current = null;
      router.push(canonicalUiHref(`${base}/support/sessions/${result.data.id}`));
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry this action.");
    } finally {
      setBusy(false);
    }
  }
  const r = detail?.row;
  function form(action: string, title: string, extra?: React.ReactNode) {
    return (
      <form
        className="ui-panel ui-form-grid"
        onSubmit={(e) => {
          e.preventDefault();
          const values = Object.fromEntries(new FormData(e.currentTarget));
          void send({ ...values, id: r!.id, action, expectedVersion: Number(r!.version) });
        }}
      >
        <h2>{title}</h2>
        {extra}
        <Field appearance="standard">
          Reason and evidence
          <TextArea appearance="standard" name="reason" required minLength={3} maxLength={2000} />
        </Field>
        <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
          {title}
        </Button>
      </form>
    );
  }
  return (
    <>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {data && data.tickets.length > 0 && (
        <FormLayout
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget),
              ticket = data.tickets.find((t) => t.id === f.get("ticketId"));
            void send({
              ticketId: f.get("ticketId"),
              ticketVersion: Number(ticket?.version),
              requestedFor: data.actorId,
              expiresAt: new Date(String(f.get("expiresAt"))).getTime(),
              reason: f.get("reason"),
            });
          }}
        >
          <h2>Request temporary customer context</h2>
          <Field appearance="standard">
            Assigned case
            <Select appearance="standard" name="ticketId" required>
              <option value="">Choose an active assigned case</option>
              {data.tickets.map((t) => (
                <option key={String(t.id)} value={String(t.id)}>
                  {String(t.title)}
                </option>
              ))}
            </Select>
          </Field>
          <PreferredDateTimeField appearance="standard" name="expiresAt" label="Expires at (within 8 hours)" required />
          <Field appearance="standard">
            Business reason
            <TextArea appearance="standard" name="reason" required minLength={3} maxLength={2000} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            Request session
          </Button>
        </FormLayout>
      )}
      {r?.canApprove && form("approve", "Approve temporary session")}
      {r?.canReview && form("reject", "Reject temporary session")}
      {r?.canIssueCapability &&
        form(
          "issue-capability",
          "Issue capability",
          <Field appearance="standard">
            Capability
            <Select appearance="standard" name="code">
              <option value="customer_order_context">Customer order context</option>
              <option value="ticket_evidence_metadata">Ticket evidence metadata</option>
            </Select>
          </Field>,
        )}
      {r?.canManage && form("revoke", "Revoke whole session")}
      {r?.canManage &&
        detail!.capabilities
          .filter((c) => c.status === "active")
          .map((c) => (
            <FormLayout
              key={String(c.id)}
              className="ui-panel ui-form-grid"
              onSubmit={(e) => {
                e.preventDefault();
                const f = new FormData(e.currentTarget);
                void send({
                  id: r.id,
                  action: "revoke-capability",
                  expectedVersion: Number(r.version),
                  capabilityId: c.id,
                  expectedCapabilityVersion: Number(c.version),
                  reason: f.get("reason"),
                });
              }}
            >
              <h2>Revoke {String(c.code)}</h2>
              <Field appearance="standard">
                Reason
                <TextArea appearance="standard" name="reason" required minLength={3} />
              </Field>
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                Revoke capability
              </Button>
            </FormLayout>
          ))}
    </>
  );
}
