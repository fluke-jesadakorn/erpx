"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work_projection/work-fetch";
import type { crmActivityWorkspace } from "@/server/modules/crm/public/crm-activities";
import { Card, FormLayout } from "@/components/ui/foundation";
type Data = Awaited<ReturnType<typeof crmActivityWorkspace>>;
export function CRMActivityConsole({ base, data }: { base: string; data: Data }) {
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
      const response = await workFetch(`/api/v1${base}/crm/activities`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": last.current.key },
          body: JSON.stringify(input),
        }),
        result = await response.json();
      if (!response.ok) {
        if (response.status < 500) last.current = null;
        throw new Error(result.error?.message ?? "Unable to save CRM activity.");
      }
      last.current = null;
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry the same command.");
    } finally {
      setBusy(false);
    }
  }
  const reason = (name = "reason") => (
    <TextArea appearance="standard" name={name} minLength={3} maxLength={2000} required />
  );
  return (
    <Card as="section" appearance="standard" className="ui-panel" id="crm-activities">
      <h2>Lead activities</h2>
      <p>
        Schedule the next customer action, assign its owner and record the outcome here. Activities
        remain internal CRM evidence.
      </p>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {data.canCreate && (
        <FormLayout
          className="ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            void send({
              leadId: data.lead.id,
              expectedVersion: data.lead.version,
              activityType: f.get("activityType"),
              subject: f.get("subject"),
              details: f.get("details"),
              ownerId: f.get("ownerId"),
              dueDate: f.get("dueDate"),
            });
          }}
        >
          <Field appearance="standard">
            Activity type
            <Select appearance="standard" name="activityType">
              <option value="call">Call</option>
              <option value="email">Email</option>
              <option value="meeting">Meeting</option>
              <option value="task">Task</option>
            </Select>
          </Field>
          <Field appearance="standard">
            Subject
            <TextInput appearance="standard" name="subject" minLength={3} maxLength={200} required />
          </Field>
          <label>Next action details{reason("details")}</label>
          <Field appearance="standard">
            Responsible salesperson
            <Select appearance="standard" name="ownerId" required defaultValue="">
              <option value="">Choose an eligible salesperson</option>
              {data.recipients.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Due date
            <TextInput appearance="standard" name="dueDate" type="date" required />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !data.recipients.length}>
            Schedule activity
          </Button>
          {!data.recipients.length && (
            <p>
              No eligible salesperson is available. Ask a sales manager to restore role coverage.
            </p>
          )}
        </FormLayout>
      )}
      {data.items.map((item) => (
        <Card as="article" appearance="standard" className="ui-panel" key={`${item.id}:${item.version}`}>
          <h3>
            {item.activity_type} · {item.subject}
          </h3>
          <p>
            {item.status} · Owner {String(item.owner_name ?? "Unavailable")} · Due {item.due_date}
          </p>
          <p style={{ whiteSpace: "pre-wrap" }}>{item.details}</p>
          {item.outcome && <p style={{ whiteSpace: "pre-wrap" }}>Outcome: {item.outcome}</p>}
          {(item.canComplete || item.canManage) && (
            <FormDisclosure appearance="standard">
              <summary>Record activity decision</summary>
              <FormLayout
                className="ui-form-grid"
                onSubmit={(e) => {
                  e.preventDefault();
                  const f = new FormData(e.currentTarget),
                    action = String(f.get("action"));
                  void send({
                    id: item.id,
                    action,
                    expectedVersion: Number(item.version),
                    ownerId: f.get("ownerId"),
                    dueDate: f.get("dueDate"),
                    reason: f.get("reason"),
                  });
                }}
              >
                <Field appearance="standard">
                  Action
                  <Select appearance="standard" name="action">
                    {item.canComplete && (
                      <>
                        <option value="complete">Complete activity</option>
                        <option value="cancel">Cancel activity</option>
                      </>
                    )}
                    {item.canManage && <option value="reassign">Reassign activity</option>}
                  </Select>
                </Field>
                {item.canManage && (
                  <>
                    <Field appearance="standard">
                      Responsible salesperson
                      <Select appearance="standard" name="ownerId" defaultValue={String(item.owner_id)}>
                        {data.recipients.map((p) => (
                          <option key={p.id} value={p.id}>
                            {p.name}
                          </option>
                        ))}
                      </Select>
                    </Field>
                    <Field appearance="standard">
                      Due date
                      <TextInput appearance="standard" name="dueDate" type="date" defaultValue={String(item.due_date)} />
                    </Field>
                  </>
                )}
                {item.canComplete && <label>Outcome and reason{reason()}</label>}
                {item.canManage && !item.canComplete && <label>Management reason{reason()}</label>}
                <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                  Save activity decision
                </Button>
              </FormLayout>
            </FormDisclosure>
          )}
          <details>
            <summary>Activity history</summary>
            {data.history
              .filter((h) => h.activity_id === item.id)
              .map((h) => (
                <div key={String(h.id)}>
                  <p>
                    Version {String(h.version)} · {String(h.action)} · {String(h.actor_id)} ·{" "}
                    {String(h.note)}
                  </p>
                </div>
              ))}
          </details>
        </Card>
      ))}
      {!data.items.length && <p>No CRM activities yet.</p>}
    </Card>
  );
}
