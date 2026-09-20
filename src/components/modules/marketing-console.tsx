"use client";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import type { marketingDetail, marketingWorkspace } from "@/server/marketing";
export function MarketingConsole({
  base,
  data,
  detail,
}: {
  base: string;
  data?: Awaited<ReturnType<typeof marketingWorkspace>>;
  detail?: Awaited<ReturnType<typeof marketingDetail>>;
}) {
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
      const r = await workFetch(`/api/v1${base}/marketing`, {
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
        throw new Error(j.error?.message ?? "Unable to update Marketing.");
      }
      last.current = null;
      if (
        input &&
        typeof input === "object" &&
        "entity" in input &&
        input.entity === "candidate" &&
        !("id" in input)
      )
        router.push(`${base}/marketing/${j.data.id}`);
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry this action.");
    } finally {
      setBusy(false);
    }
  }
  const vals = (e: React.FormEvent<HTMLFormElement>) =>
      Object.fromEntries(new FormData(e.currentTarget)),
    c = detail?.candidate;
  return (
    <>
      {error && <p role="alert">{error}</p>}
      {data && (
        <>
          <form
            className="card business-form"
            onSubmit={(e) => {
              e.preventDefault();
              const f = vals(e);
              try {
                void send({
                  entity: "asset",
                  kind: f.kind,
                  name: f.name,
                  configuration: JSON.parse(String(f.configuration)),
                  preview: JSON.parse(String(f.preview)),
                });
              } catch {
                setError("Configuration and preview must be valid JSON.");
              }
            }}
          >
            <h2>Draft marketing asset</h2>
            <label>
              Kind
              <select name="kind">
                <option value="form">Form</option>
                <option value="content">Content</option>
                <option value="domain">Domain</option>
                <option value="provider">Provider</option>
              </select>
            </label>
            <label>
              Name
              <input name="name" required />
            </label>
            <label>
              Configuration JSON
              <textarea
                name="configuration"
                defaultValue={'{"verified":true}'}
                required
              />
            </label>
            <label>
              Preview JSON
              <textarea
                name="preview"
                defaultValue={'{"summary":"Review before activation"}'}
                required
              />
            </label>
            <button className="button" disabled={busy}>
              Draft asset
            </button>
          </form>
          {data.assets
            .filter((x) => x.status === "draft")
            .map((x) => (
              <form
                className="card business-form"
                key={String(x.id)}
                onSubmit={(e) => {
                  e.preventDefault();
                  void send({
                    entity: "asset",
                    id: x.id,
                    action: "activate",
                    expectedVersion: Number(x.version),
                    reason: String(new FormData(e.currentTarget).get("reason")),
                  });
                }}
              >
                <h2>Independent review · {String(x.name)}</h2>
                <label>
                  Reason
                  <textarea name="reason" required minLength={3} />
                </label>
                <button className="button" disabled={busy}>
                  Activate
                </button>
              </form>
            ))}
          <form
            className="card business-form"
            onSubmit={(e) => {
              e.preventDefault();
              const f = vals(e),
                form = data.assets.find((x) => x.id === f.formId);
              void send({
                ...f,
                entity: "candidate",
                formVersion: Number(form?.version),
                attribution: {
                  campaign: String(f.campaign),
                  source: String(f.source),
                },
              });
            }}
          >
            <h2>Capture consented lead</h2>
            <label>
              Active form
              <select name="formId">
                {data.assets
                  .filter((x) => x.kind === "form" && x.status === "active")
                  .map((x) => (
                    <option key={String(x.id)} value={String(x.id)}>
                      {String(x.name)}
                    </option>
                  ))}
              </select>
            </label>
            <label>
              Branch
              <select name="branchId">
                {data.branches.map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.name)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Contact name
              <input name="contactName" required />
            </label>
            <label>
              Contact detail
              <textarea name="contactDetail" required minLength={3} />
            </label>
            <label>
              Consent evidence
              <textarea name="consentEvidence" required minLength={3} />
            </label>
            <label>
              Campaign
              <input name="campaign" required />
            </label>
            <label>
              Source
              <input name="source" required />
            </label>
            <label>
              Brief
              <textarea name="brief" required minLength={3} />
            </label>
            <button className="button" disabled={busy}>
              Create lead candidate
            </button>
          </form>
        </>
      )}
      {c && c.status === "pending" && (
        <>
          <form
            className="card business-form"
            onSubmit={(e) => {
              e.preventDefault();
              const f = vals(e);
              void send({
                entity: "candidate",
                id: c.id,
                action: "record-engagement",
                expectedVersion: Number(c.version),
                eventType: f.eventType,
                channel: f.channel,
                reason: f.reason,
              });
            }}
          >
            <h2>Record engagement</h2>
            <label>
              Event
              <input name="eventType" required />
            </label>
            <label>
              Channel
              <input name="channel" required />
            </label>
            <label>
              Evidence
              <textarea name="reason" required minLength={3} />
            </label>
            <button className="button" disabled={busy}>
              Record
            </button>
          </form>
          <form
            className="card business-form"
            onSubmit={(e) => {
              e.preventDefault();
              const f = vals(e);
              void send({
                entity: "candidate",
                id: c.id,
                action: "handoff",
                expectedVersion: Number(c.version),
                reason: f.reason,
                dueDate: f.dueDate,
              });
            }}
          >
            <h2>Independent CRM handoff</h2>
            <label>
              Due date
              <input name="dueDate" type="date" required />
            </label>
            <label>
              Review evidence
              <textarea name="reason" required minLength={3} />
            </label>
            <button className="button" disabled={busy}>
              Create CRM lead
            </button>
          </form>
        </>
      )}
    </>
  );
}
