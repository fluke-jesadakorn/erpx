"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useState, useRef, type FormEvent } from "react";
import { useRouter } from "next/navigation";
import { Dialog } from "@/components/ui/dialog";
import { workFetch } from "@/lib/work_projection/work-fetch";
import type { PeopleTemplatesData, HoldingPeopleTemplate } from "@/lib/people_payroll/people-templates";
import { attributeTypes, targetKinds } from "@/lib/identity_access/user-management";
import styles from "@/components/people_payroll/lifecycle.module.css";
export function HoldingTemplates({ base, data }: { base: string; data: PeopleTemplatesData }) {
  const router = useRouter(),
    receipt = useRef<{ body: string; key: string } | null>(null);
  const [fields, setFields] = useState<HoldingPeopleTemplate["definition"]["attributes"]>([]);
  const [edit, setEdit] = useState<HoldingPeopleTemplate | null | undefined>(undefined),
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
      const r = await workFetch("/api/v1" + base + "/people/templates/" + path, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": receipt.current.key,
          },
          body,
        }),
        result = await r.json();
      if (!r.ok) throw Error(result.error?.message ?? "Unable to update template.");
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
        Companies adopt a reviewed copy. Publishing a new revision leaves existing Company settings
        and access unchanged until adoption.
      </p>
      {data.canPublish && (
        <Button appearance="standard"
          type="submit"
          variant="secondary"

          onClick={() => {
            setFields([]);
            setEdit(null);
          }}
        >
          Create Holding template
        </Button>
      )}
      {data.templates.map((t) => (
        <article key={t.id}>
          <strong>{t.name}</strong> · revision {t.version} ·{" "}
          {t.adoptedVersion ? "Company uses revision " + t.adoptedVersion : "Not adopted"}
          <div className={styles.actions}>
            {data.canPublish && (
              <Button appearance="standard"
                type="submit"
                variant="secondary"

                onClick={() => {
                  setFields(t.definition.attributes);
                  setEdit(t);
                }}
              >
                Edit template
              </Button>
            )}
            {data.canAdopt && (
              <Button appearance="standard"
                type="submit"
                variant="secondary"

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
              </Button>
            )}
          </div>
        </article>
      ))}
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      <Dialog appearance="standard"
        open={edit !== undefined}
        onClose={() => setEdit(undefined)}
        title={edit ? "Edit Holding template" : "Create Holding template"}
        presentation="form-drawer"
      >
        <form className={styles.stack} onSubmit={publish}>
          <Field appearance="standard">
            Template name
            <TextInput appearance="standard" name="name" defaultValue={edit?.name} required minLength={2} />
          </Field>
          <Field appearance="standard">
            Onboarding tasks, one per line
            <TextArea appearance="standard"
              name="onboarding"
              defaultValue={edit?.definition.onboardingTasks.join("\n")}
            />
          </Field>
          <Field appearance="standard">
            Offboarding tasks, one per line
            <TextArea appearance="standard"
              name="offboarding"
              defaultValue={edit?.definition.offboardingTasks.join("\n")}
            />
          </Field>
          <fieldset>
            <legend>Shared custom fields</legend>
            {fields.map((f, i) => (
              <fieldset key={i}>
                <legend>Field {i + 1}</legend>
                <Field appearance="standard">
                  Stable key
                  <TextInput appearance="standard"
                    required
                    pattern="[a-z][a-z0-9_-]{1,60}"
                    value={f.key}
                    disabled={!!edit?.definition.attributes.some((a) => a.key === f.key)}
                    onChange={(e) =>
                      setFields(fields.map((a, n) => (n === i ? { ...a, key: e.target.value } : a)))
                    }
                  />
                </Field>
                <Field appearance="standard">
                  Label
                  <TextInput appearance="standard"
                    required
                    minLength={2}
                    value={f.label}
                    onChange={(e) =>
                      setFields(
                        fields.map((a, n) => (n === i ? { ...a, label: e.target.value } : a)),
                      )
                    }
                  />
                </Field>
                <Field appearance="standard">
                  Applies to
                  <Select appearance="standard"
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
                  </Select>
                </Field>
                <Field appearance="standard">
                  Type
                  <Select appearance="standard"
                    value={f.type}
                    onChange={(e) =>
                      setFields(
                        fields.map((a, n) =>
                          n === i ? { ...a, type: e.target.value as typeof f.type } : a,
                        ),
                      )
                    }
                  >
                    {attributeTypes.map((v) => (
                      <option key={v}>{v}</option>
                    ))}
                  </Select>
                </Field>
                {f.type.endsWith("select") && (
                  <Field appearance="standard">
                    Choices, separated by commas
                    <TextInput appearance="standard"
                      value={f.options.join(",")}
                      onChange={(e) =>
                        setFields(
                          fields.map((a, n) =>
                            n === i ? { ...a, options: e.target.value.split(",") } : a,
                          ),
                        )
                      }
                    />
                  </Field>
                )}
                <Field appearance="standard">
                  Owner role
                  <TextInput appearance="standard"
                    value={f.ownerRole ?? ""}
                    onChange={(e) =>
                      setFields(
                        fields.map((a, n) =>
                          n === i ? { ...a, ownerRole: e.target.value || undefined } : a,
                        ),
                      )
                    }
                  />
                </Field>
                <Field appearance="standard">
                  Viewer roles, separated by commas
                  <TextInput appearance="standard"
                    value={f.readRoles?.join(",") ?? ""}
                    onChange={(e) =>
                      setFields(
                        fields.map((a, n) =>
                          n === i
                            ? {
                                ...a,
                                readRoles: e.target.value
                                  ? e.target.value.split(",").map((v) => v.trim())
                                  : [],
                              }
                            : a,
                        ),
                      )
                    }
                  />
                </Field>
                <Field appearance="standard">
                  Editor roles, separated by commas
                  <TextInput appearance="standard"
                    value={f.writeRoles?.join(",") ?? ""}
                    onChange={(e) =>
                      setFields(
                        fields.map((a, n) =>
                          n === i
                            ? {
                                ...a,
                                writeRoles: e.target.value
                                  ? e.target.value.split(",").map((v) => v.trim())
                                  : [],
                              }
                            : a,
                        ),
                      )
                    }
                  />
                </Field>
                <Field appearance="standard" orientation="inline">
                  <TextInput appearance="standard"
                    type="checkbox"
                    checked={f.required}
                    onChange={(e) =>
                      setFields(
                        fields.map((a, n) => (n === i ? { ...a, required: e.target.checked } : a)),
                      )
                    }
                  />
                  Required
                </Field>
                <Field appearance="standard" orientation="inline">
                  <TextInput appearance="standard"
                    type="checkbox"
                    checked={f.branchOverride !== false}
                    onChange={(e) =>
                      setFields(
                        fields.map((a, n) =>
                          n === i ? { ...a, branchOverride: e.target.checked } : a,
                        ),
                      )
                    }
                  />
                  Allow Branch overrides
                </Field>
                <Button appearance="standard"
                  variant="secondary"
                  type="button"

                  onClick={() => setFields(fields.filter((_, n) => n !== i))}
                >
                  Remove from future adoption
                </Button>
              </fieldset>
            ))}
            <Button appearance="standard"
              variant="secondary"
              type="button"

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
            </Button>
          </fieldset>
          <Field appearance="standard">
            Reason
            <TextArea appearance="standard" name="reason" required minLength={3} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" className="primary" disabled={busy}>
            Publish template revision
          </Button>
          {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
        </form>
      </Dialog>
      <Dialog appearance="standard"
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
            <Button appearance="standard"
              type="submit"
              variant="primary"
              className="primary"
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
            </Button>
            {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
          </div>
        )}
      </Dialog>
    </section>
  );
}
