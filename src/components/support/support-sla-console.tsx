"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { TimeText } from "@/components/people_payroll/time/time-preference-context";
import { useRef, useState } from "react";
import { useRouter } from "@/lib/shared/destination-router";
import { workFetch } from "@/lib/work_projection/work-fetch";
import type { supportSlaWorkspace } from "@/server/modules/support/public/support-sla";
import { Card, FormLayout } from "@/components/ui/foundation";
type Policy = { id: string; name: string; version: number };
export function SupportSlaConsole({
  base,
  data,
  ticket,
  policies = [],
}: {
  base: string;
  data?: Awaited<ReturnType<typeof supportSlaWorkspace>>;
  ticket?: { id: string; version: number };
  policies?: Policy[];
}) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    last = useRef<{ fingerprint: string; key: string } | null>(null);
  async function send(input: unknown) {
    if (busy) return;
    setBusy(true);
    setError("");
    const fingerprint = JSON.stringify(input);
    if (last.current?.fingerprint !== fingerprint)
      last.current = { fingerprint, key: crypto.randomUUID() };
    try {
      const response = await workFetch(`/api/v1${base}/support/sla`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": last.current.key },
          body: JSON.stringify(input),
        }),
        result = await response.json();
      if (!response.ok) {
        if (response.status < 500) last.current = null;
        throw new Error(result.error?.message ?? "Unable to save SLA action.");
      }
      last.current = null;
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry the same action.");
    } finally {
      setBusy(false);
    }
  }
  const reason = (
      <Field appearance="standard">
        Evidence and reason
        <TextArea appearance="standard" name="reason" minLength={3} maxLength={2000} required />
      </Field>
    ),
    minutes = (s: FormDataEntryValue | null) => {
      const [h, m] = String(s).split(":").map(Number);
      return h * 60 + m;
    };
  return (
    <>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {data?.canCreate && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Draft a resolution-time policy</h2>
          <FormLayout
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void send({
                action: "create-policy",
                name: f.get("name"),
                reason: f.get("reason"),
                policy: {
                  targetMinutes: Number(f.get("target")),
                  pauseWhileWaiting: f.has("pause"),
                  calendar: {
                    timezone: f.get("timezone"),
                    weekdays: f.getAll("weekday").map(Number),
                    opensAtMinute: minutes(f.get("opens")),
                    closesAtMinute: minutes(f.get("closes")),
                    holidays: String(f.get("holidays")).split(/\s+/).filter(Boolean),
                  },
                },
              });
            }}
          >
            <Field appearance="standard">
              Policy name
              <TextInput appearance="standard" name="name" minLength={3} maxLength={160} required />
            </Field>
            <Field appearance="standard">
              Timezone
              <TextInput appearance="standard" name="timezone" placeholder="Asia/Bangkok" required />
            </Field>
            <fieldset>
              <legend>Working days</legend>
              {["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"].map(
                (day, n) => (
                  <Field appearance="standard" orientation="inline" key={day}>
                    <TextInput appearance="standard" type="checkbox" name="weekday" value={n} />
                    {day}
                  </Field>
                ),
              )}
            </fieldset>
            <Field appearance="standard">
              Opens at
              <TextInput appearance="standard" name="opens" type="time" required />
            </Field>
            <Field appearance="standard">
              Closes at
              <TextInput appearance="standard" name="closes" type="time" required />
            </Field>
            <Field appearance="standard">
              Resolution target (working minutes)
              <TextInput appearance="standard" name="target" type="number" min={1} max={28800} required />
            </Field>
            <Field appearance="standard" orientation="inline">
              <TextInput appearance="standard" name="pause" type="checkbox" />
              Pause while waiting for customer information
            </Field>
            <Field appearance="standard">
              Holidays (YYYY-MM-DD, one per line)
              <TextArea appearance="standard" name="holidays" />
            </Field>
            {reason}
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Create policy for independent review
            </Button>
          </FormLayout>
        </Card>
      )}
      {data?.alerts.map((r) => (
        <Card as="section" appearance="standard" className="ui-panel" key={r.id}>
          <h2>Target escalation · {r.title}</h2>
          <p>
            Breached <TimeText appearance="standard" value={Number(r.breached_at)} /> ·{" "}
            {r.acknowledged_by ? "Follow-up responsibility recorded" : "Awaiting Support manager"}
          </p>
          <a href={canonicalUiHref(`${base}/support/${r.ticket_id}`)}>Open source case</a>
          {r.reason && <p>{r.reason}</p>}
          {!r.acknowledged_by && !Number(r.recipients) && (
            <InlineNotice tone="danger" role="alert">
              No eligible Support manager. An administrator must assign authorized coverage.
            </InlineNotice>
          )}
          {!r.acknowledged_by && Number(r.can_ack) > 0 && (
            <FormLayout
              className="ui-form-grid"
              onSubmit={(e) => {
                e.preventDefault();
                const f = new FormData(e.currentTarget);
                void send({
                  id: r.id,
                  action: "acknowledge",
                  expectedVersion: Number(r.version),
                  reason: f.get("reason"),
                });
              }}
            >
              <p>
                Record your follow-up plan. This keeps the breach history and does not resolve the
                case.
              </p>
              {reason}
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                Take responsibility for follow-up
              </Button>
            </FormLayout>
          )}
        </Card>
      ))}
      {data?.policies.map((p) => (
        <Card as="section" appearance="standard" className="ui-panel" key={`${p.id}:${p.version}`}>
          <h2>
            {String(p.name)} · {String(p.status)}
          </h2>
          <p>
            {p.policy.targetMinutes} working minutes · {p.policy.calendar.timezone} · Working days{" "}
            {p.policy.calendar.weekdays
              .map(
                (n) =>
                  ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"][n],
              )
              .join(", ")}{" "}
            ·{" "}
            {p.policy.pauseWhileWaiting
              ? "Waiting for customer pauses the clock"
              : "Clock continues while waiting for customer"}
          </p>
          <p>
            Hours {String(Math.floor(p.policy.calendar.opensAtMinute / 60)).padStart(2, "0")}:
            {String(p.policy.calendar.opensAtMinute % 60).padStart(2, "0")}–
            {String(Math.floor(p.policy.calendar.closesAtMinute / 60)).padStart(2, "0")}:
            {String(p.policy.calendar.closesAtMinute % 60).padStart(2, "0")} · Holidays:{" "}
            {p.policy.calendar.holidays.join(", ") || "None recorded"}
          </p>
          {data.canManage && p.status !== "retired" && (
            <FormLayout
              className="ui-form-grid"
              onSubmit={(e) => {
                e.preventDefault();
                const f = new FormData(e.currentTarget);
                void send({
                  id: p.id,
                  action: f.get("action"),
                  expectedVersion: Number(p.version),
                  reason: f.get("reason"),
                });
              }}
            >
              <Field appearance="standard">
                Policy decision
                <Select appearance="standard" name="action">
                  {p.canApprove && <option value="approve">Approve policy</option>}
                  <option value="retire">Retire for future cases</option>
                </Select>
              </Field>
              {reason}
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                Record policy decision
              </Button>
            </FormLayout>
          )}
        </Card>
      ))}
      {ticket && (
        <FormLayout
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget),
              policy = policies.find((p) => p.id === f.get("policy"));
            if (policy)
              void send({
                id: ticket.id,
                action: "attach",
                expectedVersion: ticket.version,
                policyId: policy.id,
                policyVersion: policy.version,
                reason: f.get("reason"),
              });
          }}
        >
          <h2>Attach an approved resolution-time target</h2>
          <p>
            The clock starts when attached. Its working calendar and target remain unchanged if the
            policy is later retired.
          </p>
          <Field appearance="standard">
            Approved policy
            <Select appearance="standard" name="policy" required>
              <option value="">Choose policy</option>
              {policies.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </Select>
          </Field>
          {reason}
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !policies.length}>
            Start working-time target
          </Button>
          {!policies.length && (
            <p>
              A Company Support manager must prepare a policy and another manager must approve it
              first.
            </p>
          )}
        </FormLayout>
      )}
    </>
  );
}
