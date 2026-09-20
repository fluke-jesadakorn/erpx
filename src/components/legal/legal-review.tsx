"use client";
import { useEffect, useState } from "react";
import { diffWordsWithSpace } from "diff";
import type { LegalDetail } from "@/lib/legal/model";
import s from "./legal.module.css";
type Props = {
  detail: LegalDetail;
  call: (path: string, body?: unknown) => Promise<unknown>;
  command: (body: Record<string, unknown>) => Promise<boolean>;
  busy: boolean;
};
export function LegalApproval({ detail, call, command, busy }: Props) {
  const [view, setView] = useState<{
      instance: { state: string } | null;
      steps: {
        id: string;
        ordinal: number;
        approver_user_id: string;
        state: string;
      }[];
      history: unknown[];
    } | null>(null),
    [error, setError] = useState("");
  useEffect(() => {
    let live = true;
    call(`/approval/${detail.item.id}`)
      .then((v) => {
        if (live) setView(v as typeof view);
      })
      .catch((e) => {
        if (live) setError(e.message);
      });
    return () => {
      live = false;
    };
  }, [call, detail.item.id, detail.item.version]);
  return (
    <section className={s.detail}>
      <h3>Contract approval route</h3>
      {error && <p role="alert">{error}</p>}
      {view?.steps.map((step) => (
        <p key={step.id}>
          {step.ordinal}. {step.approver_user_id} — {step.state}
        </p>
      ))}
      {["draft", "review", "approval"].includes(detail.item.state) && (
        <form
          className={s.detail}
          onSubmit={(e) => {
            e.preventDefault();
            const v = Object.fromEntries(new FormData(e.currentTarget));
            void command({
              action: "approval.open",
              ...v,
              approverIds: String(v.approverIds)
                .split(",")
                .map((v) => v.trim())
                .filter(Boolean),
              signerIds: String(v.signerIds)
                .split(",")
                .map((v) => v.trim())
                .filter(Boolean),
            });
          }}
        >
          <label className={s.field}>
            Document version
            <select name="documentId" required>
              {detail.documents.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.name} · v{d.version}
                </option>
              ))}
            </select>
          </label>
          <label className={s.field}>
            Approver user IDs in order, comma separated
            <input name="approverIds" required />
          </label>
          <label className={s.field}>
            Signer user IDs in order, comma separated
            <input name="signerIds" required />
          </label>
          <label className={s.field}>
            Reason
            <input name="reason" required minLength={3} />
          </label>
          <button className={s.button} disabled={busy}>
            Open approval route
          </button>
        </form>
      )}
      {view?.instance?.state === "pending" && (
        <>
          <form
            className={s.detail}
            onSubmit={(e) => {
              e.preventDefault();
              void command({
                action: "approval.decide",
                ...Object.fromEntries(new FormData(e.currentTarget)),
              });
            }}
          >
            <label className={s.field}>
              Decision
              <select name="decision">
                <option value="approve">Approve this locked version</option>
                <option value="revise">Return for revision</option>
                <option value="reject">Reject</option>
              </select>
            </label>
            <label className={s.field}>
              Reason
              <input name="reason" required minLength={3} />
            </label>
            <button className={s.button} disabled={busy}>
              Record my decision
            </button>
          </form>
          <form
            className={s.detail}
            onSubmit={(e) => {
              e.preventDefault();
              void command({
                action: "approval.reroute",
                ...Object.fromEntries(new FormData(e.currentTarget)),
              });
            }}
          >
            <label className={s.field}>
              Replacement reviewer user ID
              <input name="approverId" required />
            </label>
            <label className={s.field}>
              Reason for replacement
              <input name="reason" required minLength={3} />
            </label>
            <button className={s.button} disabled={busy}>
              Replace current reviewer
            </button>
          </form>
        </>
      )}
      <details>
        <summary>Decision history</summary>
        <pre className={s.body}>
          {JSON.stringify(view?.history ?? [], null, 2)}
        </pre>
      </details>
    </section>
  );
}
export function LegalEditor({
  detail,
  command,
  busy,
}: Pick<Props, "detail" | "command" | "busy">) {
  const docs = detail.documents.filter((d) => d.body),
    [left, setLeft] = useState(docs[1]?.id ?? docs[0]?.id ?? ""),
    [right, setRight] = useState(docs[0]?.id ?? ""),
    [sections, setSections] = useState([
      { id: crypto.randomUUID(), heading: "", body: "" },
    ]);
  const a = docs.find((d) => d.id === left)?.body ?? "",
    b = docs.find((d) => d.id === right)?.body ?? "",
    changes = diffWordsWithSpace(a, b, { timeout: 100, maxEditLength: 10000 });
  return (
    <section className={s.detail}>
      <details>
        <summary>Compare document versions</summary>
        <div className={s.grid}>
          {[
            [left, setLeft],
            [right, setRight],
          ].map(([value, set], i) => (
            <label className={s.field} key={i}>
              {i ? "Revised" : "Original"}
              <select
                value={value as string}
                onChange={(e) => (set as typeof setLeft)(e.target.value)}
              >
                {docs.map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.name} · v{d.version}
                  </option>
                ))}
              </select>
            </label>
          ))}
        </div>
        <div className={s.body}>
          {changes
            ? changes.map((p, i) =>
                p.added ? (
                  <ins key={i}>{p.value}</ins>
                ) : p.removed ? (
                  <del key={i}>{p.value}</del>
                ) : (
                  <span key={i}>{p.value}</span>
                ),
              )
            : "Comparison exceeds the interactive limit. Download both versions to review."}
        </div>
      </details>
      {detail.item.state === "draft" && (
        <details>
          <summary>Draft by clause</summary>
          <form
            className={s.detail}
            onSubmit={async (e) => {
              e.preventDefault();
              const v = new FormData(e.currentTarget);
              if (
                await command({
                  action: "document",
                  name: v.get("name"),
                  body: sections
                    .map((c) => `${c.heading}\n${c.body}`)
                    .join("\n\n"),
                })
              )
                setSections([
                  { id: crypto.randomUUID(), heading: "", body: "" },
                ]);
            }}
          >
            <label className={s.field}>
              Version name
              <input name="name" required />
            </label>
            {sections.map((c, i) => (
              <fieldset key={c.id}>
                <legend>Clause {i + 1}</legend>
                <label className={s.field}>
                  Heading
                  <input
                    value={c.heading}
                    onChange={(e) =>
                      setSections(
                        sections.map((x) =>
                          x.id === c.id ? { ...x, heading: e.target.value } : x,
                        ),
                      )
                    }
                  />
                </label>
                <label className={s.field}>
                  Text
                  <textarea
                    required
                    value={c.body}
                    onChange={(e) =>
                      setSections(
                        sections.map((x) =>
                          x.id === c.id ? { ...x, body: e.target.value } : x,
                        ),
                      )
                    }
                  />
                </label>
                <button
                  type="button"
                  className={s.button}
                  disabled={i === 0}
                  onClick={() => {
                    const next = [...sections];
                    [next[i - 1], next[i]] = [next[i], next[i - 1]];
                    setSections(next);
                  }}
                >
                  Move up
                </button>
                <button
                  type="button"
                  className={s.button}
                  disabled={sections.length === 1}
                  onClick={() =>
                    setSections(sections.filter((x) => x.id !== c.id))
                  }
                >
                  Remove clause
                </button>
              </fieldset>
            ))}
            <button
              type="button"
              className={s.button}
              onClick={() =>
                setSections([
                  ...sections,
                  { id: crypto.randomUUID(), heading: "", body: "" },
                ])
              }
            >
              Add clause
            </button>
            <button className={s.button} disabled={busy}>
              Save immutable draft version
            </button>
          </form>
        </details>
      )}
    </section>
  );
}
