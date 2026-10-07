"use client";
import { DraftFormDrawer } from "@/components/ui/draft-form-drawer";
import { useRouter } from "next/navigation";
import { useEmbeddedRecord } from "@/components/records/record-context";
import { recordDrawerHref } from "@/lib/record-drawer";
import { RecordLink } from "@/components/records/record-link";
import { DialogClose } from "@/components/ui/dialog";
import { FormGrid, FormLayout } from "@/components/ui/patterns";
import { Card } from "@/components/ui/foundation";

import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, Table, TextArea, TextInput, LocalTabs, TabsList, TabsTrigger, TabsPanel } from "@/components/ui/foundation";

import { TimeText } from "@/components/time/time-preference-context";
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
import { LegalEvidenceFields, LegalSnapshot } from "./legal-evidence-fields";
import { LegalExecutedCopies } from "./legal-executed-copies";
import { LegalLawCorpus } from "./legal-law-corpus";
import { LegalReleaseAcceptance } from "./legal-release-acceptance";
import { LegalWorkQueue } from "./legal-work-queue";
import { LegalAssignment } from "./legal-assignment";
import { LegalDocumentTextReview } from "./legal-document-text-review";
import { useLegalPeople } from "./legal-person-select";
import { createLegalCommandClient } from "@/lib/legal/command-client";
import { useState, useCallback, useEffect, useMemo, useRef, type FormEvent } from "react";
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
} from "@/server/modules/legal/public/rules/model";
import s from "./legal.module.css";
import { TableFrame } from "@/components/ui/foundation";
const button = `${s.button}`,
  primary = `${s.button} ${s.primary}`;
export function LegalWorkspace({
  base,
  initial,
  initialDetail = null,
  initialCreate = false,
  initialKind = "request",
}: {
  base: string;
  initial: LegalData;
  initialDetail?: LegalDetail | null;
  initialCreate?: boolean;
  initialKind?: LegalKind;
}) {
  const router = useRouter(), embedded = useEmbeddedRecord();
  const [data, setData] = useState(initial),
    [kind, setKind] = useState<LegalKind>(initialDetail?.item.kind ?? initialKind),
    [search, setSearch] = useState(""),
    [state, setState] = useState(""),
    [selected, setSelected] = useState<LegalDetail | null>(initialDetail),
    [creating, setCreating] = useState(
      initialCreate && initial.enabled && initial.permissions.includes("legal.create"),
    ),
    [editing, setEditing] = useState(false),
    [busy, setBusy] = useState(false),
    [pendingAction, setPendingAction] = useState(false),
    [error, setError] = useState(""),
    [tab, setTab] = useState("Overview"),
    [tool, setTool] = useState<"reports" | "knowledge" | "thailand" | "laws" | "release" | null>(
      null,
    ),
    [workView, setWorkView] = useState<"register" | "mine" | "deadlines">("register"),
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
  const contextKey = `erpx:legal:${base}:${JSON.stringify(initial.workContext ?? { userId: initial.userId })}`;
  const send = useMemo(() => createLegalCommandClient(`${contextKey}:command`), [contextKey]);
  useEffect(() => {
    const frame = requestAnimationFrame(() => setPendingAction(send.hasPending()));
    return () => cancelAnimationFrame(frame);
  }, [send]);
  const call = useCallback(
    async (path: string, body?: unknown) => {
      if (body !== undefined) {
        try {
          const result = await send(api + path, body);
          setPendingAction(false);
          return result;
        } catch (error) {
          setPendingAction(send.hasPending());
          throw error;
        }
      }
      const r = await workFetch(api + path, { cache: "no-store" });
      const j = await r.json();
      if (!r.ok) throw new Error(j.error?.message ?? "Request failed");
      return j.data;
    },
    [api, send],
  );
  function location(record?: string, selectedKind = kind) {
    const url = new URL(window.location.href);
    url.searchParams.delete("new");
    url.searchParams.set("kind", selectedKind);
    if (record) url.searchParams.set("record", record);
    else url.searchParams.delete("record");
    window.history.replaceState(null, "", url);
  }
  async function load(k = kind, page = 1, q = search, status = state) {
    setData(await call(`?kind=${k}&page=${page}&search=${encodeURIComponent(q)}&state=${status}`));
  }
  async function open(id: string) {
    if (!embedded || selected?.item.id !== id) { router.push(recordDrawerHref(window.location.href, { kind: "legal", id, args: { kind } }), { scroll: false }); return; }
    setBusy(true);
    setError("");
    try {
      const detail = (await call(`/${id}`)) as LegalDetail;
      setSelected(detail);
      setKind(detail.item.kind);
      setWorkView("register");
      location(id, detail.item.kind);
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
        ...(selected ? { id: selected.item.id, expectedVersion: selected.item.version } : {}),
        ...body,
      });
      if (body.action === "create" || body.action === "revision.create") {
        setCreating(false);
        location(result.id, body.action === "create" ? (body.item as ItemInput).kind : kind);
        setSelected(await call(`/${result.id}`));
      } else if (selected) setSelected(await call(`/${selected.item.id}`));
      setEditing(false);
      await load();
      return true;
    } catch (e) {
      setError((e as Error).message);
      return false;
    } finally {
      setBusy(false);
    }
  }
  async function refreshDetail() {
    if (selected) setSelected(await call(`/${selected.item.id}`));
    await load();
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
    setWorkView("register");
    location(undefined, k);
    setSelected(null);
    setCreating(false);
    setEditing(false);
    setError("");
    load(k).catch((e) => setError(e.message));
  }
  const total = data.counts.reduce((n, c) => n + c.count, 0),
    count = (states: string[]) =>
      data.counts.filter((c) => states.includes(c.state)).reduce((n, c) => n + c.count, 0);
  return (
    <div className="ui-stack">
      <header className={s.header}>
        <div>
          <span className={s.eyebrow}>
            <Scale size={15} aria-hidden="true" /> Legal workspace
          </span>
          <h1>Legal</h1>
          <p>Commitments, decisions and evidence in one place.</p>
        </div>
        {can("create") && data.enabled && (
          <Button appearance="standard"
            type="submit"
            variant="ghost"
            className={primary}
            onClick={() => {
              setCreating(true);
              setWorkView("register");
              setSelected(null);
            }}
          >
            <Plus size={16} aria-hidden="true" /> New {kind}
          </Button>
        )}
      </header>
      {error && (
        <InlineNotice tone="danger" role="alert" className={s.error}>
          {error}
        </InlineNotice>
      )}
      {pendingAction && (
        <InlineNotice tone="info" className={s.notice} role="status">
          <p>
            A previous Legal action has an uncertain result. Recover its original request before
            starting another action.
          </p>
          <Button appearance="standard"
            type="submit"
            variant="ghost"
            className={button}
            disabled={busy}
            onClick={async () => {
              setBusy(true);
              setError("");
              try {
                const recovered = await send.recover();
                setPendingAction(send.hasPending());
                if (recovered) {
                  const recordId =
                    typeof recovered.input.id === "string"
                      ? recovered.input.id
                      : ["create", "revision.create"].includes(String(recovered.input.action))
                        ? (recovered.data as { id: string }).id
                        : null;
                  if (recordId) await open(recordId);
                  await load();
                }
              } catch (error) {
                setError(error instanceof Error ? error.message : "Unable to recover the action.");
                setPendingAction(send.hasPending());
              } finally {
                setBusy(false);
              }
            }}
          >
            Recover previous Legal action
          </Button>
        </InlineNotice>
      )}
      {!data.enabled && (
        <div className={s.notice}>
          Legal is currently read-only for this Company.{" "}
          {can("manage") && (
            <Button appearance="standard"
              type="submit"
              variant="ghost"
              disabled={busy}
              className={button}
              onClick={() => command({ action: "settings", enabled: true, aiEnabled: false })}
            >
              Enable Legal
            </Button>
          )}
        </div>
      )}
      <div className={s.metrics}>
        {[
          ["Accessible records", total],
          ["In review", count(["review", "approval"])],
          ["Executed contracts", count(["executed"])],
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
        {(
          [
            ["reports", "Reports", BarChart3],
            ["knowledge", "Knowledge search", BookOpen],
            ["thailand", "Thailand starter pack", Globe2],
            ["laws", "Official law sources", BookOpen],
            ["release", "Company readiness", Scale],
          ] as const
        )
          .filter(
            ([id]) =>
              (id !== "thailand" || (can("create") && data.enabled)) &&
              (id !== "release" || can("manage")),
          )
          .map(([id, label, Icon]) => (
            <Button appearance="standard"
              type="submit"
              variant="ghost"
              key={id}
              className={s.toolButton}
              aria-expanded={tool === id}
              aria-controls="legal-tool-panel"
              onClick={() => setTool(tool === id ? null : id)}
            >
              <Icon size={16} aria-hidden="true" />
              {label}
            </Button>
          ))}
      </div>
      {tool && (
        <section
          id="legal-tool-panel"
          className={s.toolPanel}
          aria-label={
            tool === "reports"
              ? "Legal reports"
              : tool === "knowledge"
                ? "Knowledge search"
                : tool === "laws"
                  ? "Official law sources"
                  : tool === "release"
                    ? "Company readiness"
                    : "Thailand starter pack"
          }
        >
          <div className={s.panelHeading}>
            <div>
              <span className={s.eyebrow}>Workspace tools</span>
              <h2>
                {tool === "reports"
                  ? "Reports & insights"
                  : tool === "knowledge"
                    ? "Search legal knowledge"
                    : tool === "laws"
                      ? "Official law sources"
                      : tool === "release"
                        ? "Company readiness"
                        : "Thailand starter pack"}
              </h2>
            </div>
            <Button appearance="standard"
              type="submit"
              variant="ghost"
              className={s.iconButton}
              aria-label="Close workspace tool"
              onClick={() => setTool(null)}
            >
              <X size={18} />
            </Button>
          </div>
          {tool === "reports" && <LegalReports call={call} open={open} />}
          {tool === "knowledge" && <LegalKnowledge call={call} open={open} base={base} />}
          {tool === "thailand" && (
            <LegalThailand people={data.people} call={call} refresh={() => load()} />
          )}
          {tool === "laws" && (
            <LegalLawCorpus
              call={call}
              manage={can("manage")}
              canReview={can("approve")}
              userId={data.userId}
            />
          )}
          {tool === "release" && <LegalReleaseAcceptance call={call} userId={data.userId} />}
        </section>
      )}
      <div className={s.layout}>
        <nav className={s.nav} aria-label="Legal registers">
          <span className={s.navLabel}>WORKSPACE</span>
          <Button appearance="standard"
            type="submit"
            variant="ghost"
            aria-current={workView === "mine"}
            onClick={() => {
              setWorkView("mine");
              setSelected(null);
              setCreating(false);
              setEditing(false);
            }}
          >
            My legal work
          </Button>
          <Button appearance="standard"
            type="submit"
            variant="ghost"
            aria-current={workView === "deadlines"}
            onClick={() => {
              setWorkView("deadlines");
              setSelected(null);
              setCreating(false);
              setEditing(false);
            }}
          >
            Deadlines
          </Button>
          <span className={s.navLabel}>REGISTERS</span>
          {kinds.map((k) => (
            <Button appearance="standard"
              type="submit"
              variant="ghost"
              key={k}
              aria-current={workView === "register" && k === kind}
              onClick={() => select(k)}
            >
              <span>{labels[k]}</span>
            </Button>
          ))}
        </nav>
        <Card as="section" appearance="standard" className={s.panel}>
          {(creating || editing) && (
            <ItemForm
              key={`${contextKey}:${selected?.item.id ?? kind}:${editing}`}
              kind={selected?.item.kind ?? kind}
              detail={editing ? selected : null}
              data={data}
              busy={busy}
              call={call}
              draftKey={`${contextKey}:draft:${editing ? selected?.item.id : kind}:${editing ? selected?.item.version : "new"}`}
              onCancel={() => {
                setCreating(false);
                setEditing(false);
              }}
              onSave={(item) => command({ action: editing ? "update" : "create", item })}
            />
          )}
          {workView !== "register" && !selected && !creating ? (
            <LegalWorkQueue
              key={workView}
              view={workView}
              people={data.people}
              call={call}
              open={open}
            />
          ) : selected ? (
            <LocalTabs className={s.detail} value={tab} onValueChange={value => setTab(String(value))}>
              <div className={s.actions}>
                <Button appearance="standard"
                  type="submit"
                  variant="ghost"
                  className={button}
                  onClick={() => setSelected(null)}
                >
                  ← All records
                </Button>
                <span className={s.badge}>{selected.item.state}</span>
                <span className={s.small}>
                  Version {selected.item.version} · {selected.item.classification}
                </span>
              </div>
              <h2>{selected.item.title}</h2>
              <div className={s.small}>Record ID: {selected.item.id}</div>
              <OperationSummary
                operation={{
                  status: selected.item.state,
                  responsible:
                    data.people.find((person) => person.id === selected.item.owner_id)?.name ??
                    "Assigned Legal owner",
                  currentStep: selected.item.state.replaceAll("_", " "),
                  nextAction: allowedTransitions(selected.item.kind, selected.item.state).length
                    ? `Review ${selected.item.kind} and continue the current workflow`
                    : "Review the recorded history",
                  blockedReason: null,
                  dueAt: null,
                  recordHref: null,
                  actionKind: selected.item.state === "approval" ? "approval" : "work",
                }}
              >
                {selected.item.due_date && <p className="muted">Due {selected.item.due_date}</p>}
              </OperationSummary>
              <TabsList aria-label="Legal record sections">
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
                  <TabsTrigger value={t}
                    key={t}
                  >
                    {t}
                  </TabsTrigger>
                ))}
              </TabsList>
              <TabsPanel value={tab} key={tab}>
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
                <LegalRetention detail={selected} call={call} command={command} busy={busy} />
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
                  <div className={s.body}>{selected.item.description || "No description yet."}</div>
                  <div className={s.grid}>
                    {fields[selected.item.kind].map((f) => (
                      <div key={f.key}>
                        <span className={s.small}>{f.label}</span>
                        <div className={s.body}>{String(selected.item.data[f.key] ?? "—")}</div>
                      </div>
                    ))}
                  </div>
                  <div className={s.small}>
                    {selected.item.jurisdiction} · {selected.item.currency} ·{" "}
                    {selected.item.timezone} · Due {selected.item.due_date ?? "not set"}
                  </div>
                  {can("edit") && data.enabled && (
                    <Button appearance="standard"
                      type="submit"
                      variant="ghost"
                      className={button}
                      onClick={() => setEditing(true)}
                    >
                      Edit record
                    </Button>
                  )}
                  {can("review") && data.enabled && (
                    <LegalAssignment detail={selected} call={call} command={command} busy={busy} />
                  )}
                  {selected.item.kind === "counsel" && (
                    <>
                      <h3>Spending handoff</h3>
                      <p className={s.small}>
                        Only the summary you enter is copied into a draft purchase request.
                        Confidential case documents stay in Legal. Submit and approve the request
                        through the existing spending workflow.
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
                        onSubmit={(v) => command({ action: "finance.prepare", ...v })}
                      />
                    </>
                  )}
                  <h3>Linked records</h3>
                  {selected.links.map((l) => (
                    <Button appearance="standard"
                      type="submit"
                      variant="ghost"
                      key={`${l.target_id}-${l.relation}`}
                      className={button}
                      onClick={() => open(l.target_id)}
                    >
                      {l.relation} · {l.title}
                    </Button>
                  ))}
                  <ActionForm
                    busy={busy}
                    label="Link record"
                    fields={[
                      { name: "targetId", label: "Target record ID" },
                      {
                        name: "relation",
                        label: "Relationship",
                        options: ["related", "evidence", "amends", "obligation", "derived"],
                      },
                    ]}
                    onSubmit={(v) => command({ action: "link", ...v })}
                  />
                  <h3>Discussion</h3>
                  {selected.comments.map((c) => (
                    <div key={c.id} className={s.event}>
                      <span className={s.small}>
                        {c.visibility} ·{" "}
                        {data.people.find((person) => person.id === c.created_by)?.name ??
                          "Authorized colleague"}
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
                  {selected.item.kind === "contract" && (
                    <LegalExecutedCopies
                      detail={selected}
                      api={api}
                      call={call}
                      command={command}
                      busy={busy}
                      userId={data.userId}
                      canReview={can("approve")}
                      onRefresh={refreshDetail}
                    />
                  )}
                  {selected.item.kind !== "rule" && can("edit") && (
                    <LegalDocumentTextReview
                      detail={selected}
                      call={call}
                      command={command}
                      busy={busy}
                      userId={data.userId}
                      canReview={can("approve")}
                      inspection={inspection}
                    />
                  )}
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
                      onSubmit={(v) => command({ action: "source.attach", ...v })}
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
                      <Button appearance="standard"
                        type="submit"
                        variant="ghost"
                        className={button}
                        disabled={busy}
                        onClick={async () => {
                          try {
                            const j = inspection as { jobId?: string };
                            if (j.jobId) setInspection(await call(`/processing/${j.jobId}`));
                          } catch (e) {
                            setError((e as Error).message);
                          }
                        }}
                      >
                        Refresh processing result
                      </Button>
                      {selected.item.state === "draft" &&
                        typeof (inspection as { result?: { text?: unknown } })?.result?.text ===
                          "string" && (
                          <Button appearance="standard"
                            type="submit"
                            variant="ghost"
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
                          </Button>
                        )}
                      <LegalEvidenceFields value={inspection} />
                    </div>
                  )}
                  {selected.item.state === "draft" && (
                    <Field appearance="standard" className={s.field}>
                      Upload PDF, DOCX or image (up to 10 MB)
                      <TextInput appearance="standard"
                        type="file"
                        disabled={busy}
                        accept=".pdf,.docx,.png,.jpg,.jpeg"
                        onChange={(e) => {
                          const file = e.target.files?.[0];
                          if (file) void upload(file);
                        }}
                      />
                    </Field>
                  )}
                  <p className={s.small}>
                    Each save creates an immutable version. Return the record to draft to add
                    content.
                  </p>
                  {selected.submissions.length > 0 && (
                    <>
                      <h3>External submissions</h3>
                      {selected.submissions.map((u) => (
                        <div className={s.event} key={u.id}>
                          <a href={`${api}/submissions/${u.id}`}>{u.name}</a>
                          <div className={s.small}>
                            {u.created_by} · <TimeText appearance="standard" value={u.created_at} /> · SHA-256{" "}
                            {u.checksum}
                          </div>
                          <p className={s.small}>
                            Review this submission before uploading it as a new internal draft
                            version.
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
                      <pre className={s.body}>{d.body || "Uploaded document"}</pre>
                      <div className={s.actions}>
                        <a data-slot="button" className={['ui-standard-control', (button)].filter(Boolean).join(' ')} href={`${api}/documents/${d.id}`}>
                          Download original
                        </a>
                        {d.body && (
                          <>
                            <a data-slot="button" className={['ui-standard-control', (button)].filter(Boolean).join(' ')} href={`${api}/documents/${d.id}?format=docx`}>
                              DOCX
                            </a>
                            <a data-slot="button" className={['ui-standard-control', (button)].filter(Boolean).join(' ')} href={`${api}/documents/${d.id}?format=pdf`}>
                              PDF
                            </a>
                          </>
                        )}
                        <Button appearance="standard"
                          type="submit"
                          variant="ghost"
                          className={button}
                          disabled={busy}
                          onClick={() => inspect(d.id, "ocr")}
                        >
                          Extract text
                        </Button>
                        {d.media_type === "application/pdf" && (
                          <Button appearance="standard"
                            type="submit"
                            variant="ghost"
                            className={button}
                            disabled={busy}
                            onClick={() => inspect(d.id, "verify")}
                          >
                            Inspect signatures
                          </Button>
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
                  {["contract", "rule", "template"].includes(selected.item.kind) && (
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
                      onSubmit={(v) => command({ action: "revision.create", ...v })}
                    />
                  )}
                  <ActionForm
                    busy={busy}
                    label="Change state"
                    fields={[
                      {
                        name: "state",
                        label: "Next state",
                        options: allowedTransitions(selected.item.kind, selected.item.state),
                      },
                      { name: "reason", label: "Reason", textarea: true },
                    ]}
                    onSubmit={(v) => command({ action: "transition", ...v })}
                  />
                  {selected.item.kind === "rule" && (
                    <Button appearance="standard"
                      type="submit"
                      variant="ghost"
                      className={button}
                      onClick={async () => {
                        try {
                          setInspection(await call(`/rule-impact/${selected.item.id}`));
                          setTab("Documents");
                        } catch (e) {
                          setError((e as Error).message);
                        }
                      }}
                    >
                      Preview affected records
                    </Button>
                  )}
                  {selected.item.kind === "contract" && (
                    <LegalApproval
                      people={data.people}
                      detail={selected}
                      call={call}
                      command={command}
                      busy={busy}
                    />
                  )}
                  {selected.item.kind === "contract" && (
                    <p className={s.notice}>
                      Record an agreement signed outside ERPX in Documents. An independent reviewer
                      must confirm the executed copy before activation.
                    </p>
                  )}
                  {selected.envelopes.map((e) => (
                    <div className={s.notice} key={e.id}>
                      <strong>Signing · {e.state}</strong>
                      <p className={s.body}>{e.consent}</p>
                      <p>Expires {<TimeText appearance="standard" value={e.expires_at} />}</p>
                      {e.signers.map((p) => (
                        <div key={p.user_id}>
                          {p.ordinal + 1}.{" "}
                          {data.people.find((person) => person.id === p.user_id)?.name ??
                            "Recorded signer"}{" "}
                          · {p.state}
                        </div>
                      ))}
                    </div>
                  ))}
                </>
              )}
              {tab === "Access" && (
                <>
                  <h3>Internal membership</h3>
                  {selected.members.map((m) => (
                    <div key={m.user_id} className={s.event}>
                      {data.people.find((person) => person.id === m.user_id)?.name ??
                        "Authorized colleague"}{" "}
                      · {m.access}
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
                  <a href={`/legal-portal/${base.split("/")[2]}`} target="_blank" rel="noreferrer">
                    Open external portal
                  </a>
                  {selected.shares.map((share) => (
                    <div className={s.event} key={share.id}>
                      {share.email} ·{" "}
                      {share.revoked_at ? "Revoked" : <TimeText appearance="standard" value={share.expires_at} />}
                      <div className={s.small}>
                        Accepted user: {share.user_id ?? "Invitation not accepted"}
                      </div>
                      {!share.revoked_at && (
                        <Button appearance="standard"
                          type="submit"
                          variant="ghost"
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
                        </Button>
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
                    Access expires after 30 days. The recipient signs in with the specified verified
                    email.
                  </p>
                </>
              )}
              {tab === "Holds" && (
                <>
                  <p>
                    A preservation hold protects linked evidence. Release requires an independent
                    reviewer.
                  </p>
                  {selected.holds.map((h) => (
                    <div className={s.hold} key={h.id}>
                      <strong>{h.state}</strong>
                      <p>{h.reason}</p>
                      {h.acknowledged_at ? (
                        <p className={s.small}>
                          Acknowledged <TimeText appearance="standard" value={h.acknowledged_at} />
                        </p>
                      ) : (
                        <ActionForm
                          busy={busy}
                          label="Acknowledge preservation notice"
                          fields={[
                            {
                              name: "reason",
                              label: "Confirm the preservation steps you have taken",
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
                        <p className={s.small}>Release is managed on the source record.</p>
                      )}
                      {h.direct && (
                        <ActionForm
                          busy={busy}
                          label={h.state === "active" ? "Request release" : "Approve release"}
                          fields={[{ name: "reason", label: "Release reason" }]}
                          onSubmit={(v) =>
                            command({
                              action:
                                h.state === "active" ? "hold.request-release" : "hold.release",
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
                    AI proposals use accessible documents and independently reviewed, applicable law
                    sources. Review every statement and citation before accepting a draft.
                  </p>
                  {can("manage") && (
                    <Button appearance="standard"
                      type="submit"
                      variant="ghost"
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
                    </Button>
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
                  <Button appearance="standard"
                    type="submit"
                    variant="ghost"
                    className={button}
                    onClick={() =>
                      call(`/ai/${selected.item.id}`)
                        .then(setProposals)
                        .catch((e) => setError(e.message))
                    }
                  >
                    Refresh proposals
                  </Button>
                  {proposals.map((p) => (
                    <div className={s.notice} key={String(p.id)}>
                      <strong>{String(p.state)}</strong>
                      <p>{String(p.question)}</p>
                      {p.error ? <p>{String(p.error)}</p> : null}
                      <LegalEvidenceFields value={p.proposal} />
                      {p.state === "proposed" && selected.item.state === "draft" && can("edit") && (
                        <ActionForm
                          busy={busy}
                          label="Accept reviewed draft as a new version"
                          fields={[
                            { name: "name", label: "Reviewed document name" },
                            {
                              name: "body",
                              label: "Review the content and remove unsupported portions",
                              textarea: true,
                              initial: String(
                                (p.proposal as { draft?: string } | null)?.draft ?? "",
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
                <a data-slot="button" className={['ui-standard-control', (button)].filter(Boolean).join(' ')} href={`${api}/evidence/${selected.item.id}`}>
                  Download evidence and access history
                </a>
              )}
              {tab === "Activity" &&
                selected.events.map((e) => (
                  <div className={s.event} key={e.id}>
                    <strong>{e.action.replace(/[._]/g, " ")}</strong>
                    <p>{e.note}</p>
                    {e.snapshot && (
                      <details>
                        <summary>Record at this event</summary>
                        <LegalSnapshot snapshot={e.snapshot} />
                      </details>
                    )}
                    <span className={s.small}>
                      {data.people.find((person) => person.id === e.actor_id)?.name ??
                        "Authorized colleague"}{" "}
                      · <TimeText appearance="standard" value={e.created_at} /> · v{e.version}
                    </span>
                  </div>
                ))}
              </TabsPanel>
            </LocalTabs>
          ) : (
            <>
              <div className={s.registerHeading}>
                <h2>{labels[kind]}</h2>
                <span className={s.small}>Manage records, deadlines and decisions</span>
              </div>
              <div className={s.actions}>
                {can("export") && (
                  <a data-slot="button" className={['ui-standard-control', (button)].filter(Boolean).join(' ')} href={`${api}/export/${kind}`}>
                    Export register
                  </a>
                )}
                <Button appearance="standard"
                  type="submit"
                  variant="ghost"
                  className={button}
                  onClick={() =>
                    call("/views")
                      .then(setSaved)
                      .catch((e) => setError(e.message))
                  }
                >
                  Saved views
                </Button>
                <Button appearance="standard"
                  type="submit"
                  variant="ghost"
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
                </Button>
                {saved.map((v) => (
                  <Button appearance="standard"
                    type="submit"
                    variant="ghost"
                    key={v.name}
                    className={button}
                    onClick={() => {
                      setKind(v.query.kind);
                      setSearch(v.query.search);
                      setState(v.query.state);
                      load(v.query.kind, 1, v.query.search, v.query.state).catch((e) =>
                        setError(e.message),
                      );
                    }}
                  >
                    {v.name}
                  </Button>
                ))}
              </div>
              {can("create") && (
                <details className={s.event}>
                  <summary>Import records</summary>
                  <p className={s.small}>
                    Upload a JSON array of draft records. Preview validates every row before an
                    atomic import.
                  </p>
                  <TextInput appearance="standard"
                    aria-label="Import records JSON"
                    type="file"
                    accept=".json,application/json"
                    onChange={async (e) => {
                      const file = e.target.files?.[0];
                      if (!file) return;
                      try {
                        if (file.size > 500000) throw new Error("Use a file below 500 KB.");
                        const rows = JSON.parse(await file.text());
                        setImportData(await call("", { action: "import.preview", rows }));
                      } catch (e) {
                        setError((e as Error).message);
                      }
                    }}
                  />
                  {importData && (
                    <>
                      <p>
                        {importData.count} rows · {importData.errors.length} errors
                      </p>
                      {importData.errors.map((e) => (
                        <p key={e.row}>
                          Row {e.row}: {e.message}
                        </p>
                      ))}
                      <Button appearance="standard"
                        type="submit"
                        variant="ghost"
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
                      </Button>
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
                <TextInput appearance="standard"
                  aria-label="Search legal records"
                  placeholder="Search records…"
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                />
                <Select appearance="standard"
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
                </Select>
                <Button appearance="standard" clientManaged type="submit" variant="ghost" className={button}>
                  Search
                </Button>
              </form>
              {data.items.length ? (
                <TableFrame appearance="standard" label="Records"><Table className={s.list}>
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
                          <RecordLink href={`${base}/legal?record=${encodeURIComponent(i.id)}&kind=${kind}`}>{i.title}</RecordLink>
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
                </Table></TableFrame>
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
                  <Button appearance="standard"
                    type="submit"
                    variant="ghost"
                    className={button}
                    disabled={data.page <= 1}
                    onClick={() => load(kind, data.page - 1).catch((e) => setError(e.message))}
                  >
                    Previous
                  </Button>
                  <Button appearance="standard"
                    type="submit"
                    variant="ghost"
                    className={button}
                    disabled={data.page * 30 >= data.total}
                    onClick={() => load(kind, data.page + 1).catch((e) => setError(e.message))}
                  >
                    Next
                  </Button>
                </div>
              </div>
            </>
          )}
        </Card>
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
  call,
  draftKey,
}: {
  kind: LegalKind;
  detail: LegalDetail | null;
  data: LegalData;
  busy: boolean;
  onSave: (x: ItemInput) => Promise<boolean>;
  onCancel: () => void;
  call: (path: string) => Promise<unknown>;
  draftKey: string;
}) {
  const item = detail?.item;
  const formRef = useRef<HTMLFormElement>(null);
  const [restored, setRestored] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [ownerId, setOwnerId] = useState(item?.owner_id ?? data.userId);
  const { people, error: peopleError } = useLegalPeople(call, "owner");
  useEffect(() => {
    if (!formRef.current || !people.length) return;
    try {
      const saved = sessionStorage.getItem(draftKey);
      if (!saved) return;
      const draft = JSON.parse(saved) as { savedAt: number; values: Record<string, string> };
      if (Date.now() - draft.savedAt > 24 * 60 * 60 * 1000) {
        sessionStorage.removeItem(draftKey);
        return;
      }
      for (const [name, value] of Object.entries(draft.values)) {
        if (name === "ownerId") {
          continue;
        }
        const control = formRef.current.elements.namedItem(name);
        if (
          control instanceof HTMLInputElement ||
          control instanceof HTMLTextAreaElement ||
          control instanceof HTMLSelectElement
        )
          control.value = value;
      }
      const frame = requestAnimationFrame(() => {
        if (typeof draft.values.ownerId === "string") setOwnerId(draft.values.ownerId);
        setRestored(true);
        setDirty(true);
      });
      return () => cancelAnimationFrame(frame);
    } catch {}
  }, [draftKey, people]);
  function preserve(form: HTMLFormElement, selectedOwner = ownerId) {
    setDirty(true);
    const values = { ...Object.fromEntries(new FormData(form).entries()), ownerId: selectedOwner };
    try {
      sessionStorage.setItem(draftKey, JSON.stringify({ savedAt: Date.now(), values }));
    } catch {}
  }
  async function save(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const f = new FormData(e.currentTarget),
      values: ItemInput["data"] = {};
    for (const field of fields[kind]) {
      const v = String(f.get(`field:${field.key}`) ?? "");
      if (v) values[field.key] = field.type === "number" ? Number(v) : v;
    }
    const saved = await onSave({
      kind,
      title: String(f.get("title")),
      description: String(f.get("description")),
      classification: String(f.get("classification")) as ItemInput["classification"],
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
    if (saved) {
      try {
        sessionStorage.removeItem(draftKey);
      } catch {}
    }
  }
  return (
    <DraftFormDrawer open title={`${item ? "Edit" : "New"} ${kind}`} dirty={dirty} dismissDisabled={busy} onClose={onCancel} onDiscard={() => {
      try { sessionStorage.removeItem(draftKey); } catch {}
      onCancel();
    }}>
    <FormLayout layout="stack" ref={formRef} onSubmit={save} onChange={(event) => preserve(event.currentTarget, event.target instanceof HTMLSelectElement && event.target.name === "ownerId" ? event.target.value : ownerId)}>
      <p className={s.small}>
        {restored
          ? "Your unsent details were restored in this tab. Review them before saving."
          : "Unsent details are preserved in this tab for up to 24 hours."}
      </p>
      {peopleError && <InlineNotice tone="danger" role="alert">{peopleError}</InlineNotice>}
      <FormGrid as="div">
        <Field appearance="standard" className={`${s.field} ${s.wide}`}>
          Title
          <TextInput appearance="standard"
            name="title"
            required
            minLength={2}
            maxLength={240}
            defaultValue={item?.title}
          />
        </Field>
        <Field appearance="standard" className={`${s.field} ${s.wide}`}>
          Description
          <TextArea appearance="standard" name="description" defaultValue={item?.description} />
        </Field>
        <Field appearance="standard" className={s.field}>
          Owner
          <Select appearance="standard" name="ownerId" required value={ownerId} onChange={(event) => setOwnerId(event.target.value)}>
            <option value="">Choose an eligible owner</option>
            {people.map((p) => (
              <option value={p.id} key={p.id}>
                {p.name}
              </option>
            ))}
          </Select>
          {people.length > 0 && ownerId && !people.some((person) => person.id === ownerId) && (
            <span role="status">The selected owner is no longer eligible. Choose an available person.</span>
          )}
        </Field>
        <Field appearance="standard" className={s.field}>
          Classification
          <Select appearance="standard" name="classification" defaultValue={item?.classification ?? "confidential"}>
            {["internal", "confidential", "restricted"].map((v) => (
              <option key={v}>{v}</option>
            ))}
          </Select>
        </Field>
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
          <Field appearance="standard" key={f.key} className={s.field}>
            {f.label}
            <TextInput appearance="standard" name={f.key} required defaultValue={f.value} />
          </Field>
        ))}
        <Field appearance="standard" className={s.field}>
          Due date
          <TextInput appearance="standard" type="date" name="dueDate" defaultValue={item?.due_date ?? ""} />
        </Field>
        <Field appearance="standard" className={s.field}>
          Priority
          <Select appearance="standard" name="priority" defaultValue={item?.priority ?? "normal"}>
            {["normal", "high", "urgent"].map((v) => (
              <option key={v}>{v}</option>
            ))}
          </Select>
        </Field>
        {fields[kind].map((f) => (
          <Field appearance="standard" className={`${s.field} ${f.type === "textarea" ? s.wide : ""}`} key={f.key}>
            {f.label}
            {f.key === "escalationOwner" ? (
              <Select appearance="standard" name={`field:${f.key}`} defaultValue={item?.data[f.key] ?? ""}>
                <option value="">Choose an escalation owner</option>
                {people.map((person) => (
                  <option key={person.id} value={person.id}>
                    {person.name}
                  </option>
                ))}
              </Select>
            ) : f.type === "textarea" ? (
              <TextArea appearance="standard"
                name={`field:${f.key}`}
                required={f.required}
                defaultValue={item?.data[f.key] ?? ""}
              />
            ) : (
              <TextInput appearance="standard"
                name={`field:${f.key}`}
                type={f.type ?? "text"}
                min={f.type === "number" ? 0 : undefined}
                required={f.required}
                defaultValue={item?.data[f.key] ?? ""}
              />
            )}
          </Field>
        ))}
      </FormGrid>
      <div className="ui-form-actions">
        <DialogClose render={<Button appearance="standard" variant="secondary" type="button" />}>Cancel</DialogClose>
        <Button appearance="standard"
          variant="ghost"
          type="button"
          className={button}
          onClick={() => {
            try {
              sessionStorage.removeItem(draftKey);
            } catch {}
            formRef.current?.reset();
            setOwnerId(item?.owner_id ?? data.userId);
            setRestored(false);
            setDirty(false);
          }}
        >
          Clear unsent details
        </Button>
        <Button appearance="standard" clientManaged type="submit" variant="ghost" className={primary} disabled={busy || !people.some((person) => person.id === ownerId)}>
          {busy ? "Saving…" : "Save record"}
        </Button>
      </div>
    </FormLayout>
    </DraftFormDrawer>
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
        <Field appearance="standard" className={s.field} key={f.name}>
          {f.label}
          {f.choices ? (
            <Select appearance="standard" name={f.name} required={!f.optional}>
              {f.choices.map((c) => (
                <option key={c.value} value={c.value}>
                  {c.label}
                </option>
              ))}
            </Select>
          ) : f.options ? (
            <Select appearance="standard" name={f.name} required>
              {f.options.map((v) => (
                <option key={v}>{v}</option>
              ))}
            </Select>
          ) : f.textarea ? (
            <TextArea appearance="standard" name={f.name} required={!f.optional} defaultValue={f.initial} />
          ) : (
            <TextInput appearance="standard" name={f.name} type={f.type ?? "text"} required={!f.optional} />
          )}
        </Field>
      ))}
      <div>
        <Button appearance="standard"
          clientManaged type="submit"
          variant="ghost"
          className={button}
          disabled={busy || spec.some((f) => f.options?.length === 0 || f.choices?.length === 0)}
        >
          {label}
        </Button>
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
            String(chosen.data.body ?? "").matchAll(/\{\{([a-zA-Z][a-zA-Z0-9_]*)\}\}/g),
            (m) => m[1],
          ),
        ),
      ]
    : [];
  return (
    <details className={s.event}>
      <summary>Prepare from an approved template</summary>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      <Button appearance="standard"
        variant="ghost"
        className={button}
        type="button"
        onClick={async () => {
          try {
            const [a, b] = await Promise.all([
              call("?kind=template&state=approved"),
              call("?kind=template&state=active"),
            ]);
            setTemplates([...(a as LegalData).items, ...(b as LegalData).items]);
          } catch (e) {
            setError((e as Error).message);
          }
        }}
      >
        Find approved templates
      </Button>
      {templates.map((t) => (
        <Button appearance="standard"
          variant="ghost"
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
        </Button>
      ))}
      {chosen && (
        <form
          className={s.detail}
          onSubmit={async (e) => {
            e.preventDefault();
            const values = Object.fromEntries(new FormData(e.currentTarget)) as Record<
              string,
              string
            >;
            if (await onApply(chosen.id, values)) setChosen(null);
          }}
        >
          <h4>
            {chosen.title} · v{chosen.version}
          </h4>
          {variables.map((v) => (
            <Field appearance="standard" key={v} className={s.field}>
              {v}
              <TextInput appearance="standard" name={v} required />
            </Field>
          ))}
          <pre className={s.body}>{String(chosen.data.body ?? "")}</pre>
          <Button appearance="standard" clientManaged type="submit" variant="ghost" className={button} disabled={busy}>
            Create document version
          </Button>
        </form>
      )}
    </details>
  );
}
