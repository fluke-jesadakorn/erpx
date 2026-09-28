"use client";
import { BarChart3, BookOpen, Globe2, Plus, X, Scale } from "lucide-react";
import { workFetch } from "@/lib/work-fetch";
import { LegalApproval, LegalEditor } from "./legal-review";
import { LegalReports } from "./legal-reports";
import { LegalGovernance } from "./legal-governance";
import { LegalThailand } from "./legal-thailand";
import { LegalKnowledge } from "./legal-knowledge";
import { LegalDeadlines } from "./legal-deadlines";
import { LegalFeatures } from "./legal-features";
import { LegalRetention } from "./legal-retention";
import { useState, useCallback, type FormEvent } from "react";
import { OperationSummary } from "@/components/workplace/operation-summary";
import {
  kinds,
  labels,
  fields,
  allowedTransitions,
  type LegalKind,
  type LegalItem,
  type LegalData,
  type LegalDetail,
  type ItemInput,
} from "@/lib/legal/model";
import s from "./legal.module.css";
const button = `${s.button}`,
  primary = `${s.button} ${s.primary}`;
export function LegalWorkspace({
  base,
  initial,
  initialDetail = null,
}: {
  base: string;
  initial: LegalData;
  initialDetail?: LegalDetail | null;
}) {
  const [data, setData] = useState(initial),
    [kind, setKind] = useState<LegalKind>(
      initialDetail?.item.kind ?? initial.items[0]?.kind ?? "request",
    ),
    [search, setSearch] = useState(""),
    [state, setState] = useState(""),
    [selected, setSelected] = useState<LegalDetail | null>(initialDetail),
    [creating, setCreating] = useState(false),
    [editing, setEditing] = useState(false),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [tab, setTab] = useState("Overview"),
    [tool, setTool] = useState<"reports" | "knowledge" | "thailand" | null>(null),
    [inspection, setInspection] = useState<unknown>(null),
    [proposals, setProposals] = useState<Record<string, unknown>[]>([]);
  const [importData, setImportData] = useState<{
      rows: unknown[];
      errors: { row: number; message: string }[];
      digest: string;
      count: number;
    } | null>(null),
    [saved, setSaved] = useState<
      {
        name: string;
        query: { kind: LegalKind; search: string; state: string };
      }[]
    >([]);
  const api = `/api/v1${base}/legal`,
    can = (p: string) => data.permissions.includes(`legal.${p}`);
  const call = useCallback(
    async (path: string, body?: unknown) => {
      const r = await workFetch(
        api + path,
        body
          ? {
              method: "POST",
              headers: {
                "Content-Type": "application/json",
                "Idempotency-Key": crypto.randomUUID(),
              },
              body: JSON.stringify(body),
            }
          : { cache: "no-store" },
      );
      const j = await r.json();
      if (!r.ok) throw new Error(j.error?.message ?? "Request failed");
      return j.data;
    },
    [api],
  );
  async function load(k = kind, page = 1, q = search, status = state) {
    setData(
      await call(
        `?kind=${k}&page=${page}&search=${encodeURIComponent(q)}&state=${status}`,
      ),
    );
  }
  async function open(id: string) {
    setBusy(true);
    setError("");
    try {
      setSelected(await call(`/${id}`));
      setCreating(false);
      setEditing(false);
      setTab("Overview");
    } catch (e) {
      setError(String((e as Error).message));
    } finally {
      setBusy(false);
    }
  }
  async function command(body: Record<string, unknown>) {
    setBusy(true);
    setError("");
    try {
      const result = await call("", {
        ...(selected
          ? { id: selected.item.id, expectedVersion: selected.item.version }
          : {}),
        ...body,
      });
      await load();
      if (body.action === "create" || body.action === "revision.create") {
        setCreating(false);
        setSelected(await call(`/${result.id}`));
      } else if (selected) setSelected(await call(`/${selected.item.id}`));
      setEditing(false);
      return true;
    } catch (e) {
      setError((e as Error).message);
      return false;
    } finally {
      setBusy(false);
    }
  }
  async function inspect(id: string, action: string) {
    setBusy(true);
    setError("");
    try {
      setInspection(await call(`/documents/${id}/${action}`, {}));
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  }
  async function upload(file: File) {
    if (!selected) return;
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(`${api}/upload/${selected.item.id}`, {
        method: "POST",
        headers: {
          "Content-Type": file.type,
          "Idempotency-Key": crypto.randomUUID(),
          "X-Record-Version": String(selected.item.version),
          "X-File-Name": encodeURIComponent(file.name),
        },
        body: file,
      });
      const json = await response.json();
      if (!response.ok) throw new Error(json.error?.message ?? "Upload failed");
      setSelected(await call(`/${selected.item.id}`));
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  }
  function select(k: LegalKind) {
    setKind(k);
    setSelected(null);
    setCreating(false);
    setEditing(false);
    setError("");
    load(k).catch((e) => setError(e.message));
  }
  const total = data.counts.reduce((n, c) => n + c.count, 0),
    count = (states: string[]) =>
      data.counts
        .filter((c) => states.includes(c.state))
        .reduce((n, c) => n + c.count, 0);
  return (
    <div className={s.workspace}>
      <header className={s.header}>
        <div>
          <span className={s.eyebrow}><Scale size={15} aria-hidden="true" /> Legal workspace</span>
          <h1>Legal</h1>
          <p>Commitments, decisions and evidence in one place.</p>
        </div>
        {can("create") && data.enabled && (
          <button
            className={primary}
            onClick={() => {
              setCreating(true);
              setSelected(null);
            }}
          >
            <Plus size={16} aria-hidden="true" /> New {kind}
          </button>
        )}
      </header>
      {error && (
        <div role="alert" className={s.error}>
          {error}
        </div>
      )}
      {!data.enabled && (
        <div className={s.notice}>
          Legal is currently read-only for this Company.{" "}
          {can("manage") && (
            <button
              disabled={busy}
              className={button}
              onClick={() =>
                command({ action: "settings", enabled: true, aiEnabled: false })
              }
            >
              Enable Legal
            </button>
          )}
        </div>
      )}
      <div className={s.metrics}>
        {[
          ["Accessible records", total],
          ["In review", count(["review", "approval"])],
          ["Signing", count(["signing"])],
          ["Active commitments", count(["active"])],
        ].map(([label, n]) => (
          <div className={s.metric} key={label}>
            <span className={s.small}>{label}</span>
            <strong>{n}</strong>
          </div>
        ))}
      </div>
      <div className={s.toolBar} aria-label="Legal tools">
        <span className={s.toolLabel}>Workspace tools</span>
        {([
          ["reports", "Reports", BarChart3],
          ["knowledge", "Knowledge search", BookOpen],
          ["thailand", "ชุดกฎประเทศไทย", Globe2],
        ] as const).filter(([id]) => id !== "thailand" || (can("create") && data.enabled)).map(([id, label, Icon]) => (
          <button key={id} className={s.toolButton} aria-expanded={tool === id} aria-controls="legal-tool-panel" onClick={() => setTool(tool === id ? null : id)}>
            <Icon size={16} aria-hidden="true" />{label}
          </button>
        ))}
      </div>
      {tool && <section id="legal-tool-panel" className={s.toolPanel} aria-label={tool === "reports" ? "Legal reports" : tool === "knowledge" ? "Knowledge search" : "ชุดกฎประเทศไทย"}>
        <div className={s.panelHeading}><div><span className={s.eyebrow}>Workspace tools</span><h2>{tool === "reports" ? "Reports & insights" : tool === "knowledge" ? "Search legal knowledge" : "ชุดเริ่มต้นประเทศไทย"}</h2></div>
          <button className={s.iconButton} aria-label="Close workspace tool" onClick={() => setTool(null)}><X size={18} /></button>
        </div>
        {tool === "reports" && <LegalReports call={call} open={open} />}
        {tool === "knowledge" && <LegalKnowledge call={call} open={open} />}
        {tool === "thailand" && <LegalThailand people={data.people} call={call} refresh={() => load()} />}
      </section>}
      <div className={s.layout}>
        <nav className={s.nav} aria-label="Legal registers">
          <span className={s.navLabel}>REGISTERS</span>
          {kinds.map((k) => (
            <button key={k} aria-current={k === kind} onClick={() => select(k)}>
              <span>{labels[k]}</span>
            </button>
          ))}
        </nav>
        <section className={s.panel}>
          {creating || editing ? (
            <ItemForm
              kind={selected?.item.kind ?? kind}
              detail={editing ? selected : null}
              data={data}
              busy={busy}
              onCancel={() => {
                setCreating(false);
                setEditing(false);
              }}
              onSave={(item) =>
                command({ action: editing ? "update" : "create", item })
              }
            />
          ) : selected ? (
            <div className={s.detail}>
              <div className={s.actions}>
                <button className={button} onClick={() => setSelected(null)}>
                  ← All records
                </button>
                <span className={s.badge}>{selected.item.state}</span>
                <span className={s.small}>
                  Version {selected.item.version} ·{" "}
                  {selected.item.classification}
                </span>
              </div>
              <h2>{selected.item.title}</h2>
              <div className={s.small}>Record ID: {selected.item.id}</div>
              <OperationSummary operation={{status:selected.item.state,responsible:data.people.find(person=>person.id===selected.item.owner_id)?.name??"Assigned Legal owner",currentStep:selected.item.state.replaceAll("_"," "),nextAction:allowedTransitions(selected.item.kind,selected.item.state).length?`Review ${selected.item.kind} and continue the current workflow`:"Review the recorded history",blockedReason:null,dueAt:null,recordHref:null,actionKind:selected.item.state==="approval"?"approval":"work"}}>{selected.item.due_date&&<p className="muted">Due {selected.item.due_date}</p>}</OperationSummary>
              <div className={s.tabs}>
                {[
                  "Overview",
                  "Tasks & controls",
                  "Documents",
                  "Workflow",
                  "Access",
                  "Holds",
                  "Retention",
                  "AI",
                  "Activity",
                ].map((t) => (
                  <button
                    className={t === tab ? primary : button}
                    key={t}
                    onClick={() => setTab(t)}
                  >
                    {t}
                  </button>
                ))}
              </div>
              {tab === "Tasks & controls" && (
                <>
                  <LegalDeadlines />
                  <LegalFeatures
                    detail={selected}
                    people={data.people}
                    call={call}
                    command={command}
                    busy={busy}
                  />
                </>
              )}
              {tab === "Retention" && (
                <LegalRetention
                  detail={selected}
                  call={call}
                  command={command}
                  busy={busy}
                />
              )}
              {tab === "Overview" && (
                <>
                  <LegalGovernance
                    base={base}
                    key={selected.item.id}
                    detail={selected}
                    call={call}
                    command={command}
                    busy={busy}
                    manage={can("manage")}
                  />
                  <div className={s.body}>
                    {selected.item.description || "No description yet."}
                  </div>
                  <div className={s.grid}>
                    {fields[selected.item.kind].map((f) => (
                      <div key={f.key}>
                        <span className={s.small}>{f.label}</span>
                        <div className={s.body}>
                          {String(selected.item.data[f.key] ?? "—")}
                        </div>
                      </div>
                    ))}
                  </div>
                  <div className={s.small}>
                    {selected.item.jurisdiction} · {selected.item.currency} ·{" "}
                    {selected.item.timezone} · Due{" "}
                    {selected.item.due_date ?? "not set"}
                  </div>
                  {can("edit") && data.enabled && (
                    <button className={button} onClick={() => setEditing(true)}>
                      Edit record
                    </button>
                  )}
                  {selected.item.kind === "counsel" && (
                    <>
                      <h3>Spending handoff</h3>
                      <p className={s.small}>
                        Only the summary you enter is copied into a draft
                        purchase request. Confidential case documents stay in
                        Legal. Submit and approve the request through the
                        existing spending workflow.
                      </p>
                      <ActionForm
                        busy={busy}
                        label="Prepare purchase request"
                        fields={[
                          {
                            name: "summary",
                            label: "Summary permitted for Finance",
                            textarea: true,
                          },
                          {
                            name: "estimatedCost",
                            label: `Estimated cost (${selected.item.currency})`,
                          },
                        ]}
                        onSubmit={(v) =>
                          command({ action: "finance.prepare", ...v })
                        }
                      />
                    </>
                  )}
                  <h3>Linked records</h3>
                  {selected.links.map((l) => (
                    <button
                      key={`${l.target_id}-${l.relation}`}
                      className={button}
                      onClick={() => open(l.target_id)}
                    >
                      {l.relation} · {l.title}
                    </button>
                  ))}
                  <ActionForm
                    busy={busy}
                    label="Link record"
                    fields={[
                      { name: "targetId", label: "Target record ID" },
                      {
                        name: "relation",
                        label: "Relationship",
                        options: [
                          "related",
                          "evidence",
                          "amends",
                          "obligation",
                          "derived",
                        ],
                      },
                    ]}
                    onSubmit={(v) => command({ action: "link", ...v })}
                  />
                  <h3>Discussion</h3>
                  {selected.comments.map((c) => (
                    <div key={c.id} className={s.event}>
                      <span className={s.small}>
                        {c.visibility} · {c.created_by}
                      </span>
                      <p className={s.body}>{c.body}</p>
                    </div>
                  ))}
                  <ActionForm
                    busy={busy}
                    label="Add comment"
                    fields={[
                      { name: "body", label: "Comment", textarea: true },
                      {
                        name: "visibility",
                        label: "Audience",
                        options: ["internal", "external"],
                      },
                      {
                        name: "documentId",
                        label: "Document version",
                        optional: true,
                        choices: [
                          { value: "", label: "Record (internal only)" },
                          ...selected.documents.map((d) => ({
                            value: d.id,
                            label: `${d.name} · v${d.version}`,
                          })),
                        ],
                      },
                      {
                        name: "section",
                        label: "Clause / section (optional)",
                        optional: true,
                      },
                    ]}
                    onSubmit={(v) => command({ action: "comment", ...v })}
                  />
                </>
              )}
              {tab === "Documents" && (
                <>
                  {selected.item.state === "draft" && (
                    <ActionForm
                      busy={busy}
                      label="Preserve existing ERPX attachment"
                      fields={[
                        {
                          name: "attachmentId",
                          label:
                            "Attachment ID from an accessible request, expense, payment, leave or task",
                        },
                      ]}
                      onSubmit={(v) =>
                        command({ action: "source.attach", ...v })
                      }
                    />
                  )}
                  {selected.item.state === "draft" && (
                    <TemplateComposer
                      call={call}
                      busy={busy}
                      onApply={(templateId, values) =>
                        command({
                          action: "template.apply",
                          templateId,
                          values,
                        })
                      }
                    />
                  )}
                  <LegalEditor
                    key={selected.item.id}
                    detail={selected}
                    command={command}
                    busy={busy}
                  />
                  {inspection !== null && (
                    <div className={s.notice}>
                      <strong>Document inspection</strong>
                      <button
                        className={button}
                        disabled={busy}
                        onClick={async () => {
                          try {
                            const j = inspection as { jobId?: string };
                            if (j.jobId)
                              setInspection(
                                await call(`/processing/${j.jobId}`),
                              );
                          } catch (e) {
                            setError((e as Error).message);
                          }
                        }}
                      >
                        Refresh processing result
                      </button>
                      {selected.item.state === "draft" &&
                        typeof (inspection as { result?: { text?: unknown } })
                          ?.result?.text === "string" && (
                          <button
                            className={button}
                            disabled={busy}
                            onClick={() => {
                              const result = inspection as {
                                result: { text: string };
                              };
                              void command({
                                action: "document",
                                name: "Reviewed imported text",
                                body: result.result.text,
                              });
                            }}
                          >
                            Accept extracted text as a new draft
                          </button>
                        )}
                      <pre className={s.body}>
                        {JSON.stringify(inspection, null, 2)}
                      </pre>
                    </div>
                  )}
                  {selected.item.state === "draft" && (
                    <label className={s.field}>
                      Upload PDF, DOCX or image (up to 10 MB)
                      <input
                        type="file"
                        disabled={busy}
                        accept=".pdf,.docx,.png,.jpg,.jpeg"
                        onChange={(e) => {
                          const file = e.target.files?.[0];
                          if (file) void upload(file);
                        }}
                      />
                    </label>
                  )}
                  <p className={s.small}>
                    Each save creates an immutable version. Return the record to
                    draft to add content.
                  </p>
                  {selected.submissions.length > 0 && (
                    <>
                      <h3>External submissions</h3>
                      {selected.submissions.map((u) => (
                        <div className={s.event} key={u.id}>
                          <a href={`${api}/submissions/${u.id}`}>{u.name}</a>
                          <div className={s.small}>
                            {u.created_by} ·{" "}
                            {new Date(u.created_at).toLocaleString()} · SHA-256{" "}
                            {u.checksum}
                          </div>
                          <p className={s.small}>
                            Review this submission before uploading it as a new
                            internal draft version.
                          </p>
                        </div>
                      ))}
                    </>
                  )}
                  {selected.documents.map((d) => (
                    <details key={d.id} className={s.event}>
                      <summary>
                        {d.name} · v{d.version} {d.locked ? "· Locked" : ""}
                      </summary>
                      <div className={s.small}>
                        ID: {d.id}
                        <br />
                        SHA-256: {d.checksum}
                      </div>
                      <pre className={s.body}>
                        {d.body || "Uploaded document"}
                      </pre>
                      <div className={s.actions}>
                        <a className={button} href={`${api}/documents/${d.id}`}>
                          Download original
                        </a>
                        {d.body && (
                          <>
                            <a
                              className={button}
                              href={`${api}/documents/${d.id}?format=docx`}
                            >
                              DOCX
                            </a>
                            <a
                              className={button}
                              href={`${api}/documents/${d.id}?format=pdf`}
                            >
                              PDF
                            </a>
                          </>
                        )}
                        <button
                          className={button}
                          disabled={busy}
                          onClick={() => inspect(d.id, "ocr")}
                        >
                          Extract text
                        </button>
                        {d.media_type === "application/pdf" && (
                          <button
                            className={button}
                            disabled={busy}
                            onClick={() => inspect(d.id, "verify")}
                          >
                            Inspect signatures
                          </button>
                        )}
                      </div>
                    </details>
                  ))}
                  {selected.item.state === "draft" && (
                    <ActionForm
                      busy={busy}
                      label="Save document version"
                      fields={[
                        { name: "name", label: "Document name" },
                        {
                          name: "body",
                          label: "Document / clause content",
                          textarea: true,
                        },
                      ]}
                      onSubmit={(v) => command({ action: "document", ...v })}
                    />
                  )}
                </>
              )}
              {tab === "Workflow" && (
                <>
                  {["contract", "rule", "template"].includes(
                    selected.item.kind,
                  ) && (
                    <ActionForm
                      busy={busy}
                      label="Create amendment / renewal / replacement draft"
                      fields={[
                        { name: "title", label: "New draft title" },
                        {
                          name: "reason",
                          label: "Reason and changes requested",
                          textarea: true,
                        },
                      ]}
                      onSubmit={(v) =>
                        command({ action: "revision.create", ...v })
                      }
                    />
                  )}
                  <ActionForm
                    busy={busy}
                    label="Change state"
                    fields={[
                      {
                        name: "state",
                        label: "Next state",
                        options: allowedTransitions(
                          selected.item.kind,
                          selected.item.state,
                        ),
                      },
                      { name: "reason", label: "Reason", textarea: true },
                    ]}
                    onSubmit={(v) => command({ action: "transition", ...v })}
                  />
                  {selected.item.kind === "rule" && (
                    <button
                      className={button}
                      onClick={async () => {
                        try {
                          setInspection(
                            await call(`/rule-impact/${selected.item.id}`),
                          );
                          setTab("Documents");
                        } catch (e) {
                          setError((e as Error).message);
                        }
                      }}
                    >
                      Preview affected records
                    </button>
                  )}
                  {selected.item.kind === "contract" && (
                    <LegalApproval
                      detail={selected}
                      call={call}
                      command={command}
                      busy={busy}
                    />
                  )}
                  {selected.item.state === "approved" &&
                    selected.item.kind === "contract" && (
                      <ActionForm
                        busy={busy}
                        label="Send for signature"
                        fields={[
                          {
                            name: "consent",
                            label: "Exact consent text shown to signers",
                            textarea: true,
                          },
                        ]}
                        onSubmit={(v) =>
                          command({
                            action: "signing.send",
                            ...v,
                            expiresAt: Date.now() + 30 * 86400000,
                          })
                        }
                      />
                    )}
                  {selected.envelopes.map((e) => (
                    <div className={s.notice} key={e.id}>
                      <strong>Signing · {e.state}</strong>
                      <p className={s.body}>{e.consent}</p>
                      <p>
                        Expires {new Date(e.expires_at).toLocaleDateString()}
                      </p>
                      {e.signers.map((p) => (
                        <div key={p.user_id}>
                          {p.ordinal + 1}. {p.user_id} · {p.state}
                        </div>
                      ))}
                      {e.state === "sent" &&
                        e.signers.some(
                          (p) =>
                            p.user_id === data.userId && p.state === "pending",
                        ) && (
                          <>
                            <p>
                              Verify MFA in <a href="/my-access">My access</a>,
                              then review the locked document before signing.
                            </p>
                            <button
                              className={button}
                              disabled={busy}
                              onClick={() =>
                                command({
                                  action: "signing.view",
                                  envelopeId: e.id,
                                })
                              }
                            >
                              Record document review
                            </button>
                            <ActionForm
                              busy={busy}
                              label="Accept and sign"
                              fields={[
                                {
                                  name: "consent",
                                  label: "Enter the exact consent text",
                                  textarea: true,
                                },
                              ]}
                              onSubmit={(v) =>
                                command({
                                  action: "signing.sign",
                                  envelopeId: e.id,
                                  ...v,
                                })
                              }
                            />
                            <button
                              className={button}
                              disabled={busy}
                              onClick={() =>
                                command({
                                  action: "signing.decline",
                                  envelopeId: e.id,
                                })
                              }
                            >
                              Decline
                            </button>
                          </>
                        )}
                    </div>
                  ))}
                </>
              )}
              {tab === "Access" && (
                <>
                  <h3>Internal membership</h3>
                  {selected.members.map((m) => (
                    <div key={m.user_id} className={s.event}>
                      {m.user_id} · {m.access}
                    </div>
                  ))}
                  <ActionForm
                    busy={busy}
                    label="Update membership"
                    fields={[
                      {
                        name: "userId",
                        label: "Company member",
                        choices: data.people.map((p) => ({
                          value: p.id,
                          label: p.name,
                        })),
                      },
                      {
                        name: "access",
                        label: "Access",
                        options: ["read", "edit", "remove"],
                      },
                    ]}
                    onSubmit={(v) => command({ action: "member", ...v })}
                  />
                  <h3>External shares</h3>
                  <a
                    href={`/legal-portal/${base.split("/")[2]}`}
                    target="_blank"
                    rel="noreferrer"
                  >
                    Open external portal
                  </a>
                  {selected.shares.map((share) => (
                    <div className={s.event} key={share.id}>
                      {share.email} ·{" "}
                      {share.revoked_at
                        ? "Revoked"
                        : new Date(share.expires_at).toLocaleDateString()}
                      <div className={s.small}>
                        Accepted user:{" "}
                        {share.user_id ?? "Invitation not accepted"}
                      </div>
                      {!share.revoked_at && (
                        <button
                          className={button}
                          disabled={busy}
                          onClick={() =>
                            command({
                              action: "share.revoke",
                              shareId: share.id,
                            })
                          }
                        >
                          Revoke share
                        </button>
                      )}
                    </div>
                  ))}
                  <h3>Share a document version externally</h3>
                  <ActionForm
                    busy={busy}
                    label="Create external access"
                    fields={[
                      {
                        name: "email",
                        label: "Verified account email",
                        type: "email",
                      },
                      {
                        name: "documentId",
                        label: "Document version",
                        choices: selected.documents.map((d) => ({
                          value: d.id,
                          label: `${d.name} · v${d.version}`,
                        })),
                      },
                    ]}
                    onSubmit={(v) =>
                      command({
                        action: "share",
                        ...v,
                        expiresAt: Date.now() + 30 * 86400000,
                      })
                    }
                  />
                  <p className={s.small}>
                    Access expires after 30 days. The recipient signs in with
                    the specified verified email.
                  </p>
                </>
              )}
              {tab === "Holds" && (
                <>
                  <p>
                    A preservation hold protects linked evidence. Release
                    requires an independent reviewer.
                  </p>
                  {selected.holds.map((h) => (
                    <div className={s.hold} key={h.id}>
                      <strong>{h.state}</strong>
                      <p>{h.reason}</p>
                      {h.acknowledged_at ? (
                        <p className={s.small}>
                          Acknowledged{" "}
                          {new Date(h.acknowledged_at).toLocaleString()}
                        </p>
                      ) : (
                        <ActionForm
                          busy={busy}
                          label="Acknowledge preservation notice"
                          fields={[
                            {
                              name: "reason",
                              label:
                                "Confirm the preservation steps you have taken",
                              textarea: true,
                            },
                          ]}
                          onSubmit={(v) =>
                            command({
                              action: "hold.acknowledge",
                              holdId: h.id,
                              ...v,
                            })
                          }
                        />
                      )}

                      {!h.direct && (
                        <p className={s.small}>
                          Release is managed on the source record.
                        </p>
                      )}
                      {h.direct && (
                        <ActionForm
                          busy={busy}
                          label={
                            h.state === "active"
                              ? "Request release"
                              : "Approve release"
                          }
                          fields={[{ name: "reason", label: "Release reason" }]}
                          onSubmit={(v) =>
                            command({
                              action:
                                h.state === "active"
                                  ? "hold.request-release"
                                  : "hold.release",
                              holdId: h.id,
                              ...v,
                            })
                          }
                        />
                      )}
                    </div>
                  ))}
                  <ActionForm
                    busy={busy}
                    label="Place legal hold"
                    fields={[
                      {
                        name: "reason",
                        label: "Preservation reason",
                        textarea: true,
                      },
                    ]}
                    onSubmit={(v) => command({ action: "hold.create", ...v })}
                  />
                </>
              )}
              {tab === "AI" && (
                <>
                  <p>
                    AI proposals use this record and its latest document
                    versions. Review every statement and citation before copying
                    it into a draft.
                  </p>
                  {can("manage") && (
                    <button
                      className={button}
                      disabled={busy}
                      onClick={() =>
                        command({
                          action: "settings",
                          enabled: true,
                          aiEnabled: true,
                        })
                      }
                    >
                      Enable Legal AI for this Company
                    </button>
                  )}
                  <ActionForm
                    busy={busy}
                    label="Request AI proposal"
                    fields={[
                      {
                        name: "question",
                        label: "Draft, summarize, compare or identify risks",
                        textarea: true,
                      },
                    ]}
                    onSubmit={(v) => command({ action: "ai.generate", ...v })}
                  />
                  <button
                    className={button}
                    onClick={() =>
                      call(`/ai/${selected.item.id}`)
                        .then(setProposals)
                        .catch((e) => setError(e.message))
                    }
                  >
                    Refresh proposals
                  </button>
                  {proposals.map((p) => (
                    <div className={s.notice} key={String(p.id)}>
                      <strong>{String(p.state)}</strong>
                      <p>{String(p.question)}</p>
                      {p.error ? <p>{String(p.error)}</p> : null}
                      <pre className={s.body}>
                        {JSON.stringify(p.proposal, null, 2)}
                      </pre>
                      {p.state === "proposed" &&
                        selected.item.state === "draft" &&
                        can("edit") && (
                          <ActionForm
                            busy={busy}
                            label="Accept reviewed draft as a new version"
                            fields={[
                              { name: "name", label: "Reviewed document name" },
                              {
                                name: "body",
                                label:
                                  "Review the content and remove unsupported portions",
                                textarea: true,
                                initial: String(
                                  (p.proposal as { draft?: string } | null)
                                    ?.draft ?? "",
                                ),
                              },
                            ]}
                            onSubmit={(v) =>
                              command({
                                action: "ai.apply",
                                proposalId: p.id,
                                ...v,
                              })
                            }
                          />
                        )}
                    </div>
                  ))}
                </>
              )}
              {tab === "Activity" && can("export") && (
                <a
                  className={button}
                  href={`${api}/evidence/${selected.item.id}`}
                >
                  Download evidence and access history
                </a>
              )}
              {tab === "Activity" &&
                selected.events.map((e) => (
                  <div className={s.event} key={e.id}>
                    <strong>{e.action}</strong>
                    <p>{e.note}</p>
                    {e.snapshot && (
                      <details>
                        <summary>Record at this event</summary>
                        <pre className={s.body}>
                          {JSON.stringify(JSON.parse(e.snapshot), null, 2)}
                        </pre>
                      </details>
                    )}
                    <span className={s.small}>
                      {e.actor_id} · {new Date(e.created_at).toLocaleString()} ·
                      v{e.version}
                    </span>
                  </div>
                ))}
            </div>
          ) : (
            <>
              <div className={s.registerHeading}><h2>{labels[kind]}</h2><span className={s.small}>Manage records, deadlines and decisions</span></div>
              <div className={s.actions}>
                {can("export") && (
                  <a className={button} href={`${api}/export/${kind}`}>
                    Export register
                  </a>
                )}
                <button
                  className={button}
                  onClick={() =>
                    call("/views")
                      .then(setSaved)
                      .catch((e) => setError(e.message))
                  }
                >
                  Saved views
                </button>
                <button
                  className={button}
                  disabled={busy}
                  onClick={() =>
                    command({
                      action: "view.save",
                      name: `${labels[kind]} · ${search || state || "All"}`,
                      query: { kind, search, state },
                    })
                  }
                >
                  Save current view
                </button>
                {saved.map((v) => (
                  <button
                    key={v.name}
                    className={button}
                    onClick={() => {
                      setKind(v.query.kind);
                      setSearch(v.query.search);
                      setState(v.query.state);
                      load(
                        v.query.kind,
                        1,
                        v.query.search,
                        v.query.state,
                      ).catch((e) => setError(e.message));
                    }}
                  >
                    {v.name}
                  </button>
                ))}
              </div>
              {can("create") && (
                <details className={s.event}>
                  <summary>Import records</summary>
                  <p className={s.small}>
                    Upload a JSON array of draft records. Preview validates
                    every row before an atomic import.
                  </p>
                  <input
                    aria-label="Import records JSON"
                    type="file"
                    accept=".json,application/json"
                    onChange={async (e) => {
                      const file = e.target.files?.[0];
                      if (!file) return;
                      try {
                        if (file.size > 500000)
                          throw new Error("Use a file below 500 KB.");
                        const rows = JSON.parse(await file.text());
                        setImportData(
                          await call("", { action: "import.preview", rows }),
                        );
                      } catch (e) {
                        setError((e as Error).message);
                      }
                    }}
                  />
                  {importData && (
                    <>
                      <p>
                        {importData.count} rows · {importData.errors.length}{" "}
                        errors
                      </p>
                      {importData.errors.map((e) => (
                        <p key={e.row}>
                          Row {e.row}: {e.message}
                        </p>
                      ))}
                      <button
                        className={button}
                        disabled={busy || importData.errors.length > 0}
                        onClick={async () => {
                          if (
                            await command({
                              action: "import.commit",
                              rows: importData.rows,
                              digest: importData.digest,
                            })
                          )
                            setImportData(null);
                        }}
                      >
                        Import validated records
                      </button>
                    </>
                  )}
                </details>
              )}

              <form
                className={s.toolbar}
                onSubmit={(e) => {
                  e.preventDefault();
                  load().catch((e) => setError(e.message));
                }}
              >
                <input
                  aria-label="Search legal records"
                  placeholder="Search records…"
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                />
                <select
                  aria-label="State filter"
                  value={state}
                  onChange={(e) => setState(e.target.value)}
                >
                  <option value="">All states</option>
                  {[
                    "draft",
                    "review",
                    "approval",
                    "approved",
                    "signing",
                    "executed",
                    "active",
                    "completed",
                    "closed",
                    "archived",
                  ].map((v) => (
                    <option key={v}>{v}</option>
                  ))}
                </select>
                <button className={button}>Search</button>
              </form>
              {data.items.length ? (
                <table className={s.list}>
                  <thead>
                    <tr>
                      <th>Record</th>
                      <th>Status</th>
                      <th>Due</th>
                      <th>Classification</th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.items.map((i) => (
                      <tr key={i.id}>
                        <td>
                          <button
                            className={s.link}
                            disabled={busy}
                            onClick={() => open(i.id)}
                          >
                            {i.title}
                          </button>
                          <div className={s.small}>{i.jurisdiction}</div>
                        </td>
                        <td>
                          <span className={s.badge}>{i.state}</span>
                        </td>
                        <td>{i.due_date ?? "—"}</td>
                        <td>{i.classification}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              ) : (
                <div className={s.empty}>
                  <h3>No records yet</h3>
                  <p>Create a {kind} to start tracking work and evidence.</p>
                </div>
              )}
              <div className={s.footer}>
                <span className={s.small}>
                  {data.total} records · Page {data.page}
                </span>
                <div className={s.actions}>
                  <button
                    className={button}
                    disabled={data.page <= 1}
                    onClick={() =>
                      load(kind, data.page - 1).catch((e) =>
                        setError(e.message),
                      )
                    }
                  >
                    Previous
                  </button>
                  <button
                    className={button}
                    disabled={data.page * 30 >= data.total}
                    onClick={() =>
                      load(kind, data.page + 1).catch((e) =>
                        setError(e.message),
                      )
                    }
                  >
                    Next
                  </button>
                </div>
              </div>
            </>
          )}
        </section>
      </div>
    </div>
  );
}
function ItemForm({
  kind,
  detail,
  data,
  busy,
  onSave,
  onCancel,
}: {
  kind: LegalKind;
  detail: LegalDetail | null;
  data: LegalData;
  busy: boolean;
  onSave: (x: ItemInput) => Promise<boolean>;
  onCancel: () => void;
}) {
  const item = detail?.item;
  async function save(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const f = new FormData(e.currentTarget),
      values: ItemInput["data"] = {};
    for (const field of fields[kind]) {
      const v = String(f.get(`field:${field.key}`) ?? "");
      if (v) values[field.key] = field.type === "number" ? Number(v) : v;
    }
    await onSave({
      kind,
      title: String(f.get("title")),
      description: String(f.get("description")),
      classification: String(
        f.get("classification"),
      ) as ItemInput["classification"],
      country: String(f.get("country")),
      jurisdiction: String(f.get("jurisdiction")),
      language: String(f.get("language")),
      currency: String(f.get("currency")),
      timezone: String(f.get("timezone")),
      priority: String(f.get("priority")) as ItemInput["priority"],
      dueDate: String(f.get("dueDate")) || null,
      ownerId: String(f.get("ownerId")),
      data: values,
    });
  }
  return (
    <form onSubmit={save}>
      <h2>
        {item ? "Edit" : "New"} {kind}
      </h2>
      <div className={s.grid}>
        <label className={`${s.field} ${s.wide}`}>
          Title
          <input
            name="title"
            required
            minLength={2}
            maxLength={240}
            defaultValue={item?.title}
          />
        </label>
        <label className={`${s.field} ${s.wide}`}>
          Description
          <textarea name="description" defaultValue={item?.description} />
        </label>
        <label className={s.field}>
          Owner
          <select name="ownerId" defaultValue={item?.owner_id ?? data.userId}>
            {data.people.map((p) => (
              <option value={p.id} key={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </label>
        <label className={s.field}>
          Classification
          <select
            name="classification"
            defaultValue={item?.classification ?? "confidential"}
          >
            {["internal", "confidential", "restricted"].map((v) => (
              <option key={v}>{v}</option>
            ))}
          </select>
        </label>
        {[
          {
            key: "country",
            label: "Country code",
            value: item?.country ?? "TH",
          },
          {
            key: "jurisdiction",
            label: "Jurisdiction",
            value: item?.jurisdiction ?? "Thailand",
          },
          { key: "language", label: "Language", value: item?.language ?? "th" },
          {
            key: "currency",
            label: "Currency",
            value: item?.currency ?? "THB",
          },
          {
            key: "timezone",
            label: "Timezone",
            value: item?.timezone ?? "Asia/Bangkok",
          },
        ].map((f) => (
          <label key={f.key} className={s.field}>
            {f.label}
            <input name={f.key} required defaultValue={f.value} />
          </label>
        ))}
        <label className={s.field}>
          Due date
          <input
            type="date"
            name="dueDate"
            defaultValue={item?.due_date ?? ""}
          />
        </label>
        <label className={s.field}>
          Priority
          <select name="priority" defaultValue={item?.priority ?? "normal"}>
            {["normal", "high", "urgent"].map((v) => (
              <option key={v}>{v}</option>
            ))}
          </select>
        </label>
        {fields[kind].map((f) => (
          <label
            className={`${s.field} ${f.type === "textarea" ? s.wide : ""}`}
            key={f.key}
          >
            {f.label}
            {f.type === "textarea" ? (
              <textarea
                name={`field:${f.key}`}
                required={f.required}
                defaultValue={item?.data[f.key] ?? ""}
              />
            ) : (
              <input
                name={`field:${f.key}`}
                type={f.type ?? "text"}
                min={f.type === "number" ? 0 : undefined}
                required={f.required}
                defaultValue={item?.data[f.key] ?? ""}
              />
            )}
          </label>
        ))}
      </div>
      <div className={s.footer}>
        <button type="button" className={button} onClick={onCancel}>
          Cancel
        </button>
        <button className={primary} disabled={busy}>
          {busy ? "Saving…" : "Save record"}
        </button>
      </div>
    </form>
  );
}
function ActionForm({
  fields: spec,
  label,
  onSubmit,
  busy,
}: {
  fields: {
    name: string;
    label: string;
    options?: string[];
    choices?: { value: string; label: string }[];
    textarea?: boolean;
    type?: string;
    optional?: boolean;
    initial?: string;
  }[];
  label: string;
  onSubmit: (v: Record<string, string>) => Promise<boolean>;
  busy: boolean;
}) {
  return (
    <form
      className={s.detail}
      onSubmit={async (e) => {
        e.preventDefault();
        const form = e.currentTarget,
          v = Object.fromEntries(new FormData(form)) as Record<string, string>;
        if (await onSubmit(v)) form.reset();
      }}
    >
      {spec.map((f) => (
        <label className={s.field} key={f.name}>
          {f.label}
          {f.choices ? (
            <select name={f.name} required={!f.optional}>
              {f.choices.map((c) => (
                <option key={c.value} value={c.value}>
                  {c.label}
                </option>
              ))}
            </select>
          ) : f.options ? (
            <select name={f.name} required>
              {f.options.map((v) => (
                <option key={v}>{v}</option>
              ))}
            </select>
          ) : f.textarea ? (
            <textarea
              name={f.name}
              required={!f.optional}
              defaultValue={f.initial}
            />
          ) : (
            <input
              name={f.name}
              type={f.type ?? "text"}
              required={!f.optional}
            />
          )}
        </label>
      ))}
      <div>
        <button
          className={button}
          disabled={
            busy ||
            spec.some((f) => f.options?.length === 0 || f.choices?.length === 0)
          }
        >
          {label}
        </button>
      </div>
    </form>
  );
}

function TemplateComposer({
  call,
  busy,
  onApply,
}: {
  call: (path: string, body?: unknown) => Promise<unknown>;
  busy: boolean;
  onApply: (id: string, values: Record<string, string>) => Promise<boolean>;
}) {
  const [templates, setTemplates] = useState<LegalItem[]>([]),
    [chosen, setChosen] = useState<LegalItem | null>(null),
    [error, setError] = useState("");
  const variables = chosen
    ? [
        ...new Set(
          Array.from(
            String(chosen.data.body ?? "").matchAll(
              /\{\{([a-zA-Z][a-zA-Z0-9_]*)\}\}/g,
            ),
            (m) => m[1],
          ),
        ),
      ]
    : [];
  return (
    <details className={s.event}>
      <summary>Prepare from an approved template</summary>
      {error && <p role="alert">{error}</p>}
      <button
        className={button}
        type="button"
        onClick={async () => {
          try {
            const [a, b] = await Promise.all([
              call("?kind=template&state=approved"),
              call("?kind=template&state=active"),
            ]);
            setTemplates([
              ...(a as LegalData).items,
              ...(b as LegalData).items,
            ]);
          } catch (e) {
            setError((e as Error).message);
          }
        }}
      >
        Find approved templates
      </button>
      {templates.map((t) => (
        <button
          className={button}
          key={t.id}
          type="button"
          onClick={async () => {
            try {
              setChosen(((await call(`/${t.id}`)) as LegalDetail).item);
            } catch (e) {
              setError((e as Error).message);
            }
          }}
        >
          {t.title}
        </button>
      ))}
      {chosen && (
        <form
          className={s.detail}
          onSubmit={async (e) => {
            e.preventDefault();
            const values = Object.fromEntries(
              new FormData(e.currentTarget),
            ) as Record<string, string>;
            if (await onApply(chosen.id, values)) setChosen(null);
          }}
        >
          <h4>
            {chosen.title} · v{chosen.version}
          </h4>
          {variables.map((v) => (
            <label key={v} className={s.field}>
              {v}
              <input name={v} required />
            </label>
          ))}
          <pre className={s.body}>{String(chosen.data.body ?? "")}</pre>
          <button className={button} disabled={busy}>
            Create document version
          </button>
        </form>
      )}
    </details>
  );
}
