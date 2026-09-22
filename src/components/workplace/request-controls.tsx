"use client";

import { createRequestDraftOnce, createRequestUploader } from "@/lib/request-draft-create";
import { workFetch } from "@/lib/work-fetch";
import Link from "next/link";
import { useEffect, useState, useRef } from "react";
import { useRouter } from "next/navigation";
import { useHydrated } from "@/components/ui/use-hydrated";
import { useRecoverableDraft } from "@/components/ui/use-recoverable-draft";
import { useStaffCommand } from "@/components/modules/staff-work-controls";
import {
  serviceCategories,
  requestKinds,
  requestStateLabels,
  type RequestKind,
  type RequestItem,
} from "@/lib/employee-services";
import type {
  EmployeeRequest,
  employeeRequestDetail,
} from "@/server/employee-requests";
import styles from "./services.module.css";
export type RequestOptions = {
  nodes: {
    id: string;
    kind: string;
    display_name: string;
    branch_id: string | null;
  }[];
  branchId: string | null;
  departmentId: string | null;
  companyId: string;
  userId: string;
};
export function RequestEditor({
  base,
  kind,
  options,
  request,
  parentRequestId = null,
  returnTo = "",
}: {
  base: string;
  kind: RequestKind;
  options: RequestOptions;
  request?: EmployeeRequest;
  parentRequestId?: string | null;
  returnTo?: string;
}) {
  const ready = useHydrated(),
    router = useRouter(),
    command = useStaffCommand(base);
  const creationKey = `erpx:request-create:${base}:${options.companyId}:${options.userId}:${kind}:${parentRequestId ?? "new"}`;
  const createOnce = useRef(createRequestDraftOnce()), uploader = useRef(createRequestUploader());
  const [files,setFiles] = useState<File[]>([]), [saving,setSaving] = useState(false), [saveError,setSaveError] = useState(""), [createdId,setCreatedId] = useState(""), [recovering,setRecovering] = useState(false);
  useEffect(() => {
    try {
      const id = sessionStorage.getItem(`${creationKey}:created`);
      if (request && id === request.id) { sessionStorage.removeItem(`${creationKey}:created`); return; }
      if (!request && !id && sessionStorage.getItem(`${creationKey}:creation`)) {
        // Restore a persisted network attempt after hydration.
        // eslint-disable-next-line react-hooks/set-state-in-effect
        setRecovering(true);
        setSaveError("The previous save has not been confirmed. Retry to recover the same draft. Select any files again if needed.");
      }
      if (id && !request) {
        setCreatedId(id);
        setSaveError("Your draft is saved. Select any files that did not finish uploading again; browsers cannot restore selected files.");
      }
    } catch {}
  },[creationKey,request]);
  const initial = {
    title: request?.title ?? "",
    description: request?.description ?? "",
    category: request?.category ?? "it",
    location: request?.location ?? "",
    impact: request?.impact ?? "normal",
    branchId: request?.branch_id ?? options.branchId ?? "",
    departmentId: request?.department_id ?? options.departmentId ?? "",
    neededDate: request?.needed_date ?? "",
    currency: request?.currency ?? "",
    items:
      request?.items_json ??
      JSON.stringify([{ description: "", quantity: "", unit: "each", estimatedCost: "" }]),
  };
  const draft = useRecoverableDraft(
      `erpx:request:${options.companyId}:${options.userId}:${request?.id ?? `${kind}:${parentRequestId ?? "new"}`}`,
      initial,
      "request",
    ),
    fields = draft.fields;
  const receiveAi = draft.receiveAi;
  useEffect(() => {
    function suggestion(event: Event) {
      const data = (
        event as CustomEvent<{
          requestId: string;
          sourceVersion: number;
          fields: { name: string; value: string | null }[];
          overwrite: boolean;
          proposalId?: string | null;
          invalidate?: boolean;
        }>
      ).detail;
      if (
        request &&
        data.requestId === request.id &&
        data.sourceVersion === request.version
      )
        receiveAi(data);
    }
    window.addEventListener("erpx:request-ai", suggestion);
    return () => window.removeEventListener("erpx:request-ai", suggestion);
  }, [request, receiveAi]);
  const items = JSON.parse(fields.items) as RequestItem[],
    dirty = JSON.stringify(fields) !== JSON.stringify(initial);
  const set = (name: keyof typeof fields, value: string) =>
    draft.update((p) => ({ ...p, [name]: value }));
  const itemChange = (index: number, name: keyof RequestItem, value: string) =>
    set(
      "items",
      JSON.stringify(
        items.map((item, i) =>
          i === index ? { ...item, [name]: value } : item,
        ),
      ),
    );
  async function save(e: React.FormEvent) {
    e.preventDefault();
    setSaving(true); setSaveError("");
    try {
      const input = {...fields,kind,branchId:fields.branchId || null,departmentId:fields.departmentId || null,items:kind === "service" ? [] : items,parentRequestId:request?.parent_request_id ?? parentRequestId,expectedVersion:request?.version ?? 1};
      const saved = request ? await command.send(`requests/${request.id}`,input) : {id: await createOnce.current(base,creationKey,input)};
      if (!saved) return;
      if (!request) { setCreatedId(saved.id); setRecovering(false); }
      for (const file of files) await uploader.current(base,saved.id,file);
      draft.clear();
      if (!request) router.push(`${base}/requests/${saved.id}${returnTo ? `?${new URLSearchParams({return:returnTo})}` : ""}`);
      else { setFiles([]); router.refresh(); }
    } catch (error) { if (!request) { try { setRecovering(!!sessionStorage.getItem(`${creationKey}:creation`)); } catch {} } setSaveError(error instanceof Error ? error.message : "Unable to save. Your details are preserved.");}
    finally {setSaving(false);}
  }

  return (
    <section className="card">
      <h2>
        {request
          ? "Request details"
          : requestKinds.find(([id]) => id === kind)?.[1]}
      </h2>
      <p className={styles.quiet}>
        Review your details and attach supporting files here. A draft keeps your progress safe.
      </p>
      {draft.notice}
      {saveError && <p role="alert">{saveError}</p>}
      {createdId && <p><Link href={`${base}/requests/${createdId}`}>Open saved draft</Link> · Retry uploads below without creating another request.</p>}
      <form onSubmit={save}>
        <label>Supporting files<input type="file" multiple accept="image/jpeg,image/png,image/webp,application/pdf" disabled={!ready || saving} onChange={e => setFiles(Array.from(e.target.files ?? []))}/></label>
        {!!files.length && <p>{files.map(file => file.name).join(", ")}</p>}
        {(createdId || recovering) && <button className="button" disabled={saving}>{saving ? "Saving…" : recovering ? "Recover saved draft" : "Retry uploads and open draft"}</button>}

        <fieldset disabled={!ready || command.busy || saving || !!createdId || recovering} className={styles.form}>
          {kind === "service" && (
            <div className={styles.pair}>
              <label>
                Service category
                <select
                  value={fields.category}
                  onChange={(e) => set("category", e.target.value)}
                >
                  {serviceCategories.map(([id, label]) => (
                    <option key={id} value={id}>
                      {label}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Impact
                <select
                  value={fields.impact}
                  onChange={(e) => set("impact", e.target.value)}
                >
                  <option value="normal">Normal</option>
                  <option value="urgent">Urgent</option>
                  <option value="critical">Critical</option>
                </select>
              </label>
            </div>
          )}
          <label>
            Title
            <input
              maxLength={200}
              value={fields.title}
              onChange={(e) => set("title", e.target.value)}
              placeholder={
                kind === "service"
                  ? "What needs attention?"
                  : "What do you need?"
              }
            />
          </label>
          <label>
            {kind === "service" ? "Describe the issue" : "Purpose and details"}
            <textarea
              maxLength={10000}
              value={fields.description}
              onChange={(e) => set("description", e.target.value)}
            />
          </label>
          <div className={styles.pair}>
            <label>
              Branch
              <select
                value={fields.branchId}
                onChange={(e) => {
                  set("branchId", e.target.value);
                  set("departmentId", "");
                }}
              >
                <option value="">Company / shared location</option>
                {options.nodes
                  .filter((n) => n.kind === "branch")
                  .map((n) => (
                    <option key={n.id} value={n.id}>
                      {n.display_name}
                    </option>
                  ))}
              </select>
            </label>
            <label>
              Department
              <select
                value={fields.departmentId}
                onChange={(e) => set("departmentId", e.target.value)}
              >
                <option value="">No department selected</option>
                {options.nodes
                  .filter(
                    (n) =>
                      n.kind === "department" &&
                      (n.branch_id ?? "") === fields.branchId,
                  )
                  .map((n) => (
                    <option key={n.id} value={n.id}>
                      {n.display_name}
                    </option>
                  ))}
              </select>
            </label>
          </div>
          <label>
            Location
            <input
              maxLength={300}
              value={fields.location}
              onChange={(e) => set("location", e.target.value)}
              placeholder="Building, floor, room, or shared workplace"
            />
          </label>
          {kind !== "service" && (
            <>
              <div className={styles.pair}>
                <label>
                  Needed by{kind === "purchase" ? " (optional)" : ""}
                  <input
                    type="date"
                    value={fields.neededDate}
                    onChange={(e) => set("neededDate", e.target.value)}
                  />
                </label>
                <label>
                  Estimate currency
                  <input
                    value={fields.currency}
                    maxLength={3}
                    onChange={(e) =>
                      set("currency", e.target.value.toUpperCase())
                    }
                    placeholder="e.g. THB"
                  />
                </label>
              </div>
              {items.map((item, index) => (
                <div className={styles.item} key={index}>
                  <label>
                    Item {index + 1}
                    <input
                      value={item.description}
                      maxLength={500}
                      onChange={(e) =>
                        itemChange(index, "description", e.target.value)
                      }
                    />
                  </label>
                  <label>
                    Quantity
                    <input
                      inputMode="decimal"
                      value={item.quantity}
                      onChange={(e) =>
                        itemChange(index, "quantity", e.target.value)
                      }
                    />
                  </label>
                  <label>
                    Unit
                    <input
                      value={item.unit ?? "each"}
                      maxLength={30}
                      required
                      onChange={(e) => itemChange(index, "unit", e.target.value)}
                    />
                  </label>
                  <label>
                    Estimated total
                    <input
                      inputMode="decimal"
                      value={item.estimatedCost}
                      onChange={(e) =>
                        itemChange(index, "estimatedCost", e.target.value)
                      }
                      placeholder="Optional"
                    />
                  </label>
                  <button
                    type="button"
                    className="button ghost"
                    aria-label={`Remove item ${index + 1}`}
                    onClick={() =>
                      set(
                        "items",
                        JSON.stringify(items.filter((_, i) => i !== index)),
                      )
                    }
                  >
                    Remove
                  </button>
                </div>
              ))}
              {items.length < 20 && (
                <button
                  type="button"
                  className="button secondary"
                  onClick={() =>
                    set(
                      "items",
                      JSON.stringify([
                        ...items,
                        { description: "", quantity: "", unit: "each", estimatedCost: "" },
                      ]),
                    )
                  }
                >
                  Add item
                </button>
              )}
            </>
          )}
          <section aria-label="Review request"><h3>Review</h3><p>{fields.title || "Add a title"} · {fields.location || "No location specified"}</p><p>{fields.description || "Add a description"}</p><p>{files.length} supporting files · Approval is requested after you review the saved draft.</p></section>
          <div className={styles.actions}>
            <button className="button" disabled={draft.conflict}>
              Save draft
            </button>
            {request && (
              <button
                className="button secondary"
                type="button"
                disabled={dirty || draft.conflict}
                onClick={() =>
                  void command.send(`requests/${request.id}/actions`, {
                    action: "submit",
                    expectedVersion: request.version,
                  })
                }
              >
                Submit request
              </button>
            )}
            {request && (
              <button
                className="button ghost"
                type="button"
                onClick={() =>
                  void command.send(`requests/${request.id}/actions`, {
                    action: "cancel",
                    expectedVersion: request.version,
                  })
                }
              >
                Cancel draft
              </button>
            )}
          </div>
          {request && dirty && (
            <p className={styles.quiet}>Save your changes before submitting.</p>
          )}
        </fieldset>
      </form>
      {command.error && (
        <p role="alert">
          {command.error}{" "}
          <Link href={`${base}/people?member=${request?.requester_user_id ?? options.userId}`}>
            Review People setup
          </Link>
        </p>
      )}
    </section>
  );
}
type Detail = Awaited<ReturnType<typeof employeeRequestDetail>>;
export function RequestActions({
  base,
  detail,
  userId,
  companyId,
}: {
  base: string;
  detail: Detail;
  userId: string;
  companyId: string;
}) {
  const ready = useHydrated(),
    command = useStaffCommand(base),
    row = detail.request;
  const draft = useRecoverableDraft(
    `erpx:request-action:${companyId}:${userId}:${row.id}:${row.state}`,
    {
      note: "",
      assigneeUserId: "",
      purchaseOrderId: row.purchase_order_id ?? "",
      assetReference: "",
    },
    "request",
  );
  const fields = draft.fields,
    set = (name: keyof typeof fields, value: string) =>
      draft.update((p) => ({ ...p, [name]: value }));
  const labels: Record<string, string> = {
    approve: "Approve request",
    return: "Return for correction",
    reject: "Reject request",
    assign: "Assign handler",
    start: "Start work",
    ask: "Ask requester",
    reply: "Send reply",
    comment: "Send message",
    resolve:
      row.kind === "equipment"
        ? "Record handover"
        : row.kind === "purchase"
          ? "Confirm fulfillment"
          : "Resolve issue",
    confirm: "Confirm completion",
    reopen: "Reopen request",
    cancel: "Cancel request",
    "link-order": "Link purchase order",
  };
  const actions = detail.actions.filter(
    (a) => labels[a] && !(a === "cancel" && detail.actions.includes("edit")),
  );
  if (!actions.length) return null;
  async function act(action: string) {
    const saved = await command.send(`requests/${row.id}/actions`, {
      action,
      expectedVersion: row.version,
      ...fields,
      assigneeUserId: fields.assigneeUserId || null,
      purchaseOrderId: fields.purchaseOrderId || null,
    });
    if (saved) draft.clear();
  }
  return (
    <section className="card">
      <h2>Next action</h2>
      <p>{detail.nextAction}</p>
      {draft.notice}
      <fieldset className={styles.form} disabled={!ready || command.busy}>
        <label>
          Message or decision evidence
          <textarea
            value={fields.note}
            maxLength={10000}
            onChange={(e) => set("note", e.target.value)}
            placeholder="Describe your review, question, or completed work"
          />
        </label>
        {actions.includes("assign") && (
          <label>
            Handler
            <select
              value={fields.assigneeUserId}
              onChange={(e) => set("assigneeUserId", e.target.value)}
            >
              <option value="">Choose team member</option>
              {detail.handlers.map((h) => (
                <option key={h.user_id} value={h.user_id}>
                  {h.name}
                </option>
              ))}
            </select>
          </label>
        )}
        {actions.includes("link-order") && (
          <label>
            Purchase order ID
            <input
              value={fields.purchaseOrderId}
              onChange={(e) => set("purchaseOrderId", e.target.value)}
            />
          </label>
        )}
        {actions.includes("resolve") && row.kind === "equipment" && (
          <label>
            Existing asset reference (optional)
            <input
              value={fields.assetReference}
              onChange={(e) => set("assetReference", e.target.value)}
            />
          </label>
        )}
        <div className={styles.actions}>
          {actions.map((action) => (
            <button
              key={action}
              type="button"
              className={`button ${["reject", "return", "cancel", "reopen"].includes(action) ? "secondary" : ""}`}
              onClick={() => void act(action)}
            >
              {labels[action]}
            </button>
          ))}
        </div>
      </fieldset>
      {command.error && <p role="alert">{command.error}</p>}
    </section>
  );
}
export function RequestFiles({
  base,
  detail,
}: {
  base: string;
  detail: Detail;
}) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const canUpload =
    detail.actions.includes("edit") ||
    (detail.relationship.assigned &&
      ["queued", "in_progress", "waiting_requester"].includes(
        detail.request.state,
      )) ||
    (detail.relationship.own && detail.request.state === "waiting_requester");
  async function upload(file: File) {
    setBusy(true);
    setError("");
    try {
      if (file.size > 10 * 1024 * 1024)
        throw Error("Choose a file smaller than 10 MB.");
      const r = await workFetch(`/api/v1${base}/uploads`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": crypto.randomUUID(),
        },
        body: JSON.stringify({
          ownerType: "employee_request",
          ownerId: detail.request.id,
          purpose: detail.actions.includes("edit")
            ? "request_evidence"
            : "fulfillment",
          filename: file.name,
          mediaType: file.type,
          bytes: file.size,
        }),
      });
      const b = (await r.json()) as {
        data: { uploadUrl: string };
        error?: { message: string };
      };
      if (!r.ok) throw Error(b.error?.message ?? "Unable to prepare upload.");
      const put = await workFetch(b.data.uploadUrl, {
        method: "PUT",
        headers: { "Content-Type": file.type },
        body: file,
      });
      const saved = (await put.json()) as { error?: { message: string } };
      if (!put.ok)
        throw Error(
          saved.error?.message ?? "Upload failed. Your request is saved.",
        );
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Upload failed.");
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="card">
      <h2>Evidence and files</h2>
      {detail.attachments.map((a) => (
        <p key={a.id}>
          <a href={`/api/v1${base}/attachments/${a.id}/content`}>
            {a.original_name}
          </a>{" "}
          <small className={styles.quiet}>
            {a.purpose === "fulfillment" ? "Work evidence" : "Request evidence"}
          </small>
        </p>
      ))}
      {!detail.attachments.length && (
        <p className={styles.quiet}>No files attached.</p>
      )}
      {canUpload && (
        <label className={styles.form}>
          Attach photo or document
          <input
            type="file"
            disabled={busy}
            accept="image/jpeg,image/png,image/webp,application/pdf"
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (file) void upload(file);
              e.target.value = "";
            }}
          />
        </label>
      )}
      {busy && <p role="status">Uploading…</p>}
      {error && <p role="alert">{error}</p>}
    </section>
  );
}
export function RequestHistory({ detail }: { detail: Detail }) {
  return (
    <details className={`card ${styles.details}`}>
      <summary>Conversation and history</summary>
      {detail.events.map((e) => (
        <article key={e.id} className={styles.history}>
          <strong>{e.action.replaceAll("-", " ")}</strong>
          <small className={styles.quiet}>
            {" "}
            ·{" "}
            {new Date(e.created_at).toLocaleString("en-GB", {
              timeZone: "Asia/Bangkok",
            })}
          </small>
          {e.note && <p>{e.note}</p>}
          <small className={styles.quiet}>
            Version {e.request_version} · {e.actor_name}
          </small>
        </article>
      ))}
    </details>
  );
}
export function RequestState({ state }: { state: string }) {
  return <span className="badge">{requestStateLabels[state] ?? state}</span>;
}
