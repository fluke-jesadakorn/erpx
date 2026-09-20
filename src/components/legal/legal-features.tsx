"use client";
import { useEffect, useState } from "react";
import {
  featureSpecs,
  type FeatureKind,
  type FeatureRow,
  type FeatureSpec,
} from "@/lib/legal/features";
import type { LegalDetail } from "@/lib/legal/model";
import s from "./legal.module.css";
export function LegalFeatures({
  detail,
  people,
  call,
  command,
  busy,
}: {
  detail: LegalDetail;
  people: { id: string; name: string }[];
  call: (path: string, body?: unknown) => Promise<unknown>;
  command: (body: Record<string, unknown>) => Promise<boolean>;
  busy: boolean;
}) {
  const choices = Object.entries(featureSpecs).filter(([, v]) =>
    (v.kinds as string[]).includes(detail.item.kind),
  );
  const [category, setCategory] = useState<FeatureKind>(
      (choices[0]?.[0] ?? "custody") as FeatureKind,
    ),
    [groups, setGroups] = useState<
      { category: string; rows: FeatureRow[]; total: number; page: number }[]
    >([]),
    [page, setPage] = useState(1),
    [search, setSearch] = useState(""),
    [error, setError] = useState("");
  const spec: FeatureSpec = featureSpecs[category];
  useEffect(() => {
    let live = true;
    call(
      `/features/${detail.item.id}?category=${category}&page=${page}&search=${encodeURIComponent(search)}`,
    )
      .then((r) => {
        if (live) setGroups(r as typeof groups);
      })
      .catch((e) => {
        if (live) setError(e.message);
      });
    return () => {
      live = false;
    };
  }, [detail.item.id, detail.item.version, call, category, page, search]);
  const owners = people.filter((p) =>
    detail.members.some((m) => m.user_id === p.id),
  );
  return (
    <div className={s.detail}>
      {error && (
        <p role="alert" className={s.error}>
          {error}
        </p>
      )}
      <label className={s.field}>
        Workflow
        <select
          value={category}
          onChange={(e) => {
            setCategory(e.target.value as FeatureKind);
            setPage(1);
          }}
        >
          {choices.map(([k, v]) => (
            <option key={k} value={k}>
              {v.label}
            </option>
          ))}
        </select>
      </label>
      <h3>{spec.label}</h3>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          setSearch(String(new FormData(e.currentTarget).get("search") ?? ""));
          setPage(1);
        }}
      >
        <input
          name="search"
          aria-label="Filter workflow items"
          placeholder="Filter workflow items"
        />
        <button className={s.button}>Filter</button>
      </form>
      <p className={s.small}>
        {groups.find((g) => g.category === category)?.total ?? 0} workflow items
        · page {page}
      </p>
      <div className={s.actions}>
        <button
          className={s.button}
          disabled={page === 1}
          onClick={() => setPage(page - 1)}
        >
          Previous
        </button>
        <button
          className={s.button}
          disabled={
            page * 50 >=
            (groups.find((g) => g.category === category)?.total ?? 0)
          }
          onClick={() => setPage(page + 1)}
        >
          Next
        </button>
      </div>
      <form
        key={category}
        className={s.detail}
        onSubmit={async (e) => {
          e.preventDefault();
          const form = e.currentTarget,
            v = Object.fromEntries(new FormData(form)),
            data = Object.fromEntries(
              spec.fields.map((f) => [f.key, String(v[f.key] ?? "")]),
            );
          if (
            await command({
              action: "feature.create",
              category,
              title: v.title,
              ownerId: v.ownerId,
              dueDate: v.dueDate || null,
              data,
            })
          )
            form.reset();
        }}
      >
        <label className={s.field}>
          Title
          <input name="title" required minLength={3} />
        </label>
        <div className={s.grid}>
          <label className={s.field}>
            Owner
            <select name="ownerId" required>
              {owners.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </label>
          <label className={s.field}>
            Due date
            <input name="dueDate" type="date" />
          </label>
        </div>
        {spec.fields.map((f) => (
          <label className={s.field} key={f.key}>
            {f.label}
            {f.key === "documentId" ? (
              <select name={f.key} required>
                {detail.documents.map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.name} · v{d.version}
                  </option>
                ))}
              </select>
            ) : f.type === "textarea" ? (
              <textarea name={f.key} required={f.required} />
            ) : (
              <input
                name={f.key}
                type={f.type ?? "text"}
                required={f.required}
                min={0}
              />
            )}
          </label>
        ))}
        <button className={s.button} disabled={busy || !owners.length}>
          Create workflow item
        </button>
      </form>
      {groups
        .find((g) => g.category === category)
        ?.rows.map((row) => (
          <FeatureEntry
            key={`${row.id}:${row.version}`}
            row={row}
            busy={busy}
            command={command}
          />
        ))}
    </div>
  );
}
function FeatureEntry({
  row,
  busy,
  command,
}: {
  row: FeatureRow;
  busy: boolean;
  command: (body: Record<string, unknown>) => Promise<boolean>;
}) {
  const spec: FeatureSpec = featureSpecs[row.category],
    states = spec.states[row.state] ?? [];
  return (
    <article className={s.notice}>
      <h4>
        {row.title} <span className={s.badge}>{row.state}</span>
      </h4>
      <p className={s.small}>
        Due {row.due_date ?? "not set"} · v{row.version}
      </p>
      <dl>
        {spec.fields.map((f) => (
          <div key={f.key}>
            <dt>{f.label}</dt>
            <dd className={s.body}>{row.data[f.key] || "—"}</dd>
          </div>
        ))}
      </dl>
      {[
        "draft",
        "planned",
        "preparing",
        "open",
        "remediation",
        "in_progress",
        "received",
        "assessed",
        "requested",
        "pending",
      ].includes(row.state) && (
        <details>
          <summary>Edit workflow details</summary>
          <form
            className={s.detail}
            onSubmit={(e) => {
              e.preventDefault();
              const v = Object.fromEntries(new FormData(e.currentTarget));
              void command({
                action: "feature.update",
                category: row.category,
                featureId: row.id,
                featureVersion: row.version,
                data: Object.fromEntries(
                  spec.fields.map((f) => [f.key, String(v[f.key] ?? "")]),
                ),
                reason: v.reason,
              });
            }}
          >
            {spec.fields.map((f) => (
              <label className={s.field} key={f.key}>
                {f.label}
                <textarea
                  name={f.key}
                  required={f.required}
                  defaultValue={row.data[f.key]}
                />
              </label>
            ))}
            <label className={s.field}>
              Change reason
              <input name="reason" required minLength={3} />
            </label>
            <button className={s.button} disabled={busy}>
              Save workflow details
            </button>
          </form>
        </details>
      )}
      {states.length > 0 && (
        <form
          className={s.detail}
          onSubmit={async (e) => {
            e.preventDefault();
            const v = Object.fromEntries(new FormData(e.currentTarget));
            await command({
              action: "feature.transition",
              category: row.category,
              featureId: row.id,
              featureVersion: row.version,
              state: v.state,
              reason: v.reason,
              data: {
                ...row.data,
                evidence: String(v.evidence ?? ""),
                ...(row.category === "intake"
                  ? { response: String(v.response ?? "") }
                  : {}),
              },
            });
          }}
        >
          <label className={s.field}>
            Next state
            <select name="state">
              {states.map((state) => (
                <option key={state}>{state}</option>
              ))}
            </select>
          </label>
          {row.category === "intake" && (
            <label className={s.field}>
              Response
              <textarea name="response" defaultValue={row.data.response} />
            </label>
          )}
          <label className={s.field}>
            Evidence
            <textarea name="evidence" defaultValue={row.data.evidence} />
          </label>
          <label className={s.field}>
            Decision reason
            <input name="reason" required minLength={3} />
          </label>
          <button className={s.button} disabled={busy}>
            Apply workflow decision
          </button>
        </form>
      )}
    </article>
  );
}
