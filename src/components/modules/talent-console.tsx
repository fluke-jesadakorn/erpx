"use client";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import type { talentDetail, talentWorkspace } from "@/server/talent";
function useCommand(base: string) {
  const router = useRouter(),
    last = useRef<{ body: string; key: string } | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  async function send(input: unknown) {
    const body = JSON.stringify(input);
    if (last.current?.body !== body)
      last.current = { body, key: crypto.randomUUID() };
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
      {error && <p role="alert">{error}</p>}
      {data && (
        <>
          <form
            className="card business-form"
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
            <label>
              Job title
              <input name="jobTitle" required />
            </label>
            <label>
              Openings
              <input name="openings" type="number" min="1" required />
            </label>
            <label>
              Target start
              <input name="targetStartDate" type="date" required />
            </label>
            <label>
              Description
              <textarea name="description" required minLength={3} />
            </label>
            <button className="button" disabled={busy}>
              Draft requisition
            </button>
          </form>
          {data.requisitions
            .filter((x) => ["draft", "approved"].includes(String(x.status)))
            .map((x) => (
              <form
                className="card business-form"
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
                  {x.status === "draft" ? "Review" : "Open"} ·{" "}
                  {String(x.job_title)}
                </h2>
                <label>
                  Reason
                  <textarea name="reason" required minLength={3} />
                </label>
                <button className="button" disabled={busy}>
                  Continue
                </button>
              </form>
            ))}
          <form
            className="card business-form"
            onSubmit={(e) => {
              e.preventDefault();
              void send({ ...form(e), entity: "candidate" });
            }}
          >
            <h2>Record candidate consent</h2>
            <label>
              Legal name
              <input name="legalName" required />
            </label>
            <label>
              Email
              <input name="email" type="email" required />
            </label>
            <label>
              Phone
              <input name="phone" />
            </label>
            <label>
              Consent evidence
              <textarea name="consentEvidence" required minLength={3} />
            </label>
            <label>
              Retain until
              <input name="retentionUntil" type="date" required />
            </label>
            <button className="button" disabled={busy}>
              Create private candidate
            </button>
          </form>
          <form
            className="card business-form"
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
            <label>
              Open requisition
              <select name="requisitionId">
                {data.requisitions
                  .filter((x) => x.status === "open")
                  .map((x) => (
                    <option key={String(x.id)} value={String(x.id)}>
                      {String(x.job_title)}
                    </option>
                  ))}
              </select>
            </label>
            <label>
              Candidate
              <select name="candidateId">
                {data.candidates
                  .filter((x) => x.status === "active")
                  .map((x) => (
                    <option key={String(x.id)} value={String(x.id)}>
                      {String(x.legal_name)}
                    </option>
                  ))}
              </select>
            </label>
            <label>
              Owner
              <select name="ownerId">
                {data.people.map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.name)}
                  </option>
                ))}
              </select>
            </label>
            <button className="button" disabled={busy}>
              Create application
            </button>
          </form>
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
        <form
          className="card business-form"
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
          <label>
            Recommendation
            <select name="recommendation">
              <option value="hire">Hire</option>
              <option value="hold">Hold</option>
              <option value="reject">Reject</option>
            </select>
          </label>
          <label>
            Score bps
            <input name="scoreBps" type="number" min="0" max="10000" required />
          </label>
          <label>
            Evidence
            <textarea name="evidence" required minLength={3} />
          </label>
          <button className="button" disabled={busy}>
            Submit proposal
          </button>
        </form>
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
        <form
          className="card business-form"
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
          <label>
            Currency
            <input name="currency" defaultValue="THB" required />
          </label>
          <label>
            Base amount
            <input name="baseAmount" required />
          </label>
          <label>
            Start date
            <input name="startDate" type="date" required />
          </label>
          <label>
            Expires on
            <input name="expiresOn" type="date" required />
          </label>
          <label>
            Terms
            <textarea name="terms" required minLength={3} />
          </label>
          <button className="button" disabled={busy}>
            Draft offer
          </button>
        </form>
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
        <form
          className="card business-form"
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
          <label>
            Evidence
            <textarea name="acceptanceEvidence" required minLength={3} />
          </label>
          <label>
            Reason
            <textarea name="reason" required minLength={3} />
          </label>
          <button className="button" disabled={busy}>
            Accept and hand off
          </button>
        </form>
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
    <form
      className="card business-form"
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
      <label>
        Reason
        <textarea name="reason" required minLength={3} />
      </label>
      <button className="button" disabled={busy}>
        Continue
      </button>
    </form>
  );
}
