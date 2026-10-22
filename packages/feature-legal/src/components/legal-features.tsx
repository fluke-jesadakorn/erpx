"use client";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { FormDisclosure } from "@erpx/ui/components/form-disclosure";
import { Button, Field, Select, TextArea, TextInput } from "@erpx/ui/components/foundation";

import { useEffect, useState } from "react";
import {
  featureSpecs,
  workflowFieldChoices,
  type FeatureKind,
  type FeatureRow,
  type FeatureSpec,
} from "@erpx/server-legal/public/rules/features";
import type { LegalDetail } from "@erpx/server-legal/public/rules/model";
import s from "@erpx/feature-legal/components/legal.module.css";
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
  const owners = people.filter((p) => detail.members.some((m) => m.user_id === p.id));
  return (
    <div className={s.detail}>
      {error && (
        <InlineNotice tone="danger" role="alert" className={s.error}>
          {error}
        </InlineNotice>
      )}
      <Field appearance="standard" className={s.field}>
        Workflow
        <Select appearance="standard"
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
        </Select>
      </Field>
      <h3>{spec.label}</h3>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          setSearch(String(new FormData(e.currentTarget).get("search") ?? ""));
          setPage(1);
        }}
      >
        <TextInput appearance="standard"
          name="search"
          aria-label="Filter workflow items"
          placeholder="Filter workflow items"
        />
        <Button appearance="standard" clientManaged type="submit" variant="ghost" className={s.button}>
          Filter
        </Button>
      </form>
      <p className={s.small}>
        {groups.find((g) => g.category === category)?.total ?? 0} workflow items · page {page}
      </p>
      <div className={s.actions}>
        <Button appearance="standard"
          type="submit"
          variant="ghost"
          className={s.button}
          disabled={page === 1}
          onClick={() => setPage(page - 1)}
        >
          Previous
        </Button>
        <Button appearance="standard"
          type="submit"
          variant="ghost"
          className={s.button}
          disabled={page * 50 >= (groups.find((g) => g.category === category)?.total ?? 0)}
          onClick={() => setPage(page + 1)}
        >
          Next
        </Button>
      </div>
      <form
        key={category}
        className={s.detail}
        onSubmit={async (e) => {
          e.preventDefault();
          const form = e.currentTarget,
            v = Object.fromEntries(new FormData(form)),
            data = Object.fromEntries(spec.fields.map((f) => [f.key, String(v[f.key] ?? "")]));
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
        <Field appearance="standard" className={s.field}>
          Title
          <TextInput appearance="standard" name="title" required minLength={3} />
        </Field>
        <div className={s.grid}>
          <Field appearance="standard" className={s.field}>
            Owner
            <Select appearance="standard" name="ownerId" required>
              {owners.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard" className={s.field}>
            Due date
            <TextInput appearance="standard" name="dueDate" type="date" />
          </Field>
        </div>
        {spec.fields.map((f) => (
          <Field appearance="standard" className={s.field} key={f.key}>
            {f.label}
            {f.key === "documentId" ? (
              <Select appearance="standard" name={f.key} required>
                {detail.documents.map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.name} · v{d.version}
                  </option>
                ))}
              </Select>
            ) : workflowFieldChoices[category]?.[f.key] ? (
              <Select appearance="standard" name={f.key} required={f.required} defaultValue=""><option value="">Select when applicable</option>{workflowFieldChoices[category]![f.key].map(value=><option key={value} value={value}>{value.replaceAll("_"," ")}</option>)}</Select>
            ) : f.type === "textarea" ? (
              <TextArea appearance="standard" name={f.key} required={f.required} />
            ) : (
              <TextInput appearance="standard" name={f.key} type={f.type ?? "text"} required={f.required} min={0} />
            )}
          </Field>
        ))}
        <Button appearance="standard"
          clientManaged type="submit"
          variant="ghost"
          className={s.button}
          disabled={busy || !owners.length}
        >
          Create workflow item
        </Button>
      </form>
      {groups
        .find((g) => g.category === category)
        ?.rows.map((row) => (
          <FeatureEntry key={`${row.id}:${row.version}`} row={row} busy={busy} command={command} />
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
        "awaiting_information",
        "assessed",
        "requested",
        "pending",
      ].includes(row.state) && (
        <FormDisclosure appearance="standard">
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
                data: Object.fromEntries(spec.fields.map((f) => [f.key, String(v[f.key] ?? "")])),
                reason: v.reason,
              });
            }}
          >
            {spec.fields.map((f) => (
              <Field appearance="standard" className={s.field} key={f.key}>
                {f.label}
                {workflowFieldChoices[row.category]?.[f.key] ? <Select appearance="standard" name={f.key} required={f.required} defaultValue={row.data[f.key]??""}><option value="">Select when applicable</option>{workflowFieldChoices[row.category]![f.key].map(value=><option key={value} value={value}>{value.replaceAll("_"," ")}</option>)}</Select> : <TextArea appearance="standard" name={f.key} required={f.required} defaultValue={row.data[f.key]} />}
              </Field>
            ))}
            <Field appearance="standard" className={s.field}>
              Change reason
              <TextInput appearance="standard" name="reason" required minLength={3} />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="ghost" className={s.button} disabled={busy}>
              Save workflow details
            </Button>
          </form>
        </FormDisclosure>
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
                ...(row.category === "intake" ? { response: String(v.response ?? "") } : {}),
              },
            });
          }}
        >
          <Field appearance="standard" className={s.field}>
            Next state
            <Select appearance="standard" name="state">
              {states.map((state) => (
                <option key={state}>{state}</option>
              ))}
            </Select>
          </Field>
          {row.category === "intake" && (
            <Field appearance="standard" className={s.field}>
              Response
              <TextArea appearance="standard" name="response" defaultValue={row.data.response} />
            </Field>
          )}
          <Field appearance="standard" className={s.field}>
            Evidence
            <TextArea appearance="standard" name="evidence" defaultValue={row.data.evidence} />
          </Field>
          <Field appearance="standard" className={s.field}>
            Decision reason
            <TextInput appearance="standard" name="reason" required minLength={3} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="ghost" className={s.button} disabled={busy}>
            Apply workflow decision
          </Button>
        </form>
      )}
    </article>
  );
}
