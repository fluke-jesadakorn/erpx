"use client";
import { useEffect, useState } from "react";
import type { LegalDetail } from "@/lib/legal/model";
import s from "./legal.module.css";
type Row = Record<string, string | number>;
export function LegalGovernance({
  detail,
  base,
  call,
  command,
  busy,
  manage,
}: {
  detail: LegalDetail;
  base: string;
  call: (p: string, b?: unknown) => Promise<unknown>;
  command: (b: Record<string, unknown>) => Promise<boolean>;
  busy: boolean;
  manage: boolean;
}) {
  const [data, setData] = useState<{
      forms: (Row & {
        fields: {
          key: string;
          label: string;
          required: boolean;
          type: string;
        }[];
      })[];
      authorities: Row[];
      settings: {
        enabled: boolean;
        ai_enabled: boolean;
        reminder_days: string | number[];
      } | null;
    } | null>(null),
    [error, setError] = useState(""),
    [form, setForm] = useState(""),
    [trust, setTrust] = useState<Row[]>([]),
    [references, setReferences] = useState<
      {
        kind: string;
        id: string;
        label: string;
        version: number;
        path: string;
      }[]
    >([]);
  useEffect(() => {
    let live = true;
    call(`/governance/${detail.item.id}`)
      .then((v) => {
        if (live) setData(v as typeof data);
      })
      .catch((e) => {
        if (live) setError(e.message);
      });
    return () => {
      live = false;
    };
  }, [call, detail.item.id, detail.item.version]);
  const selected = data?.forms.find((f) => f.id === form);
  return (
    <section className={s.detail}>
      {error && <p role="alert">{error}</p>}
      <details>
        <summary>References to existing ERPX records</summary>
        <p>
          A reference keeps the source permissions. Use document preservation to
          place source attachment bytes under a Legal hold.
        </p>
        <button
          className={s.button}
          onClick={async () => {
            try {
              setReferences(
                (await call(
                  `/references/${detail.item.id}`,
                )) as typeof references,
              );
            } catch (e) {
              setError((e as Error).message);
            }
          }}
        >
          Refresh accessible references
        </button>
        {references.map((r) => (
          <p key={`${r.kind}:${r.id}`}>
            <a href={`${base}/${r.path}`}>
              {r.kind} · {r.label} · v{r.version}
            </a>
          </p>
        ))}
        <form
          className={s.detail}
          onSubmit={async (e) => {
            e.preventDefault();
            if (
              await command({
                action: "reference.add",
                ...Object.fromEntries(new FormData(e.currentTarget)),
              })
            )
              setReferences(
                (await call(
                  `/references/${detail.item.id}`,
                )) as typeof references,
              );
          }}
        >
          <label className={s.field}>
            Record type
            <select name="targetKind">
              <option value="commerce_order">Sales / purchase order</option>
              <option value="person">People directory</option>
              <option value="employee_request">
                Employee / purchase request
              </option>
              <option value="expense">Finance expense</option>
            </select>
          </label>
          <label className={s.field}>
            Record ID
            <input name="targetId" required />
          </label>
          <button className={s.button} disabled={busy}>
            Link accessible source
          </button>
        </form>
      </details>
      {detail.item.kind === "request" && (
        <>
          <h3>Intake and SLA</h3>
          <form
            className={s.detail}
            onSubmit={(e) => {
              e.preventDefault();
              const v = Object.fromEntries(new FormData(e.currentTarget));
              void command({
                action: "intake.apply",
                formId: form,
                answers: Object.fromEntries(
                  (selected?.fields ?? []).map((f) => [f.key, v[f.key] ?? ""]),
                ),
                reason: v.reason,
                ...(v.escalationOwner
                  ? { escalationOwner: v.escalationOwner }
                  : {}),
              });
            }}
          >
            <label className={s.field}>
              Approved form
              <select
                value={form}
                required
                onChange={(e) => setForm(e.target.value)}
              >
                <option value="">Select</option>
                {data?.forms
                  .filter((f) => f.state === "approved")
                  .map((f) => (
                    <option key={String(f.id)} value={String(f.id)}>
                      {f.name} · SLA {f.sla_days} calendar days
                    </option>
                  ))}
              </select>
            </label>
            {selected?.fields.map((f) => (
              <label className={s.field} key={f.key}>
                {f.label}
                <input name={f.key} type={f.type} required={f.required} />
              </label>
            ))}
            <label className={s.field}>
              Escalation owner user ID
              <input name="escalationOwner" />
            </label>
            <label className={s.field}>
              Reason
              <input name="reason" required minLength={3} />
            </label>
            <button className={s.button} disabled={busy || !selected}>
              Validate intake and start SLA
            </button>
          </form>
        </>
      )}
      {["contract", "corporate"].includes(detail.item.kind) && (
        <>
          <h3>Signing authority</h3>
          <p>
            Record the signer’s authority separately from approval of contract
            wording.
          </p>
          <form
            className={s.detail}
            onSubmit={(e) => {
              e.preventDefault();
              void command({
                action: "authority.create",
                ...Object.fromEntries(new FormData(e.currentTarget)),
              });
            }}
          >
            {[
              ["userId", "Signer user ID", "text"],
              ["currency", "Currency", "text"],
              ["limitMinor", "Value limit in minor units", "number"],
              ["validFrom", "Valid from", "date"],
              ["validUntil", "Valid until", "date"],
              ["reason", "Authority basis", "text"],
            ].map(([name, label, type]) => (
              <label key={name} className={s.field}>
                {label}
                <input
                  name={name}
                  type={type}
                  required
                  defaultValue={
                    name === "currency" ? detail.item.currency : undefined
                  }
                />
              </label>
            ))}
            <label className={s.field}>
              Authority evidence
              <select name="documentId" required>
                {detail.documents.map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.name} · v{d.version}
                  </option>
                ))}
              </select>
            </label>
            <button className={s.button} disabled={busy}>
              Propose authority
            </button>
          </form>
          {data?.authorities.map((r) => (
            <form
              className={s.event}
              key={String(r.id)}
              onSubmit={(e) => {
                e.preventDefault();
                void command({
                  action:
                    r.state === "proposed"
                      ? "authority.approve"
                      : "authority.revoke",
                  authorityId: r.id,
                  authorityVersion: r.version,
                  ...Object.fromEntries(new FormData(e.currentTarget)),
                });
              }}
            >
              <p>
                {r.user_id} · {r.state} · {r.currency} {r.limit_minor} ·{" "}
                {r.valid_from} to {r.valid_until}
              </p>
              {r.state !== "revoked" && (
                <>
                  <input
                    name="reason"
                    aria-label="Authority decision reason"
                    placeholder="Decision reason"
                    required
                    minLength={3}
                  />
                  <button className={s.button} disabled={busy}>
                    {r.state === "proposed"
                      ? "Approve independently"
                      : "Revoke authority"}
                  </button>
                </>
              )}
            </form>
          ))}
        </>
      )}
      {manage && (
        <>
          <details>
            <summary>Company reminders and rollout</summary>
            <form
              key={JSON.stringify(data?.settings)}
              className={s.detail}
              onSubmit={(e) => {
                e.preventDefault();
                const v = Object.fromEntries(new FormData(e.currentTarget));
                void command({
                  action: "settings",
                  enabled: v.enabled === "on",
                  aiEnabled: v.aiEnabled === "on",
                  reminderDays: String(v.days)
                    .split(",")
                    .map((x) => Number(x.trim())),
                });
              }}
            >
              <label className={s.field}>
                Reminder days before due date
                <input
                  name="days"
                  defaultValue={
                    data?.settings
                      ? (typeof data.settings.reminder_days === "string"
                          ? JSON.parse(data.settings.reminder_days)
                          : data.settings.reminder_days
                        ).join(",")
                      : "90,60,30,7,1"
                  }
                  required
                />
              </label>
              <label>
                <input
                  type="checkbox"
                  name="enabled"
                  defaultChecked={data?.settings?.enabled ?? false}
                />
                Accept new Legal work for this Company
              </label>
              <label>
                <input
                  type="checkbox"
                  name="aiEnabled"
                  defaultChecked={data?.settings?.ai_enabled ?? false}
                />
                Allow Legal content to use the configured Company AI provider
              </label>
              <button className={s.button} disabled={busy}>
                Save Company settings
              </button>
            </form>
          </details>
          <details>
            <summary>Manage intake forms</summary>
            <form
              className={s.detail}
              onSubmit={(e) => {
                e.preventDefault();
                const v = Object.fromEntries(new FormData(e.currentTarget));
                void command({
                  action: "form.create",
                  name: v.name,
                  slaDays: Number(v.slaDays),
                  reason: v.reason,
                  fields: String(v.fields)
                    .split("\n")
                    .filter(Boolean)
                    .map((line) => {
                      const [key, label, type = "text", required = "true"] =
                        line.split("|").map((s) => s.trim());
                      return {
                        key,
                        label,
                        type,
                        required: required === "true",
                      };
                    }),
                });
              }}
            >
              <label className={s.field}>
                Name
                <input name="name" required />
              </label>
              <label className={s.field}>
                SLA calendar days
                <input
                  name="slaDays"
                  type="number"
                  min={1}
                  max={365}
                  defaultValue={7}
                />
              </label>
              <label className={s.field}>
                Fields: key | label | text/date/number | true/false
                <textarea
                  name="fields"
                  required
                  defaultValue="question | Legal question | text | true"
                />
              </label>
              <label className={s.field}>
                Reason
                <input name="reason" required minLength={3} />
              </label>
              <button className={s.button} disabled={busy}>
                Create form draft
              </button>
            </form>
            {data?.forms.map((f) => (
              <div key={String(f.id)} className={s.event}>
                {f.name} · {f.state}
                {f.state === "draft" && (
                  <button
                    className={s.button}
                    disabled={busy}
                    onClick={() =>
                      void command({
                        action: "form.approve",
                        formId: f.id,
                        formVersion: f.version,
                        reason: "Independent review of intake fields and SLA",
                      })
                    }
                  >
                    Approve independently
                  </button>
                )}
              </div>
            ))}
          </details>
          <details>
            <summary>Certificate trust store</summary>
            <p>
              Approve roots and revocation evidence only after checking their
              official source and fingerprint.
            </p>
            <button
              className={s.button}
              onClick={async () => {
                try {
                  setTrust((await call("/trust")) as Row[]);
                } catch (e) {
                  setError((e as Error).message);
                }
              }}
            >
              Refresh trust materials
            </button>
            <form
              className={s.detail}
              onSubmit={async (e) => {
                e.preventDefault();
                if (
                  await command({
                    action: "trust.add",
                    ...Object.fromEntries(new FormData(e.currentTarget)),
                  })
                )
                  setTrust((await call("/trust")) as Row[]);
              }}
            >
              <label className={s.field}>
                Type
                <select name="kind">
                  {["root", "intermediate", "crl", "ocsp"].map((k) => (
                    <option key={k}>{k}</option>
                  ))}
                </select>
              </label>
              {[
                ["name", "Name"],
                ["source", "Official HTTPS source"],
              ].map(([name, label]) => (
                <label key={name} className={s.field}>
                  {label}
                  <input name={name} required />
                </label>
              ))}
              <label className={s.field}>
                PEM certificate or base64 DER revocation evidence
                <textarea name="content" required />
              </label>
              <button className={s.button} disabled={busy}>
                Propose trust material
              </button>
            </form>
            {trust.map((r) => (
              <div key={String(r.id)} className={s.event}>
                <p>
                  {r.name} · {r.kind} · {r.state}
                </p>
                <code>{r.checksum}</code>
                <a href={String(r.source)} target="_blank" rel="noreferrer">
                  Official source
                </a>
                {r.state !== "retired" && (
                  <button
                    className={s.button}
                    disabled={busy}
                    onClick={async () => {
                      if (
                        await command({
                          action:
                            r.state === "proposed"
                              ? "trust.approve"
                              : "trust.retire",
                          materialId: r.id,
                          materialVersion: r.version,
                        })
                      )
                        setTrust((await call("/trust")) as Row[]);
                    }}
                  >
                    {r.state === "proposed"
                      ? "Approve independently"
                      : "Retire"}
                  </button>
                )}
              </div>
            ))}
          </details>
        </>
      )}
    </section>
  );
}
