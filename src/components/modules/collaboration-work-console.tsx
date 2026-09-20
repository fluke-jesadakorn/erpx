"use client";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import type { collaborationWorkspace } from "@/server/collaboration-work";
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
    [error, setError] = useState("");
  async function send(input: unknown) {
    const body = JSON.stringify(input);
    if (last.current?.body !== body)
      last.current = { key: crypto.randomUUID(), body };
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
        throw new Error(
          result.error?.message ?? "Unable to update collaboration work.",
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
  const types = data.sourceTypes;
  return (
    <>
      {error && <p role="alert">{error}</p>}
      <p>
        <a href={`${base}/inbox`}>
          Open conversations and authorized thread files
        </a>
      </p>
      {data.canWrite && (
        <>
          <form
            className="card business-form"
            onSubmit={(e) => {
              e.preventDefault();
              void send({
                entity: "relation",
                ...Object.fromEntries(new FormData(e.currentTarget)),
              });
            }}
          >
            <h2>Link records</h2>
            <label>
              Source type
              <select name="sourceType">
                {types.map((x) => (
                  <option key={x}>{x}</option>
                ))}
              </select>
            </label>
            <label>
              Source id
              <input name="sourceId" required />
            </label>
            <label>
              Target type
              <select name="targetType">
                {types.map((x) => (
                  <option key={x}>{x}</option>
                ))}
              </select>
            </label>
            <label>
              Target id
              <input name="targetId" required />
            </label>
            <label>
              Relation
              <select name="relation">
                <option value="related">Related</option>
                <option value="blocks">Blocks</option>
                <option value="depends_on">Depends on</option>
                <option value="duplicates">Duplicates</option>
              </select>
            </label>
            <label>
              Reason
              <textarea name="reason" required minLength={3} />
            </label>
            <button className="button" disabled={busy}>
              Create typed relation
            </button>
          </form>
          <form
            className="card business-form"
            onSubmit={(e) => {
              e.preventDefault();
              const f = Object.fromEntries(new FormData(e.currentTarget));
              void send({ ...f, dueAt: new Date(String(f.dueAt)).getTime() });
            }}
          >
            <h2>Create action card</h2>
            <label>
              Source type
              <select name="sourceType">
                {types.map((x) => (
                  <option key={x}>{x}</option>
                ))}
              </select>
            </label>
            <label>
              Source id
              <input name="sourceId" required />
            </label>
            <label>
              Title
              <input name="title" required />
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
            <label>
              Due
              <input name="dueAt" type="datetime-local" required />
            </label>
            <label>
              Reason
              <textarea name="reason" required minLength={3} />
            </label>
            <button className="button" disabled={busy}>
              Create action card
            </button>
          </form>
        </>
      )}
      {data.relations.map((x) => (
        <form
          className="card business-form"
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
            {String(x.source_type)} {String(x.source_id)} · {String(x.relation)}{" "}
            · {String(x.target_type)} {String(x.target_id)}
          </h2>
          <label>
            Removal reason
            <textarea name="reason" required minLength={3} />
          </label>
          <button className="button" disabled={busy}>
            Remove relation
          </button>
        </form>
      ))}
      {data.cards.map((x) => (
        <form
          className="card business-form"
          key={String(x.id)}
          onSubmit={(e) => {
            e.preventDefault();
            const f = Object.fromEntries(new FormData(e.currentTarget)),
              action = String(f.action);
            void send({
              ...f,
              id: x.id,
              expectedVersion: Number(x.version),
              dueAt:
                action === "reassign"
                  ? new Date(String(f.dueAt)).getTime()
                  : undefined,
            });
          }}
        >
          <h2>{String(x.title)}</h2>
          <p>
            {String(x.source_type)} {String(x.source_id)} · {String(x.status)} ·{" "}
            {String(x.owner_name ?? x.owner_id)}
          </p>
          {x.status === "open" && (
            <>
              <label>
                Action
                <select name="action">
                  <option value="complete">Complete</option>
                  {data.canReview && <option value="reassign">Reassign</option>}
                  {data.canReview && <option value="cancel">Cancel</option>}
                </select>
              </label>
              <label>
                New owner
                <select name="ownerId">
                  <option value={String(x.owner_id)}>Keep current owner</option>
                  {data.people.map((p) => (
                    <option key={String(p.id)} value={String(p.id)}>
                      {String(p.name)}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                New due
                <input name="dueAt" type="datetime-local" />
              </label>
              <label>
                Completion evidence
                <textarea name="evidence" />
              </label>
              <label>
                Reason
                <textarea name="reason" required minLength={3} />
              </label>
              <button className="button" disabled={busy}>
                Record action
              </button>
            </>
          )}
        </form>
      ))}
    </>
  );
}
