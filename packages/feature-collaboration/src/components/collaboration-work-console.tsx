"use client";
import WorkspaceLink from '@erpx/workplace/components/workspace-link';
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { FormDisclosure } from "@erpx/ui/components/form-disclosure";
import { Button, Field, Select, TextArea, TextInput } from "@erpx/ui/components/foundation";

import { useRef, useState } from "react";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import {captureWorkCommandContext,captureWorkCommandCompletion} from "@erpx/feature-work_projection/lib/command-context";
import {announceWorkChange} from "@erpx/feature-work_projection/lib/work-refresh";
import { PreferredDateTimeField } from "@erpx/feature-people_payroll/components/time/preferred-date-time-field";
import { matchesCollaborationReceipt, type CollaborationWorkReadModel } from "@erpx/feature-collaboration/lib/collaboration-work-read-model";
import { FormLayout } from "@erpx/ui/components/foundation";
export function CollaborationWorkConsole({
  base,
  data,
  commandUrl,
  exportHref,
}: {
  base: string;
  data: CollaborationWorkReadModel;
  commandUrl?: string;
  exportHref?: string | null;
}) {
  const router = useRouter(),
    last = useRef<{ key: string; body: string; assertCurrent: () => void; complete:()=>void; uncertain: boolean } | null>(null),
    inFlight = useRef(false),
    [busy, setBusy] = useState(false),
    [pending, setPending] = useState(false),
    [error, setError] = useState(""),
    [cardActions, setCardActions] = useState<Record<string, string>>({});
  async function send(input: unknown, retry = false) {
    if (inFlight.current) return;
    const body = retry && last.current ? last.current.body : JSON.stringify(input);
    if (body === undefined) return;
    if (last.current && last.current.body !== body) {
      setError("Confirm the original action first. Your newer entries remain unsent.");
      return;
    }
    if (!last.current) { last.current = { key: crypto.randomUUID(), body, assertCurrent: captureWorkCommandContext(), complete:captureWorkCommandCompletion(), uncertain: false }; setPending(true); }
    const command = last.current, priorUncertain = command.uncertain;
    let sent = false;
    inFlight.current = true;
    setBusy(true);
    setError("");
    try {
      command.assertCurrent();
      sent = true;
      const response = await workFetch(commandUrl ?? `/api/v1${base}/collaboration`, {
        method: "POST", headers: { "Content-Type": "application/json", "Idempotency-Key": command.key }, body: command.body,
      }, undefined, "caller");
      const result = await response.json();
      command.assertCurrent();
      if (!response.ok) {
        if (!priorUncertain && response.status < 500 && result.error?.code !== 'WORK_CONTEXT_CHANGED' && last.current === command) { last.current = null; setPending(false); }
        throw new Error(result.error?.message ?? "Unable to update collaboration work.");
      }
      if (!matchesCollaborationReceipt(command.body, command.key, result.data)) throw new Error("The acknowledgement is incomplete. Confirm the original action.");
      if (last.current === command) { last.current = null; setPending(false); command.complete(); announceWorkChange(); router.refresh(); }
    } catch (e) {
      if (last.current === command) {
        if (!sent && !priorUncertain) { last.current = null; setPending(false); }
        else command.uncertain = true;
      }
      try { command.assertCurrent(); setError(e instanceof Error ? e.message : "Confirm the original action."); } catch { /* The original attempt stays bound to its original work identity. */ }
    } finally {
      inFlight.current = false;
      setBusy(false);
    }
  }
  const types = data.sourceTypes;
  return (
    <>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {error && pending && <Button appearance="standard" clientManaged disabled={busy} onClick={() => void send(undefined, true)}>Confirm original action</Button>}
      <p>
        <WorkspaceLink href={canonicalUiHref(`${base}/inbox`)}>Open conversations and authorized thread files</WorkspaceLink>
      </p>
      {exportHref !== null && <p>
        <a href={exportHref ?? `/api/v1${base}/collaboration?format=csv`} download>
          Download authorized CSV export
        </a>
      </p>}
      {data.canWrite && (
        <>
          <FormDisclosure appearance="standard" className="ui-form-disclosure" dismissDisabled={busy}><summary>Link records</summary><FormLayout
            className="ui-panel ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              void send({
                entity: "relation",
                ...Object.fromEntries(new FormData(e.currentTarget)),
              });
            }}
          >
            <h2>Link records</h2>
            {data.fieldOnlyRelations&&<p>Relations must include a Field Service order. Support sources keep their own access rules.</p>}
            <Field appearance="standard">
              Source type
              <Select appearance="standard" name="sourceType">
                {types.map((x) => (
                  <option key={x}>{x}</option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Source id
              <TextInput appearance="standard" name="sourceId" required />
            </Field>
            <Field appearance="standard">
              Target type
              <Select appearance="standard" name="targetType">
                {types.map((x) => (
                  <option key={x}>{x}</option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Target id
              <TextInput appearance="standard" name="targetId" required />
            </Field>
            <Field appearance="standard">
              Relation
              <Select appearance="standard" name="relation">
                <option value="related">Related</option>
                <option value="blocks">Blocks</option>
                <option value="depends_on">Depends on</option>
                <option value="duplicates">Duplicates</option>
              </Select>
            </Field>
            <Field appearance="standard">
              Reason
              <TextArea appearance="standard" name="reason" required minLength={3} />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Create typed relation
            </Button>
          </FormLayout></FormDisclosure>
          <FormDisclosure appearance="standard" className="ui-form-disclosure" dismissDisabled={busy}><summary>Create action card</summary><FormLayout
            className="ui-panel ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = Object.fromEntries(new FormData(e.currentTarget));
              void send({ ...f, dueAt: new Date(String(f.dueAt)).getTime() });
            }}
          >
            <h2>Create action card</h2>
            <Field appearance="standard">
              Source type
              <Select appearance="standard" name="sourceType">
                {(data.cardSourceTypes??types).map((x) => (
                  <option key={x}>{x}</option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Source id
              <TextInput appearance="standard" name="sourceId" required />
            </Field>
            <Field appearance="standard">
              Title
              <TextInput appearance="standard" name="title" required />
            </Field>
            <Field appearance="standard">
              Owner
              <Select appearance="standard" name="ownerId">
                {data.people.map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.name)}
                  </option>
                ))}
              </Select>
            </Field>
            <PreferredDateTimeField appearance="standard" name="dueAt" label="Due" required />
            <Field appearance="standard">
              Reason
              <TextArea appearance="standard" name="reason" required minLength={3} />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Create action card
            </Button>
          </FormLayout></FormDisclosure>
        </>
      )}
      {data.relations.map((x) => (
        <FormLayout
          className="ui-panel ui-form-grid"
          key={String(x.id)}
          onSubmit={(e) => {
            e.preventDefault();
            void send({
              entity: "relation",
              id: x.id,
              action: "remove",
              expectedVersion: Number(x.version),
              reason: String(new FormData(e.currentTarget).get("reason")),
            });
          }}
        >
          <h2>
            {String(x.source_type)} {String(x.source_id)} · {String(x.relation)} ·{" "}
            {String(x.target_type)} {String(x.target_id)}
          </h2>
          <Field appearance="standard">
            Removal reason
            <TextArea appearance="standard" name="reason" required minLength={3} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            Remove relation
          </Button>
        </FormLayout>
      ))}
      {data.cards.map((x) => (
        <FormDisclosure key={String(x.id)} appearance="standard" className="ui-form-disclosure" dismissDisabled={busy}><summary>{String(x.title)}</summary><FormLayout
          className="ui-panel ui-form-grid"
          key={String(x.id)}
          onSubmit={(e) => {
            e.preventDefault();
            const f = Object.fromEntries(new FormData(e.currentTarget)),
              action = String(f.action);
            void send({
              ...f,
              id: x.id,
              expectedVersion: Number(x.version),
              dueAt: action === "reassign" ? new Date(String(f.dueAt)).getTime() : undefined,
            });
          }}
        >
          <h2>{String(x.title)}</h2>
          <div style={{ gridColumn: "1 / -1" }}>
            <p>Source: <WorkspaceLink href={`${base}${String(x.source_href)}`}>{String(x.source_title)}</WorkspaceLink></p>
            <p>Status: {String(x.status)} · Owner: {String(x.owner_name ?? x.owner_id)}</p>
            <p>Due: <TimeText value={Number(x.due_at)} appearance="standard" /></p>
          </div>
          {x.status === "completed" && x.completion_evidence && (
            <section aria-label="Completion evidence" style={{ gridColumn: "1 / -1" }}>
              <h3>Completion evidence</h3>
              <p style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>{String(x.completion_evidence)}</p>
            </section>
          )}
          {x.status === "open" && (
            <>
              <Field appearance="standard">
                Action
                <Select appearance="standard" name="action" value={cardActions[String(x.id)] ?? "complete"} onChange={event => setCardActions(current => ({ ...current, [String(x.id)]: event.target.value }))}>
                  <option value="complete">Complete</option>
                  {data.canReview && <option value="reassign">Reassign</option>}
                  {data.canReview && <option value="cancel">Cancel</option>}
                </Select>
              </Field>
              {(cardActions[String(x.id)] ?? "complete") === "reassign" && <>
              <Field appearance="standard">
                New owner
                <Select appearance="standard" name="ownerId">
                  <option value={String(x.owner_id)}>Keep current owner</option>
                  {data.people.map((p) => (
                    <option key={String(p.id)} value={String(p.id)}>
                      {String(p.name)}
                    </option>
                  ))}
                </Select>
              </Field>
              <PreferredDateTimeField appearance="standard" name="dueAt" label="New due" required />
              </>}
              {(cardActions[String(x.id)] ?? "complete") === "complete" && <Field appearance="standard">
                Completion evidence
                <TextArea appearance="standard" name="evidence" required minLength={3} />
              </Field>}
              <Field appearance="standard">
                Reason
                <TextArea appearance="standard" name="reason" required minLength={3} />
              </Field>
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                Record action
              </Button>
            </>
          )}
        </FormLayout></FormDisclosure>
      ))}
    </>
  );
}
