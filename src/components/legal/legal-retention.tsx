"use client";
import { TimeText } from "@/components/time/time-preference-context";
import { useEffect, useState } from "react";
import type { LegalDetail } from "@/lib/legal/model";
import s from "./legal.module.css";
type Policy = {
  id: string;
  name: string;
  days: number;
  state: string;
  version: number;
  basis: string;
};
type Destruction = {
  id: string;
  state: string;
  reason: string;
  manifest: string;
  error: string | null;
};
export function LegalRetention({
  detail,
  call,
  command,
  busy,
}: {
  detail: LegalDetail;
  call: (path: string) => Promise<unknown>;
  command: (body: Record<string, unknown>) => Promise<boolean>;
  busy: boolean;
}) {
  const [data, setData] = useState<{
      policies: Policy[];
      assignment: { eligible_at: number; policy_id: string } | null;
      requests: Destruction[];
    }>({ policies: [], assignment: null, requests: [] }),
    [error, setError] = useState(""),
    [refresh, setRefresh] = useState(0);
  useEffect(() => {
    let live = true;
    call(`/retention/${detail.item.id}`)
      .then((r) => {
        if (live) setData(r as typeof data);
      })
      .catch((e) => {
        if (live) setError(e.message);
      });
    return () => {
      live = false;
    };
  }, [call, detail.item.id, detail.item.version, refresh]);
  async function act(body: Record<string, unknown>) {
    if (await command(body)) setRefresh((n) => n + 1);
  }
  return (
    <div className={s.detail}>
      {error && (
        <p role="alert" className={s.error}>
          {error}
        </p>
      )}
      <h3>Retention and controlled destruction</h3>
      <p>
        Content is retained until an approved policy has elapsed and a different
        authorized reviewer approves a specific destruction manifest. Active
        holds block execution. Audit metadata and checksums remain.
      </p>
      <details>
        <summary>Company retention policies</summary>
        {data.policies.map((p) => (
          <div className={s.notice} key={p.id}>
            <strong>
              {p.name} · {p.days} days · {p.state}
            </strong>
            <p>{p.basis}</p>
            {p.state === "draft" && (
              <button
                className={s.button}
                disabled={busy}
                onClick={() =>
                  act({
                    action: "retention.approve-policy",
                    policyId: p.id,
                    policyVersion: p.version,
                    reason: "Independent review of documented policy basis",
                  })
                }
              >
                Approve policy independently
              </button>
            )}
          </div>
        ))}
        <form
          className={s.detail}
          onSubmit={async (e) => {
            e.preventDefault();
            const v = Object.fromEntries(new FormData(e.currentTarget));
            await act({
              action: "retention.policy",
              ...v,
              days: Number(v.days),
            });
          }}
        >
          <label className={s.field}>
            Policy name
            <input name="name" required />
          </label>
          <label className={s.field}>
            Retention days after closure
            <input name="days" type="number" min={1} max={36500} required />
          </label>
          <label className={s.field}>
            Legal basis and policy review evidence
            <textarea name="reason" required minLength={3} />
          </label>
          <button className={s.button} disabled={busy}>
            Create draft policy
          </button>
        </form>
      </details>
      <form
        className={s.detail}
        onSubmit={async (e) => {
          e.preventDefault();
          const v = Object.fromEntries(new FormData(e.currentTarget));
          await act({ action: "retention.assign", ...v });
        }}
      >
        <label className={s.field}>
          Approved policy
          <select name="policyId" required>
            {data.policies
              .filter((p) => p.state === "approved")
              .map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name} · {p.days} days
                </option>
              ))}
          </select>
        </label>
        <label className={s.field}>
          Assignment reason
          <input name="reason" required minLength={3} />
        </label>
        <button
          className={s.button}
          disabled={busy || !data.policies.some((p) => p.state === "approved")}
        >
          Apply policy to closed record
        </button>
      </form>
      {data.assignment && (
        <p className={s.notice}>
          Earliest destruction:{" "}
          <TimeText value={data.assignment.eligible_at} />. A new policy
          version requires a new assignment; dates never change silently.
        </p>
      )}
      <form
        className={s.detail}
        onSubmit={async (e) => {
          e.preventDefault();
          await act({
            action: "retention.request",
            reason: new FormData(e.currentTarget).get("reason"),
          });
        }}
      >
        <label className={s.field}>
          Destruction request reason
          <textarea name="reason" required minLength={3} />
        </label>
        <button className={s.button} disabled={busy || !data.assignment}>
          Prepare destruction manifest for review
        </button>
      </form>
      {data.requests.map((r) => (
        <div className={s.notice} key={r.id}>
          <strong>{r.state}</strong>
          <p>{r.reason}</p>
          {r.error && <p>{r.error}</p>}
          <details>
            <summary>Review exact destruction manifest</summary>
            <pre className={s.body}>
              {JSON.stringify(JSON.parse(r.manifest), null, 2)}
            </pre>
          </details>
          <p className={s.small}>Request {r.id}</p>
          {r.state === "requested" && (
            <form
              className={s.detail}
              onSubmit={async (e) => {
                e.preventDefault();
                const v = Object.fromEntries(new FormData(e.currentTarget));
                await act({
                  action: "retention.approve",
                  requestId: r.id,
                  ...v,
                });
              }}
            >
              <label className={s.field}>
                Type the request ID to confirm permanent content destruction
                <input name="confirmation" required pattern={r.id} />
              </label>
              <label className={s.field}>
                Independent decision and basis
                <textarea name="reason" required />
              </label>
              <button className={s.button} disabled={busy}>
                Authorize permanent destruction
              </button>
            </form>
          )}
          {["requested", "approved", "blocked", "failed"].includes(r.state) && (
            <button
              className={s.button}
              disabled={busy}
              onClick={() =>
                act({
                  action: "retention.cancel",
                  requestId: r.id,
                  reason: "Cancelled before execution",
                })
              }
            >
              Cancel destruction request
            </button>
          )}
          {["failed", "blocked", "executing"].includes(r.state) && (
            <button
              className={s.button}
              disabled={busy}
              onClick={() =>
                act({
                  action: "retention.retry",
                  requestId: r.id,
                  reason: "Request fresh review after resolving blocker",
                })
              }
            >
              Return for independent review
            </button>
          )}
        </div>
      ))}
    </div>
  );
}
