"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useTimePreference } from "@/components/time/time-preference-context";
import { formatPreferredDateTime } from "@/lib/ui-format";
import { useRef, useState, type ReactNode } from "react";
import { useRouter } from "next/navigation";
import { hubResultHref } from "@/lib/hub-result";
import { workFetch } from "@/lib/work-fetch";
import type { supportDetail, supportWorkspace } from "@/server/modules/support/public/support";
import { FormLayout } from "@/components/ui/foundation";
function SupportDecisionForm({
  label,
  children,
  onSubmit,
}: {
  label: string;
  children: ReactNode;
  onSubmit: (data: FormData) => void;
}) {
  return (
    <FormLayout
      className="ui-panel ui-form-grid"
      onSubmit={(event) => {
        event.preventDefault();
        onSubmit(new FormData(event.currentTarget));
      }}
    >
      <h2>{label}</h2>
      {children}
    </FormLayout>
  );
}

export function SupportConsole({
  base,
  data,
  detail,
  source,
  typedSource,
  recipients = [],
}: {
  base: string;
  data?: Awaited<ReturnType<typeof supportWorkspace>>;
  detail?: Awaited<ReturnType<typeof supportDetail>>;
  source?: { id: string; version: number };
  typedSource?:{kind:"pos_sale"|"pms_record"|"asset";id:string;version:number;label:string};
  recipients?: { id: string; name: string }[];
}) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    last = useRef<{ fingerprint: string; key: string } | null>(null),
    r = detail?.row;
  const { preference, systemZone, ready } = useTimePreference();
  const time = (value: number) =>
    ready ? formatPreferredDateTime(value, preference, systemZone) : "…";
  async function send(input: unknown) {
    if (busy) return;
    setBusy(true);
    setError("");
    const fingerprint = JSON.stringify(input);
    if (last.current?.fingerprint !== fingerprint)
      last.current = { fingerprint, key: crypto.randomUUID() };
    try {
      const response = await workFetch(`/api/v1${base}/support`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": last.current.key },
          body: JSON.stringify(input),
        }),
        result = await response.json();
      if (!response.ok) {
        if (response.status < 500) last.current = null;
        throw new Error(result.error?.message ?? "Unable to save support action.");
      }
      last.current = null;
      window.dispatchEvent(new Event("erpx:hub-saved"));
      window.dispatchEvent(new Event("erpx:record-saved"));
      router.push(hubResultHref(window.location.href, base, `${base}/support/${result.data.id}`) ?? `${base}/support/${result.data.id}`, { scroll: false });
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry this command.");
    } finally {
      setBusy(false);
    }
  }
  const reason = (
      <Field appearance="standard">
        Evidence and reason
        <TextArea appearance="standard" name="reason" minLength={3} maxLength={4000} required />
      </Field>
    ),
    button = (label: string) => (
      <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
        {label}
      </Button>
    ),
    payload = (f: FormData, action: string) => ({
      id: r!.id,
      action,
      expectedVersion: Number(r!.version),
      reason: f.get("reason"),
    }),
    owner = (items: typeof recipients) => (
      <Field appearance="standard">
        Eligible support colleague
        <Select appearance="standard" name="owner" required defaultValue={String(r?.owner_id ?? "")}>
          <option value="">Choose colleague</option>
          {items.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </Select>
      </Field>
    );

  return (
    <>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {data?.canCreate && (
        <FormLayout
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            void send({ kind: typedSource?"source":"general",...(typedSource?{source:typedSource}:{}), title: f.get("title"), issue: f.get("issue") });
          }}
        >
          <h2>Report an issue</h2>
          <p>Describe what happened. Support triage will choose the owner and follow-up date.</p>
          {typedSource&&<p>Source: {typedSource.label} · version {typedSource.version}</p>}
          <Field appearance="standard">
            Issue title
            <TextInput appearance="standard" name="title" minLength={3} maxLength={160} required />
          </Field>
          <Field appearance="standard">
            What happened?
            <TextArea appearance="standard" name="issue" minLength={3} maxLength={4000} required />
          </Field>
          {button("Send to Support triage")}
        </FormLayout>
      )}
      {data?.canCreateOrder && source && (
        <FormLayout
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            void send({
              orderId: source.id,
              sourceVersion: source.version,
              title: f.get("title"),
              issue: f.get("issue"),
              ownerId: f.get("owner"),
              dueDate: f.get("due"),
            });
          }}
        >
          <h2>Record customer issue from order</h2>
          <Field appearance="standard">
            Case title
            <TextInput appearance="standard" name="title" minLength={3} maxLength={160} required />
          </Field>
          <Field appearance="standard">
            Issue reported by customer
            <TextArea appearance="standard" name="issue" minLength={3} maxLength={4000} required />
          </Field>
          {owner(recipients)}
          <Field appearance="standard">
            Next follow-up due
            <TextInput appearance="standard" name="due" type="date" required />
          </Field>
          {button("Create linked support case")}
        </FormLayout>
      )}
      {r&&detail?.canReport&&!['closed','cancelled'].includes(String(r.status))&&<>
        {r.status==='triage'&&<SupportDecisionForm label="Edit my report" onSubmit={f=>void send({...payload(f,"edit-report"),title:f.get("title"),issue:f.get("issue")})}>
          <Field appearance="standard">Issue title<TextInput appearance="standard" name="title" defaultValue={String(r.title)} minLength={3} required/></Field>
          <Field appearance="standard">What happened?<TextArea appearance="standard" name="issue" defaultValue={String(r.issue)} minLength={3} required/></Field>
          {reason}{button("Save report changes")}
        </SupportDecisionForm>}
        <SupportDecisionForm label="Add information" onSubmit={f=>void send(payload(f,"add-information"))}>{reason}{button("Send additional information")}</SupportDecisionForm>
        <SupportDecisionForm label="Withdraw my report" onSubmit={f=>void send(payload(f,"withdraw-report"))}>{reason}{button("Withdraw report")}</SupportDecisionForm>
      </>}
      {r && detail?.canManage && r.status === "triage" && (
        <FormLayout
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            void send({ ...payload(f, "triage"), ownerId: f.get("owner"), dueDate: f.get("due") });
          }}
        >
          <h2>Triage this issue</h2>
          <p>
            Choose an eligible colleague and a follow-up date. The reporter&apos;s original details
            stay in the case history.
          </p>
          {owner(detail.recipients)}
          <Field appearance="standard">
            Follow-up due
            <TextInput appearance="standard" name="due" type="date" required />
          </Field>
          {reason}
          {button("Assign issue")}
        </FormLayout>
      )}
      {r && detail?.canWork && r.status === "open" && (
        <SupportDecisionForm
          label={"Accept case"}
          onSubmit={(data) => {
            void send(payload(data, "start"));
          }}
        >
          {reason}
          {button("Accept case")}
        </SupportDecisionForm>
      )}
      {r && detail?.canWork && r.status === "in_progress" && (
        <>
          {
            <SupportDecisionForm
              label={
                r.source_kind !== "sales_order"
                  ? "Request more information"
                  : "Request customer information"
              }
              onSubmit={(data) => {
                void send(payload(data, "wait-customer"));
              }}
            >
              {reason}
              {button(
                r.source_kind !== "sales_order"
                  ? "Request more information"
                  : "Request customer information",
              )}
            </SupportDecisionForm>
          }
          {
            <SupportDecisionForm
              label={"Record proposed resolution"}
              onSubmit={(data) => {
                void send(payload(data, "resolve"));
              }}
            >
              {reason}
              {button("Record proposed resolution")}
            </SupportDecisionForm>
          }
        </>
      )}
      {r && detail?.canWork && r.status === "waiting_customer" && (
        <SupportDecisionForm
          label={
            r.source_kind !== "sales_order"
              ? "Resume after new information"
              : "Resume after customer reply"
          }
          onSubmit={(data) => {
            void send(payload(data, "resume"));
          }}
        >
          {reason}
          {button(
            r.source_kind !== "sales_order"
              ? "Resume after new information"
              : "Resume after customer reply",
          )}
        </SupportDecisionForm>
      )}
      {r && detail?.canManage && !["triage", "closed", "cancelled"].includes(String(r.status)) && (
        <FormLayout
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            void send({
              ...payload(f, "reassign"),
              ownerId: f.get("owner"),
              dueDate: f.get("due"),
            });
          }}
        >
          <h2>Change case responsibility</h2>
          {owner(detail.recipients)}
          <Field appearance="standard">
            Next follow-up due
            <TextInput appearance="standard" name="due" type="date" defaultValue={String(r.due_date)} required />
          </Field>
          {reason}
          {button("Reassign case")}
        </FormLayout>
      )}
      {r && detail?.canManage && r.status === "resolved" && (
        <>
          {
            <SupportDecisionForm
              label={"Return for further work"}
              onSubmit={(data) => {
                void send(payload(data, "reject-resolution"));
              }}
            >
              {reason}
              {button("Return for further work")}
            </SupportDecisionForm>
          }
          {r.source_kind !== "sales_order" ? (
            <FormLayout
              className="ui-panel ui-form-grid"
              onSubmit={(e) => {
                e.preventDefault();
                void send(payload(new FormData(e.currentTarget), "confirm-general"));
              }}
            >
              <h2>Confirm reported issue outcome</h2>
              <p>
                An independent Support manager reviews the proposed resolution and records why the
                issue can close.
              </p>
              {reason}
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !detail.canConfirm}>
                Confirm outcome and close
              </Button>
              {!detail.canConfirm && <p>Another Support manager must review this resolution.</p>}
            </FormLayout>
          ) : (
            <FormLayout
              className="ui-panel ui-form-grid"
              onSubmit={(e) => {
                e.preventDefault();
                const f = new FormData(e.currentTarget),
                  note = detail.notes.find((n) => n.id === f.get("note"));
                if (note)
                  void send({
                    ...payload(f, "confirm"),
                    noteId: note.id,
                    noteVersion: Number(note.version),
                  });
              }}
            >
              <h2>Record customer confirmation</h2>
              <p>
                An independent manager must review a customer contact recorded after resolution.
                This is a staff-recorded confirmation; no Portal or email is sent.
              </p>
              {detail.notes.map((n) => (
                <p key={String(n.id)}>
                  {String(n.channel)} · {time(Number(n.occurred_at))} · {String(n.body)}
                </p>
              ))}
              <Field appearance="standard">
                Customer contact evidence
                <Select appearance="standard" name="note" required>
                  <option value="">Choose contact evidence</option>
                  {detail.notes.map((n) => (
                    <option key={String(n.id)} value={String(n.id)}>
                      {String(n.channel)} · {time(Number(n.occurred_at))} · v{String(n.version)}
                    </option>
                  ))}
                </Select>
              </Field>
              {reason}
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !detail.canConfirm}>
                Confirm customer outcome and close
              </Button>
              {!detail.canConfirm && <p>Another Support manager must review this resolution.</p>}
            </FormLayout>
          )}
        </>
      )}
      {r && detail?.canManage && r.status === "closed" && (
        <SupportDecisionForm
          label={r.source_kind !== "sales_order" ? "Reopen reported issue" : "Reopen customer dispute"}
          onSubmit={(data) => {
            void send(payload(data, "reopen"));
          }}
        >
          {reason}
          {button(
            r.source_kind !== "sales_order" ? "Reopen reported issue" : "Reopen customer dispute",
          )}
        </SupportDecisionForm>
      )}
      {r && detail?.canManage && !["closed", "cancelled"].includes(String(r.status)) && (
        <SupportDecisionForm
          label={"Cancel case with reason"}
          onSubmit={(data) => {
            void send(payload(data, "cancel"));
          }}
        >
          {reason}
          {button("Cancel case with reason")}
        </SupportDecisionForm>
      )}
    </>
  );
}
