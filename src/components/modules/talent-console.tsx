"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import type { talentDetail, talentWorkspace } from "@/server/talent";
import { FormLayout } from "@/components/ui/foundation";
function useCommand(base: string) {
  const router = useRouter(),
    last = useRef<{ body: string; key: string } | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  async function send(input: unknown) {
    const body = JSON.stringify(input);
    if (last.current?.body !== body) last.current = { body, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const r = await workFetch(`/api/v1${base}/talent`, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": last.current.key,
          },
          body,
        }),
        j = await r.json();
      if (!r.ok) {
        if (r.status < 500) last.current = null;
        throw new Error(j.error?.message ?? "Unable to update Talent.");
      }
      last.current = null;
      router.push(
        `${base}/talent${input && typeof input === "object" && "entity" in input && ["application", "proposal", "offer"].includes(String(input.entity)) ? `/${input.entity === "application" && !("id" in input) ? j.data.id : location.pathname.split("/").at(-1)}` : ""}`,
      );
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry this action.");
    } finally {
      setBusy(false);
    }
  }
  return { send, busy, error };
}
export function TalentConsole({
  base,
  data,
  detail,
}: {
  base: string;
  data?: Awaited<ReturnType<typeof talentWorkspace>>;
  detail?: Awaited<ReturnType<typeof talentDetail>>;
}) {
  const { send, busy, error } = useCommand(base),
    a = detail?.application,
    p = detail?.proposal,
    o = detail?.offer;
  const form = (e: React.FormEvent<HTMLFormElement>) =>
    Object.fromEntries(new FormData(e.currentTarget));
  return (
    <>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {data && (
        <>
          <FormLayout
            className="ui-panel ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = form(e);
              void send({
                ...f,
                entity: "requisition",
                openings: Number(f.openings),
              });
            }}
          >
            <h2>Draft requisition</h2>
            <Field appearance="standard">
              Job title
              <TextInput appearance="standard" name="jobTitle" required />
            </Field>
            <Field appearance="standard">
              Openings
              <TextInput appearance="standard" name="openings" type="number" min="1" required />
            </Field>
            <Field appearance="standard">
              Target start
              <TextInput appearance="standard" name="targetStartDate" type="date" required />
            </Field>
            <Field appearance="standard">
              Description
              <TextArea appearance="standard" name="description" required minLength={3} />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Draft requisition
            </Button>
          </FormLayout>
          {data.requisitions
            .filter((x) => ["draft", "approved"].includes(String(x.status)))
            .map((x) => (
              <FormLayout
                className="ui-panel ui-form-grid"
                key={String(x.id)}
                onSubmit={(e) => {
                  e.preventDefault();
                  void send({
                    entity: "requisition",
                    id: x.id,
                    action: x.status === "draft" ? "approve" : "open",
                    expectedVersion: Number(x.version),
                    reason: String(new FormData(e.currentTarget).get("reason")),
                  });
                }}
              >
                <h2>
                  {x.status === "draft" ? "Review" : "Open"} · {String(x.job_title)}
                </h2>
                <Field appearance="standard">
                  Reason
                  <TextArea appearance="standard" name="reason" required minLength={3} />
                </Field>
                <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                  Continue
                </Button>
              </FormLayout>
            ))}
          <FormDisclosure appearance="standard" className="ui-form-disclosure" dismissDisabled={busy}><summary>Record candidate consent</summary><FormLayout
            className="ui-panel ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              void send({ ...form(e), entity: "candidate" });
            }}
          >
            <h2>Record candidate consent</h2>
            <Field appearance="standard">
              Legal name
              <TextInput appearance="standard" name="legalName" required />
            </Field>
            <Field appearance="standard">
              Email
              <TextInput appearance="standard" name="email" type="email" required />
            </Field>
            <Field appearance="standard">
              Phone
              <TextInput appearance="standard" name="phone" />
            </Field>
            <Field appearance="standard">
              Consent evidence
              <TextArea appearance="standard" name="consentEvidence" required minLength={3} />
            </Field>
            <Field appearance="standard">
              Retain until
              <TextInput appearance="standard" name="retentionUntil" type="date" required />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Create private candidate
            </Button>
          </FormLayout></FormDisclosure>
          <FormLayout
            className="ui-panel ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = form(e),
                r = data.requisitions.find((x) => x.id === f.requisitionId);
              void send({
                ...f,
                entity: "application",
                requisitionVersion: Number(r?.version),
              });
            }}
          >
            <h2>Create application</h2>
            <Field appearance="standard">
              Open requisition
              <Select appearance="standard" name="requisitionId">
                {data.requisitions
                  .filter((x) => x.status === "open")
                  .map((x) => (
                    <option key={String(x.id)} value={String(x.id)}>
                      {String(x.job_title)}
                    </option>
                  ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Candidate
              <Select appearance="standard" name="candidateId">
                {data.candidates
                  .filter((x) => x.status === "active")
                  .map((x) => (
                    <option key={String(x.id)} value={String(x.id)}>
                      {String(x.legal_name)}
                    </option>
                  ))}
              </Select>
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
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Create application
            </Button>
          </FormLayout>
        </>
      )}
      {a && a.status === "applied" && (
        <Action
          title="Schedule interview"
          action="interview"
          version={Number(a.version)}
          send={send}
          busy={busy}
        />
      )}
      {a && a.status === "interview" && !p && (
        <FormLayout
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = form(e);
            void send({
              ...f,
              entity: "proposal",
              applicationId: a.id,
              applicationVersion: Number(a.version),
              scoreBps: Number(f.scoreBps),
            });
          }}
        >
          <h2>Interview proposal</h2>
          <Field appearance="standard">
            Recommendation
            <Select appearance="standard" name="recommendation">
              <option value="hire">Hire</option>
              <option value="hold">Hold</option>
              <option value="reject">Reject</option>
            </Select>
          </Field>
          <Field appearance="standard">
            Score bps
            <TextInput appearance="standard" name="scoreBps" type="number" min="0" max="10000" required />
          </Field>
          <Field appearance="standard">
            Evidence
            <TextArea appearance="standard" name="evidence" required minLength={3} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            Submit proposal
          </Button>
        </FormLayout>
      )}
      {a && p?.status === "submitted" && (
        <Action
          title="Independently accept proposal"
          action="accept-proposal"
          entity="proposal"
          id={String(p.id)}
          version={Number(p.version)}
          send={send}
          busy={busy}
        />
      )}
      {a && a.status === "interview" && p?.status === "accepted" && (
        <Action
          title="Select candidate"
          action="select"
          version={Number(a.version)}
          send={send}
          busy={busy}
        />
      )}
      {a && a.status === "selected" && !o && (
        <FormDisclosure appearance="standard" className="ui-form-disclosure" dismissDisabled={busy}><summary>Draft offer</summary><FormLayout
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            void send({
              ...form(e),
              entity: "offer",
              applicationId: a.id,
              applicationVersion: Number(a.version),
            });
          }}
        >
          <h2>Draft offer</h2>
          <Field appearance="standard">
            Currency
            <TextInput appearance="standard" name="currency" defaultValue="THB" required />
          </Field>
          <Field appearance="standard">
            Base amount
            <TextInput appearance="standard" name="baseAmount" required />
          </Field>
          <Field appearance="standard">
            Start date
            <TextInput appearance="standard" name="startDate" type="date" required />
          </Field>
          <Field appearance="standard">
            Expires on
            <TextInput appearance="standard" name="expiresOn" type="date" required />
          </Field>
          <Field appearance="standard">
            Terms
            <TextArea appearance="standard" name="terms" required minLength={3} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            Draft offer
          </Button>
        </FormLayout></FormDisclosure>
      )}
      {o && o.status === "draft" && (
        <Action
          title="Approve offer"
          action="approve-offer"
          entity="offer"
          id={String(o.id)}
          version={Number(o.version)}
          send={send}
          busy={busy}
        />
      )}{" "}
      {o && o.status === "approved" && (
        <Action
          title="Send offer"
          action="send"
          entity="offer"
          id={String(o.id)}
          version={Number(o.version)}
          send={send}
          busy={busy}
        />
      )}{" "}
      {o && o.status === "sent" && (
        <FormLayout
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = form(e);
            void send({
              entity: "offer",
              id: o.id,
              action: "accept-offer",
              expectedVersion: Number(o.version),
              reason: f.reason,
              acceptanceEvidence: f.acceptanceEvidence,
            });
          }}
        >
          <h2>Record candidate acceptance</h2>
          <Field appearance="standard">
            Evidence
            <TextArea appearance="standard" name="acceptanceEvidence" required minLength={3} />
          </Field>
          <Field appearance="standard">
            Reason
            <TextArea appearance="standard" name="reason" required minLength={3} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            Accept and hand off
          </Button>
        </FormLayout>
      )}
    </>
  );
}
function Action({
  title,
  action,
  entity = "application",
  id,
  version,
  send,
  busy,
}: {
  title: string;
  action: string;
  entity?: string;
  id?: string;
  version: number;
  send: (i: unknown) => Promise<void>;
  busy: boolean;
}) {
  return (
    <FormLayout
      className="ui-panel ui-form-grid"
      onSubmit={(e) => {
        e.preventDefault();
        void send({
          entity,
          id: id ?? location.pathname.split("/").at(-1),
          action,
          expectedVersion: version,
          reason: String(new FormData(e.currentTarget).get("reason")),
        });
      }}
    >
      <h2>{title}</h2>
      <Field appearance="standard">
        Reason
        <TextArea appearance="standard" name="reason" required minLength={3} />
      </Field>
      <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
        Continue
      </Button>
    </FormLayout>
  );
}
