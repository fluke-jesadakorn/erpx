"use client";
import { useState, useRef, type FormEvent } from "react";
import { useRouter } from "next/navigation";
import { Dialog } from "@/components/ui/dialog";
import { workFetch } from "@/lib/work-fetch";
import type {
  PeopleTemplatesData,
  HoldingPeopleTemplate,
} from "@/lib/people-templates";
import { attributeTypes, targetKinds } from "@/lib/user-management";
import styles from "./lifecycle.module.css";
export function HoldingTemplates({
  base,
  data,
}: {
  base: string;
  data: PeopleTemplatesData;
}) {
  const router = useRouter(),
    receipt = useRef<{ body: string; key: string } | null>(null);
  const [fields, setFields] = useState<
    HoldingPeopleTemplate["definition"]["attributes"]
  >([]);
  const [edit, setEdit] = useState<HoldingPeopleTemplate | null | undefined>(
      undefined,
    ),
    [preview, setPreview] = useState<{
      template: HoldingPeopleTemplate;
      token: string;
      attributes: string[];
      message: string;
    } | null>(null),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false);
  async function send(path: string, input: unknown) {
    const body = JSON.stringify(input);
    if (receipt.current?.body !== path + body)
      receipt.current = { body: path + body, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const r = await workFetch(
          "/api/v1" + base + "/people/templates/" + path,
          {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
              "Idempotency-Key": receipt.current.key,
            },
            body,
          },
        ),
        result = await r.json();
      if (!r.ok)
        throw Error(result.error?.message ?? "Unable to update template.");
      receipt.current = null;
      if (path !== "preview") router.refresh();
      return result.data;
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unable to update template.");
    } finally {
      setBusy(false);
    }
  }
  async function publish(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const f = new FormData(e.currentTarget),
      lines = (name: string) =>
        String(f.get(name) ?? "")
          .split("\n")
          .map((v) => v.trim())
          .filter(Boolean);
    const result = await send("publish", {
      id: edit?.id,
      expectedVersion: edit?.version ?? 0,
      name: String(f.get("name")),
      reason: String(f.get("reason")),
      definition: {
        onboardingTasks: lines("onboarding"),
        offboardingTasks: lines("offboarding"),
        attributes: fields,
      },
    });
    if (result) setEdit(undefined);
  }
  return (
    <section className={"card " + styles.stack}>
      <h2>Holding templates</h2>
      <p>
        Companies adopt a reviewed copy. Publishing a new revision leaves
        existing Company settings and access unchanged until adoption.
      </p>
      {data.canPublish && (
        <button
          className="button secondary"
          onClick={() => {
            setFields([]);
            setEdit(null);
          }}
        >
          Create Holding template
        </button>
      )}
      {data.templates.map((t) => (
        <article key={t.id}>
          <strong>{t.name}</strong> · revision {t.version} ·{" "}
          {t.adoptedVersion
            ? "Company uses revision " + t.adoptedVersion
            : "Not adopted"}
          <div className={styles.actions}>
            {data.canPublish && (
              <button
                className="button secondary"
                onClick={() => {
                  setFields(t.definition.attributes);
                  setEdit(t);
                }}
              >
                Edit template
              </button>
            )}
            {data.canAdopt && (
              <button
                className="button secondary"
                disabled={busy}
                onClick={async () => {
                  const p = await send("preview", {
                    templateId: t.id,
                    revision: t.version,
                    reason: "Review Holding template adoption",
                  });
                  if (p)
                    setPreview({
                      template: t,
                      token: p.previewToken,
                      attributes: p.attributes,
                      message: p.message,
                    });
                }}
              >
                Preview Company adoption
              </button>
            )}
          </div>
        </article>
      ))}
      {error && <p role="alert">{error}</p>}
      <Dialog
        open={edit !== undefined}
        onClose={() => setEdit(undefined)}
        title={edit ? "Edit Holding template" : "Create Holding template"}
        presentation="responsive-panel"
      >
        <form className={styles.stack} onSubmit={publish}>
          <label>
            Template name
            <input
              name="name"
              defaultValue={edit?.name}
              required
              minLength={2}
            />
          </label>
          <label>
            Onboarding tasks, one per line
            <textarea
              name="onboarding"
              defaultValue={edit?.definition.onboardingTasks.join("\n")}
            />
          </label>
          <label>
            Offboarding tasks, one per line
            <textarea
              name="offboarding"
              defaultValue={edit?.definition.offboardingTasks.join("\n")}
            />
          </label>
          <fieldset>
            <legend>Shared custom fields</legend>
            {fields.map((f, i) => (
              <fieldset key={i}>
                <legend>Field {i + 1}</legend>
                <label>
                  Stable key
                  <input
                    required
                    pattern="[a-z][a-z0-9_-]{1,60}"
                    value={f.key}
                    disabled={
                      !!edit?.definition.attributes.some((a) => a.key === f.key)
                    }
                    onChange={(e) =>
                      setFields(
                        fields.map((a, n) =>
                          n === i ? { ...a, key: e.target.value } : a,
                        ),
                      )
                    }
                  />
                </label>
                <label>
                  Label
                  <input
                    required
                    minLength={2}
                    value={f.label}
                    onChange={(e) =>
                      setFields(
                        fields.map((a, n) =>
                          n === i ? { ...a, label: e.target.value } : a,
                        ),
                      )
                    }
                  />
                </label>
                <label>
                  Applies to
                  <select
                    value={f.target}
                    onChange={(e) =>
                      setFields(
                        fields.map((a, n) =>
                          n === i
                            ? {
                                ...a,
                                target: e.target.value as typeof f.target,
                              }
                            : a,
                        ),
                      )
                    }
                  >
                    {targetKinds.map((v) => (
                      <option key={v}>{v}</option>
                    ))}
                  </select>
                </label>
                <label>
                  Type
                  <select
                    value={f.type}
                    onChange={(e) =>
                      setFields(
                        fields.map((a, n) =>
                          n === i
                            ? { ...a, type: e.target.value as typeof f.type }
                            : a,
                        ),
                      )
                    }
                  >
                    {attributeTypes.map((v) => (
                      <option key={v}>{v}</option>
                    ))}
                  </select>
                </label>
                {f.type.endsWith("select") && (
                  <label>
                    Choices, separated by commas
                    <input
                      value={f.options.join(",")}
                      onChange={(e) =>
                        setFields(
                          fields.map((a, n) =>
                            n === i
                              ? { ...a, options: e.target.value.split(",") }
                              : a,
                          ),
                        )
                      }
                    />
                  </label>
                )}
                <label>
                  Owner role
                  <input
                    value={f.ownerRole ?? ""}
                    onChange={(e) =>
                      setFields(
                        fields.map((a, n) =>
                          n === i
                            ? { ...a, ownerRole: e.target.value || undefined }
                            : a,
                        ),
                      )
                    }
                  />
                </label>
                <label>
                  Viewer roles, separated by commas
                  <input
                    value={f.readRoles?.join(",") ?? ""}
                    onChange={(e) =>
                      setFields(
                        fields.map((a, n) =>
                          n === i
                            ? {
                                ...a,
                                readRoles: e.target.value
                                  ? e.target.value
                                      .split(",")
                                      .map((v) => v.trim())
                                  : [],
                              }
                            : a,
                        ),
                      )
                    }
                  />
                </label>
                <label>
                  Editor roles, separated by commas
                  <input
                    value={f.writeRoles?.join(",") ?? ""}
                    onChange={(e) =>
                      setFields(
                        fields.map((a, n) =>
                          n === i
                            ? {
                                ...a,
                                writeRoles: e.target.value
                                  ? e.target.value
                                      .split(",")
                                      .map((v) => v.trim())
                                  : [],
                              }
                            : a,
                        ),
                      )
                    }
                  />
                </label>
                <label>
                  <input
                    type="checkbox"
                    checked={f.required}
                    onChange={(e) =>
                      setFields(
                        fields.map((a, n) =>
                          n === i ? { ...a, required: e.target.checked } : a,
                        ),
                      )
                    }
                  />
                  Required
                </label>
                <label>
                  <input
                    type="checkbox"
                    checked={f.branchOverride !== false}
                    onChange={(e) =>
                      setFields(
                        fields.map((a, n) =>
                          n === i
                            ? { ...a, branchOverride: e.target.checked }
                            : a,
                        ),
                      )
                    }
                  />
                  Allow Branch overrides
                </label>
                <button
                  type="button"
                  className="button secondary"
                  onClick={() => setFields(fields.filter((_, n) => n !== i))}
                >
                  Remove from future adoption
                </button>
              </fieldset>
            ))}
            <button
              type="button"
              className="button secondary"
              onClick={() =>
                setFields([
                  ...fields,
                  {
                    key: "field_" + (fields.length + 1),
                    label: "New field",
                    help: "",
                    target: "person",
                    type: "text",
                    required: false,
                    options: [],
                    archived: false,
                    branchOverride: false,
                  },
                ])
              }
            >
              Add shared field
            </button>
          </fieldset>
          <label>
            Reason
            <textarea name="reason" required minLength={3} />
          </label>
          <button className="button primary" disabled={busy}>
            Publish template revision
          </button>
          {error && <p role="alert">{error}</p>}
        </form>
      </Dialog>
      <Dialog
        open={!!preview}
        onClose={() => setPreview(null)}
        title="Review Company adoption"
        presentation="responsive-panel"
      >
        {preview && (
          <div className={styles.stack}>
            <p>{preview.message}</p>
            <h3>Onboarding</h3>
            <ul>
              {preview.template.definition.onboardingTasks.map((t) => (
                <li key={t}>{t}</li>
              ))}
            </ul>
            <h3>Offboarding</h3>
            <ul>
              {preview.template.definition.offboardingTasks.map((t) => (
                <li key={t}>{t}</li>
              ))}
            </ul>
            <p>Fields: {preview.attributes.join(", ") || "None"}</p>
            <button
              className="button primary"
              disabled={busy}
              onClick={async () => {
                const r = await send("adopt", {
                  templateId: preview.template.id,
                  revision: preview.template.version,
                  reason: "Adopt reviewed Holding template",
                  previewToken: preview.token,
                });
                if (r) setPreview(null);
              }}
            >
              Adopt for this Company
            </button>
            {error && <p role="alert">{error}</p>}
          </div>
        )}
      </Dialog>
    </section>
  );
}
