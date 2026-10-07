"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import type { incidentDetail } from "@/server/modules/support/public/support-incidents";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/attachments/attachment-panel";
import {
  AttachmentFeatureSurface,
  useAttachmentFeatureMode,
} from "@/components/attachments/attachment-feature-context";
import { FormLayout, Card } from "@/components/ui/foundation";
type Person = { id: string; name: string };
export function SupportIncidentConsole({
  base,
  source,
  recipients = [],
  detail,
}: {
  base: string;
  source?: { id: string; version: number };
  recipients?: Person[];
  detail?: Awaited<ReturnType<typeof incidentDetail>>;
}) {
  const attachmentMode = useAttachmentFeatureMode(),
    router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [evidence, setEvidence] = useState<AttachmentPanelChange>({
      setId: null,
      links: [],
      readyCount: 0,
      requiredCount: 0,
      bindable: true,
    }),
    last = useRef<{ key: string; fingerprint: string } | null>(null);
  async function send(input: unknown) {
    if (busy) return;
    setBusy(true);
    setError("");
    const fingerprint = JSON.stringify(input);
    if (last.current?.fingerprint !== fingerprint)
      last.current = { key: crypto.randomUUID(), fingerprint };
    try {
      const response = await workFetch(`/api/v1${base}/support/incidents`, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": last.current.key,
          },
          body: JSON.stringify(input),
        }),
        result = await response.json();
      if (!response.ok) {
        if (response.status < 500) last.current = null;
        throw new Error(result.error?.message ?? "Unable to record incident action.");
      }
      last.current = null;
      router.push(`${base}/support/incidents/${result.data.id}`);
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry this action.");
    } finally {
      setBusy(false);
    }
  }
  const reason = (
    <Field appearance="standard">
      Evidence and reason
      <TextArea appearance="standard" name="reason" required minLength={3} maxLength={4000} />
    </Field>
  );
  const owner = (people: Person[], value = "") => (
    <Field appearance="standard">
      Incident owner
      <Select appearance="standard" name="ownerId" defaultValue={value} required>
        <option value="">Choose eligible owner</option>
        {people.map((p) => (
          <option key={p.id} value={p.id}>
            {p.name}
          </option>
        ))}
      </Select>
    </Field>
  );
  const r = detail?.row,
    status = String(r?.status ?? "");
  const activeEvidence =
      attachmentMode === "enabled"
        ? evidence.links.filter((link) => !["withdrawn", "expired"].includes(link.linkState))
        : [],
    hasEvidence = activeEvidence.length > 0,
    evidencePending = hasEvidence && !evidence.bindable,
    evidenceActions = ["start", "update", "mitigate"];
  function attachmentSetIds(action: string) {
    return hasEvidence && evidence.bindable && evidence.setId && evidenceActions.includes(action)
      ? [evidence.setId]
      : [];
  }
  function form(action: string, title: string, extra?: React.ReactNode) {
    return (
      <form
        className="ui-panel ui-form-grid"
        onSubmit={(e) => {
          e.preventDefault();
          const f = new FormData(e.currentTarget),
            data = Object.fromEntries(f);
          void send({
            ...data,
            id: r!.id,
            action,
            expectedVersion: Number(r!.version),
            attachmentSetIds: attachmentSetIds(action),
            ...(action === "link"
              ? {
                  ticketVersion: Number(
                    detail!.candidates.find((t) => t.id === f.get("ticketId"))?.version,
                  ),
                }
              : {}),
          });
        }}
      >
        <h2>{title}</h2>
        {extra}
        {reason}
        <Button appearance="standard"
          clientManaged type="submit"
          variant="primary"

          disabled={busy || evidencePending || (hasEvidence && !evidenceActions.includes(action))}
        >
          {title}
        </Button>
      </form>
    );
  }
  return (
    <>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {source && (
        <FormLayout
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            void send({
              ...Object.fromEntries(f),
              action: "create",
              ticketId: source.id,
              ticketVersion: source.version,
            });
          }}
        >
          <h2>Declare shared incident</h2>
          <Field appearance="standard">
            Incident title
            <TextInput appearance="standard" name="title" minLength={3} maxLength={160} required />
          </Field>
          <Field appearance="standard">
            Customer impact
            <TextArea appearance="standard" name="impact" minLength={3} maxLength={4000} required />
          </Field>
          <Field appearance="standard">
            Severity
            <Select appearance="standard" name="severity">
              <option value="normal">Normal</option>
              <option value="high">High</option>
              <option value="critical">Critical</option>
            </Select>
          </Field>
          {owner(recipients)}
          <Field appearance="standard">
            Next update due
            <TextInput appearance="standard" name="dueDate" type="date" required />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            Declare shared incident
          </Button>
        </FormLayout>
      )}
      {detail?.canWork && ["open", "investigating"].includes(status) && (
        <AttachmentFeatureSurface>
          <Card as="section" appearance="standard" className="ui-panel">
            <AttachmentPanel appearance="standard"
              key={`${String(r!.id)}:${String(r!.version)}`}
              base={base}
              targetType="support_incident"
              targetId={String(r!.id)}
              expectedTargetVersion={Number(r!.version)}
              componentKind="action"
              componentRef={`incident:${status}`}
              purposeKey="incident_evidence"
              title="Incident evidence"
              description="Attach investigation photos or documents. Clean files bind atomically with Start, Update, or Propose mitigation."
              visibility="Restricted to the current incident owner and eligible Support managers."
              showHistory
              onChange={setEvidence}
            />
            {evidencePending && (
              <InlineNotice tone="info" className="notice" role="status">
                Wait for every selected file to pass security scanning before recording the incident
                action.
              </InlineNotice>
            )}
            {hasEvidence && evidence.bindable && (
              <InlineNotice tone="info" className="notice" role="status">
                The clean evidence will be locked to the next investigation action.
              </InlineNotice>
            )}
          </Card>
        </AttachmentFeatureSurface>
      )}
      {detail?.canWork && status === "open" && form("start", "Start investigation")}
      {detail?.canWork &&
        ["open", "investigating"].includes(status) &&
        form("update", "Record investigation update")}
      {detail?.canWork &&
        status === "investigating" &&
        form(
          "mitigate",
          "Propose mitigation",
          <>
            <Field appearance="standard">
              Root cause
              <TextArea appearance="standard" name="rootCause" required minLength={3} maxLength={4000} />
            </Field>
            <Field appearance="standard">
              Mitigation evidence
              <TextArea appearance="standard" name="mitigation" required minLength={3} maxLength={4000} />
            </Field>
          </>,
        )}
      {detail?.canReview && status === "mitigated" && form("close", "Confirm incident mitigation")}
      {detail?.canManage &&
        status === "mitigated" &&
        form("reject", "Request further investigation")}
      {detail?.canManage && status === "closed" && form("reopen", "Reopen incident")}
      {detail?.canManage && ["open", "investigating", "mitigated"].includes(status) && (
        <>
          {form(
            "reassign",
            "Reassign incident",
            <>
              {owner(detail.recipients, String(r!.owner_id))}
              <Field appearance="standard">
                Next update due
                <TextInput appearance="standard" name="dueDate" type="date" defaultValue={String(r!.due_date)} required />
              </Field>
            </>,
          )}
          {form("cancel", "Cancel incident with evidence")}
        </>
      )}
      {detail?.canManage &&
        ["open", "investigating"].includes(status) &&
        detail.links.length > 1 &&
        form(
          "unlink",
          "Correct an incorrectly linked case",
          <Field appearance="standard">
            Case to unlink
            <Select appearance="standard" name="ticketId" required>
              <option value="">Choose linked case</option>
              {detail.links.map((t) => (
                <option key={String(t.id)} value={String(t.id)}>
                  {String(t.title)}
                </option>
              ))}
            </Select>
          </Field>,
        )}
      {detail?.canManage &&
        ["open", "investigating"].includes(status) &&
        detail.candidates.length > 0 &&
        form(
          "link",
          "Link affected customer case",
          <Field appearance="standard">
            Customer case in the same scope
            <Select appearance="standard" name="ticketId" required>
              <option value="">Choose customer case</option>
              {detail.candidates.map((t) => (
                <option key={String(t.id)} value={String(t.id)}>
                  {String(t.title)}
                </option>
              ))}
            </Select>
          </Field>,
        )}
    </>
  );
}
