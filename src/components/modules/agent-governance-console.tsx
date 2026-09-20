"use client";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import type { agentWorkspace } from "@/server/agent-governance";
export function AgentGovernanceConsole({
  base,
  data,
}: {
  base: string;
  data: Awaited<ReturnType<typeof agentWorkspace>>;
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
      const r = await workFetch(`/api/v1${base}/agents`, {
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
        throw new Error(
          j.error?.message ?? "Unable to update agent governance.",
        );
      }
      last.current = null;
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry this action.");
    } finally {
      setBusy(false);
    }
  }
  const values = (e: React.FormEvent<HTMLFormElement>) =>
    Object.fromEntries(new FormData(e.currentTarget));
  return (
    <>
      {error && <p role="alert">{error}</p>}
      <form
        className="card business-form"
        onSubmit={(e) => {
          e.preventDefault();
          void send({ ...values(e), entity: "agent" });
        }}
      >
        <h2>Draft agent</h2>
        <label>
          Name
          <input name="name" required />
        </label>
        <label>
          Purpose
          <textarea name="purpose" required minLength={3} />
        </label>
        <button className="button" disabled={busy}>
          Draft agent
        </button>
      </form>
      {data.agents
        .filter((x) => x.status === "draft")
        .map((x) => (
          <Review
            key={String(x.id)}
            entity="agent"
            row={x}
            send={send}
            busy={busy}
          />
        ))}
      <form
        className="card business-form"
        onSubmit={(e) => {
          e.preventDefault();
          const f = values(e);
          void send({
            ...f,
            entity: "version",
            allowedSources: String(f.allowedSources)
              .split(/\s+/)
              .filter(Boolean),
          });
        }}
      >
        <h2>Draft version</h2>
        <label>
          Approved agent
          <select name="agentId">
            {data.agents
              .filter((x) => x.status === "approved")
              .map((x) => (
                <option key={String(x.id)} value={String(x.id)}>
                  {String(x.name)}
                </option>
              ))}
          </select>
        </label>
        <label>
          Read-only prompt
          <textarea name="prompt" required minLength={3} />
        </label>
        <label>
          Allowed paths
          <input name="allowedSources" placeholder="/home /work" required />
        </label>
        <button className="button" disabled={busy}>
          Draft version
        </button>
      </form>
      {data.versions
        .filter((x) => x.status === "draft")
        .map((x) => (
          <Review
            key={String(x.id)}
            entity="version"
            row={x}
            send={send}
            busy={busy}
          />
        ))}
      <form
        className="card business-form"
        onSubmit={(e) => {
          e.preventDefault();
          const f = new FormData(e.currentTarget);
          void send({
            entity: "pool",
            name: f.get("name"),
            userIds: f.getAll("userIds"),
          });
        }}
      >
        <h2>Create routing pool</h2>
        <label>
          Name
          <input name="name" required />
        </label>
        <fieldset>
          <legend>Members</legend>
          {data.people.map((x) => (
            <label key={String(x.id)}>
              <input type="checkbox" name="userIds" value={String(x.id)} />
              {String(x.name)}
            </label>
          ))}
        </fieldset>
        <button className="button" disabled={busy}>
          Create pool
        </button>
      </form>
      <form
        className="card business-form"
        onSubmit={(e) => {
          e.preventDefault();
          const f = values(e);
          void send({
            entity: "assignment",
            agentVersionId: f.agentVersionId,
            poolId: f.poolId || undefined,
            userId: f.poolId ? undefined : f.userId,
            scopePath: f.scopePath,
          });
        }}
      >
        <h2>Assign approved version</h2>
        <label>
          Version
          <select name="agentVersionId">
            {data.versions
              .filter((x) => x.status === "approved")
              .map((x) => (
                <option key={String(x.id)} value={String(x.id)}>
                  {String(x.agent_name)} v{String(x.version_number)}
                </option>
              ))}
          </select>
        </label>
        <label>
          Pool
          <select name="poolId">
            <option value="">Direct user</option>
            {data.pools
              .filter((x) => x.status === "active")
              .map((x) => (
                <option key={String(x.id)} value={String(x.id)}>
                  {String(x.name)}
                </option>
              ))}
          </select>
        </label>
        <label>
          Direct user
          <select name="userId">
            {data.people.map((x) => (
              <option key={String(x.id)} value={String(x.id)}>
                {String(x.name)}
              </option>
            ))}
          </select>
        </label>
        <label>
          Path scope
          <input name="scopePath" defaultValue="/" required />
        </label>
        <button className="button" disabled={busy}>
          Assign
        </button>
      </form>
    </>
  );
}
function Review({
  entity,
  row,
  send,
  busy,
}: {
  entity: "agent" | "version";
  row: Record<string, unknown>;
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
          id: row.id,
          action: "approve",
          expectedVersion: Number(row.version),
          reason: String(new FormData(e.currentTarget).get("reason")),
        });
      }}
    >
      <h2>Independent review · {String(row.name ?? row.agent_name)}</h2>
      <label>
        Reason
        <textarea name="reason" required minLength={3} />
      </label>
      <button className="button" disabled={busy}>
        Approve
      </button>
    </form>
  );
}
