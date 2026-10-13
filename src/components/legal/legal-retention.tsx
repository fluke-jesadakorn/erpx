"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { TimeText } from "@/components/people_payroll/time/time-preference-context";
import { useEffect, useState } from "react";
import type { LegalDetail } from "@/server/modules/legal/public/rules/model";
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
        <InlineNotice tone="danger" role="alert" className={s.error}>
          {error}
        </InlineNotice>
      )}
      <h3>Retention and controlled destruction</h3>
      <p>
        Content is retained until an approved policy has elapsed and a different authorized reviewer
        approves a specific destruction manifest. Active holds block execution. Audit metadata and
        checksums remain.
      </p>
      <FormDisclosure appearance="standard">
        <summary>Company retention policies</summary>
        {data.policies.map((p) => (
          <div className={s.notice} key={p.id}>
            <strong>
              {p.name} · {p.days} days · {p.state}
            </strong>
            <p>{p.basis}</p>
            {p.state === "draft" && (
              <Button appearance="standard"
                type="submit"
                variant="ghost"
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
              </Button>
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
          <Field appearance="standard" className={s.field}>
            Policy name
            <TextInput appearance="standard" name="name" required />
          </Field>
          <Field appearance="standard" className={s.field}>
            Retention days after closure
            <TextInput appearance="standard" name="days" type="number" min={1} max={36500} required />
          </Field>
          <Field appearance="standard" className={s.field}>
            Legal basis and policy review evidence
            <TextArea appearance="standard" name="reason" required minLength={3} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="ghost" className={s.button} disabled={busy}>
            Create draft policy
          </Button>
        </form>
      </FormDisclosure>
      <form
        className={s.detail}
        onSubmit={async (e) => {
          e.preventDefault();
          const v = Object.fromEntries(new FormData(e.currentTarget));
          await act({ action: "retention.assign", ...v });
        }}
      >
        <Field appearance="standard" className={s.field}>
          Approved policy
          <Select appearance="standard" name="policyId" required>
            {data.policies
              .filter((p) => p.state === "approved")
              .map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name} · {p.days} days
                </option>
              ))}
          </Select>
        </Field>
        <Field appearance="standard" className={s.field}>
          Assignment reason
          <TextInput appearance="standard" name="reason" required minLength={3} />
        </Field>
        <Button appearance="standard"
          clientManaged type="submit"
          variant="ghost"
          className={s.button}
          disabled={busy || !data.policies.some((p) => p.state === "approved")}
        >
          Apply policy to closed record
        </Button>
      </form>
      {data.assignment && (
        <p className={s.notice}>
          Earliest destruction: <TimeText appearance="standard" value={data.assignment.eligible_at} />. A new policy
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
        <Field appearance="standard" className={s.field}>
          Destruction request reason
          <TextArea appearance="standard" name="reason" required minLength={3} />
        </Field>
        <Button appearance="standard"
          clientManaged type="submit"
          variant="ghost"
          className={s.button}
          disabled={busy || !data.assignment}
        >
          Prepare destruction manifest for review
        </Button>
      </form>
      {data.requests.map((r) => (
        <div className={s.notice} key={r.id}>
          <strong>{r.state}</strong>
          <p>{r.reason}</p>
          {r.error && <p>{r.error}</p>}
          <details>
            <summary>Review exact destruction manifest</summary>
            <pre className={s.body}>{JSON.stringify(JSON.parse(r.manifest), null, 2)}</pre>
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
              <Field appearance="standard" className={s.field}>
                Type the request ID to confirm permanent content destruction
                <TextInput appearance="standard" name="confirmation" required pattern={r.id} />
              </Field>
              <Field appearance="standard" className={s.field}>
                Independent decision and basis
                <TextArea appearance="standard" name="reason" required />
              </Field>
              <Button appearance="standard" clientManaged type="submit" variant="ghost" className={s.button} disabled={busy}>
                Authorize permanent destruction
              </Button>
            </form>
          )}
          {["requested", "approved", "blocked", "failed"].includes(r.state) && (
            <Button appearance="standard"
              type="submit"
              variant="ghost"
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
            </Button>
          )}
          {["failed", "blocked", "executing"].includes(r.state) && (
            <Button appearance="standard"
              type="submit"
              variant="ghost"
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
            </Button>
          )}
        </div>
      ))}
    </div>
  );
}
