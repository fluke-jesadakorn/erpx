"use client";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import type { benefitDetail, benefitsWorkspace } from "@/server/benefits";
function useCommand(base: string) {
  const router = useRouter(),
    last = useRef<{ key: string; body: string } | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  async function send(input: unknown) {
    const body = JSON.stringify(input);
    if (last.current?.body !== body)
      last.current = { key: crypto.randomUUID(), body };
    setBusy(true);
    setError("");
    try {
      const r = await workFetch(`/api/v1${base}/benefits`, {
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
        throw new Error(j.error?.message ?? "Unable to update benefits.");
      }
      last.current = null;
      router.push(
        `${base}/benefits${input && typeof input === "object" && "entity" in input && input.entity === "election" ? `/${j.data.id}` : ""}`,
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
export function BenefitsConsole({
  base,
  data,
  detail,
}: {
  base: string;
  data?: Awaited<ReturnType<typeof benefitsWorkspace>>;
  detail?: Awaited<ReturnType<typeof benefitDetail>>;
}) {
  const { send, busy, error } = useCommand(base),
    e = detail?.election;
  return (
    <>
      {error && <p role="alert">{error}</p>}
      {data?.canReview && (
        <>
          <form
            className="card business-form"
            onSubmit={(x) => {
              x.preventDefault();
              const f = Object.fromEntries(new FormData(x.currentTarget));
              void send({
                ...f,
                entity: "plan",
                effectiveTo: f.effectiveTo || null,
              });
            }}
          >
            <h2>Draft benefit plan</h2>
            <label>
              Name
              <input name="name" required />
            </label>
            <label>
              Provider
              <input name="provider" required />
            </label>
            <label>
              Coverage
              <select name="coverageType">
                <option value="employee">Employee</option>
                <option value="family">Family</option>
              </select>
            </label>
            <label>
              Currency
              <input name="currency" defaultValue="THB" required />
            </label>
            <label>
              Employee deduction
              <input name="employeeDeduction" inputMode="decimal" required />
            </label>
            <label>
              Employer cost
              <input name="employerCost" inputMode="decimal" required />
            </label>
            <label>
              Effective from
              <input name="effectiveFrom" type="date" required />
            </label>
            <label>
              Effective to
              <input name="effectiveTo" type="date" />
            </label>
            <button className="button" disabled={busy}>
              Draft plan
            </button>
          </form>
          <form
            className="card business-form"
            onSubmit={(x) => {
              x.preventDefault();
              void send({
                ...Object.fromEntries(new FormData(x.currentTarget)),
                entity: "window",
              });
            }}
          >
            <h2>Draft enrollment window</h2>
            <label>
              Name
              <input name="name" required />
            </label>
            <label>
              Opens
              <input name="opensOn" type="date" required />
            </label>
            <label>
              Closes
              <input name="closesOn" type="date" required />
            </label>
            <label>
              Coverage effective
              <input name="effectiveOn" type="date" required />
            </label>
            <button className="button" disabled={busy}>
              Draft window
            </button>
          </form>
          {([
            ...data.plans
              .filter((x) => x.status === "draft")
              .map((x) => ({ entity: "plan", ...x })),
            ...data.windows
              .filter((x) => x.status === "draft")
              .map((x) => ({ entity: "window", ...x })),
          ] as Array<Record<string, unknown>>).map((x) => (
            <form
              className="card business-form"
              key={String(x.id)}
              onSubmit={(ev) => {
                ev.preventDefault();
                void send({
                  entity: x.entity,
                  id: x.id,
                  action: x.entity === "plan" ? "publish" : "open",
                  expectedVersion: Number(x.version),
                  reason: String(new FormData(ev.currentTarget).get("reason")),
                });
              }}
            >
              <h2>Independent review · {String(x.name)}</h2>
              <label>
                Reason
                <textarea name="reason" minLength={3} required />
              </label>
              <button className="button" disabled={busy}>
                {x.entity === "plan" ? "Publish plan" : "Open window"}
              </button>
            </form>
          ))}
        </>
      )}
      {data?.canWrite && (
        <form
          className="card business-form"
          onSubmit={(x) => {
            x.preventDefault();
            void send({
              ...Object.fromEntries(new FormData(x.currentTarget)),
              entity: "dependent",
            });
          }}
        >
          <h2>Add dependent</h2>
          <label>
            Name
            <input name="name" required />
          </label>
          <label>
            Relationship
            <input name="relationship" required />
          </label>
          <label>
            Birth date
            <input name="birthDate" type="date" required />
          </label>
          <button className="button" disabled={busy}>
            Submit dependent
          </button>
        </form>
      )}
      {data?.canReview &&
        data.dependents
          .filter((x) => x.status === "pending")
          .map((x) => (
            <form
              className="card business-form"
              key={String(x.id)}
              onSubmit={(ev) => {
                ev.preventDefault();
                void send({
                  entity: "dependent",
                  id: x.id,
                  action: "verify",
                  expectedVersion: Number(x.version),
                  reason: String(new FormData(ev.currentTarget).get("reason")),
                });
              }}
            >
              <h2>Verify dependent · {String(x.name)}</h2>
              <label>
                Reason
                <textarea name="reason" required minLength={3} />
              </label>
              <button className="button" disabled={busy}>
                Verify
              </button>
            </form>
          ))}
      {data?.canWrite && (
        <form
          className="card business-form"
          onSubmit={(x) => {
            x.preventDefault();
            const f = Object.fromEntries(new FormData(x.currentTarget)),
              plan = data.plans.find((p) => p.id === f.planId),
              win = data.windows.find((w) => w.id === f.windowId);
            void send({
              entity: "election",
              planId: f.planId,
              planVersion: Number(plan?.version),
              windowId: f.windowId,
              windowVersion: Number(win?.version),
              dependentIds: new FormData(x.currentTarget).getAll(
                "dependentIds",
              ),
            });
          }}
        >
          <h2>Elect coverage</h2>
          <label>
            Plan
            <select name="planId">
              {data.plans
                .filter((x) => x.status === "published")
                .map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.name)}
                  </option>
                ))}
            </select>
          </label>
          <label>
            Window
            <select name="windowId">
              {data.windows
                .filter((x) => x.status === "open")
                .map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.name)}
                  </option>
                ))}
            </select>
          </label>
          <fieldset>
            <legend>Verified dependents</legend>
            {data.dependents
              .filter((x) => x.status === "verified")
              .map((x) => (
                <label key={String(x.id)}>
                  <input
                    type="checkbox"
                    name="dependentIds"
                    value={String(x.id)}
                  />
                  {String(x.name)}
                </label>
              ))}
          </fieldset>
          <button className="button" disabled={busy}>
            Elect plan
          </button>
        </form>
      )}
      {e && !["released", "cancelled"].includes(String(e.status)) && (
        <form
          className="card business-form"
          onSubmit={(x) => {
            x.preventDefault();
            const f = Object.fromEntries(new FormData(x.currentTarget)),
              action =
                e.status === "elected"
                  ? "submit"
                  : e.status === "submitted"
                    ? "acknowledge"
                    : "release";
            void send({
              entity: "election",
              id: e.id,
              action,
              expectedVersion: Number(e.version),
              reason: f.reason,
              providerReference: f.providerReference || undefined,
            });
          }}
        >
          <h2>
            {e.status === "elected"
              ? "Submit election"
              : e.status === "submitted"
                ? "Record provider acknowledgement"
                : "Release payroll input"}
          </h2>
          {e.status === "submitted" && (
            <label>
              Provider reference
              <input name="providerReference" required />
            </label>
          )}
          <label>
            Reason
            <textarea name="reason" required minLength={3} />
          </label>
          <button className="button" disabled={busy}>
            Continue
          </button>
        </form>
      )}
    </>
  );
}
