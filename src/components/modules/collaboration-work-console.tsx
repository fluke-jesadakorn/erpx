"use client";
import { TimeText } from "@/components/time/time-preference-context";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import { PreferredDateTimeField } from "@/components/time/preferred-date-time-field";
import type { collaborationWorkspace } from "@/server/modules/collaboration/public/collaboration-work";
import { FormLayout } from "@/components/ui/foundation";
export function CollaborationWorkConsole({
  base,
  data,
}: {
  base: string;
  data: Awaited<ReturnType<typeof collaborationWorkspace>>;
}) {
  const router = useRouter(),
    last = useRef<{ key: string; body: string } | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [cardActions, setCardActions] = useState<Record<string, string>>({});
  async function send(input: unknown) {
    const body = JSON.stringify(input);
    if (last.current?.body !== body) last.current = { key: crypto.randomUUID(), body };
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(`/api/v1${base}/collaboration`, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": last.current.key,
          },
          body,
        }),
        result = await response.json();
      if (!response.ok) {
        if (response.status < 500) last.current = null;
        throw new Error(result.error?.message ?? "Unable to update collaboration work.");
      }
      last.current = null;
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry this action.");
    } finally {
      setBusy(false);
    }
  }
  const types = data.sourceTypes;
  return (
    <>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      <p>
        <a href={`${base}/inbox`}>Open conversations and authorized thread files</a>
      </p>
      <p>
        <a href={`/api/v1${base}/collaboration?format=csv`} download>
          Download authorized CSV export
        </a>
      </p>
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
            <p>Source: <a href={`${base}${String(x.source_href)}`}>{String(x.source_title)}</a></p>
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
