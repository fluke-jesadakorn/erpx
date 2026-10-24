"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { RequestValidation } from "@erpx/workplace/components/request-validation";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@erpx/ui/components/foundation";

import { createRequestDraftOnce, createRequestUploader } from "@erpx/feature-work_projection/lib/request-draft-create";
import { ReviewDecisionForm } from "@erpx/workplace/components/review-decision";
import type { ReviewDecision } from "@erpx/feature-work_projection/lib/work-review";
import { RequestSteps, RoutePreview } from "@erpx/workplace/components/request-experience";
import type { RequestPreview } from "@erpx/feature-work_projection/lib/request-preview";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import Link from "@erpx/workplace/components/workspace-link";
import { useEffect, useState, useRef } from "react";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import { useHydrated } from "@erpx/ui/components/use-hydrated";
import { useRecoverableDraft } from "@erpx/ui/components/use-recoverable-draft";
import { clearGuidedIntakeHandoff, readGuidedIntakeHandoff } from "@erpx/feature-shared/lib/ai/guided-intake";
import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";
import { useStaffCommand } from "@erpx/feature-work_projection/components/staff-work-controls";
import {
  serviceCategories,
  requestStateLabels,
  type RequestKind,
  type RequestItem,
} from "@erpx/feature-people_payroll/lib/employee-services";
import type {EmployeeRequest,employeeRequestDetail} from '@erpx/server-records/public/employee-requests';
import styles from "@erpx/workplace/components/services.module.css";
import { Card } from "@erpx/ui/components/foundation";
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
  assignmentId?: string;
  currency?: string;
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
  const createOnce = useRef(createRequestDraftOnce()),
    uploader = useRef(createRequestUploader());
  const [files, setFiles] = useState<File[]>([]),
    [saving, setSaving] = useState(false),
    [saveError, setSaveError] = useState(""),
    [createdId, setCreatedId] = useState(""),
    [recovering, setRecovering] = useState(false),
    [reviewing, setReviewing] = useState(false),
    [routePreview, setRoutePreview] = useState<RequestPreview | null>(null),
    [previewing, setPreviewing] = useState(false);
  const [guided, setGuided] = useState<Record<string, string> | null>(null);
  useEffect(() => {
    try {
      const id = sessionStorage.getItem(`${creationKey}:created`);
      if (request && id === request.id) {
        sessionStorage.removeItem(`${creationKey}:created`);
        return;
      }
      if (!request && !id && sessionStorage.getItem(`${creationKey}:creation`)) {
        // Restore a persisted network attempt after hydration.
        // eslint-disable-next-line react-hooks/set-state-in-effect
        setRecovering(true);
        setSaveError(
          "The previous save has not been confirmed. Retry to recover the same draft. Select any files again if needed.",
        );
      }
      if (id && !request) {
        setCreatedId(id);
        setSaveError(
          "Your draft is saved. Select any files that did not finish uploading again; browsers cannot restore selected files.",
        );
      }
    } catch {}
  }, [creationKey, request]);
  const initial = {
    title: request?.title ?? "",
    description: request?.description ?? "",
    category: request?.category ?? "it",
    location:
      request?.location ??
      options.nodes.find((node) => node.id === options.branchId)?.display_name ??
      "",
    impact: request?.impact ?? "normal",
    branchId: request?.branch_id ?? options.branchId ?? "",
    departmentId: request?.department_id ?? options.departmentId ?? "",
    neededDate: request?.needed_date ?? "",
    currency: request?.currency ?? options.currency ?? "",
    items:
      request?.items_json ??
      JSON.stringify([{ description: "", quantity: "1", unit: "each", estimatedCost: "" }]),
  };
  const draft = useRecoverableDraft(
      `erpx:request:${options.companyId}:${options.userId}:${request?.id ?? `${kind}:${parentRequestId ?? "new"}`}`,
      initial,
      "request",
    ),
    fields = draft.fields;
  useEffect(() => {
    if (kind !== "purchase" || request || !options.assignmentId) return;
    const restore = () =>
      setGuided(
        readGuidedIntakeHandoff(
          options.companyId,
          options.userId,
          options.assignmentId!,
          base,
          "purchase",
        ),
      );
    restore();
    window.addEventListener("erpx:guided-intake-handoff", restore);
    return () => window.removeEventListener("erpx:guided-intake-handoff", restore);
  }, [kind, request, options.companyId, options.userId, options.assignmentId, base]);
  function applyGuided() {
    if (!guided) return;
    draft.update((previous) => {
      const existing = JSON.parse(previous.items) as RequestItem[];
      const first = existing[0] ?? {
        description: "",
        quantity: "",
        unit: "each",
        estimatedCost: "",
      };
      const proposedQuantity =
        /^\d+(?:\.\d+)?$/.test(guided.quantity ?? "") && Number(guided.quantity) > 0
          ? guided.quantity
          : "";
      return {
        ...previous,
        title: guided.title || guided.itemName || previous.title,
        description: guided.description || previous.description,
        location: guided.location || previous.location,
        neededDate: /^\d{4}-\d{2}-\d{2}$/.test(guided.neededDate ?? "")
          ? guided.neededDate
          : previous.neededDate,
        items: JSON.stringify([
          {
            ...first,
            description: guided.itemName || first.description,
            quantity: proposedQuantity || first.quantity,
          },
          ...existing.slice(1),
        ]),
      };
    });
    clearGuidedIntakeHandoff(
      options.companyId,
      options.userId,
      options.assignmentId!,
      base,
      "purchase",
    );
    setGuided(null);
  }
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
      if (request && data.requestId === request.id && data.sourceVersion === request.version)
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
      JSON.stringify(items.map((item, i) => (i === index ? { ...item, [name]: value } : item))),
    );
  useEffect(()=>{
    if(kind!=="service")return;
    let active=true;
    void workFetch(`/api/v1${base}/requests/preview`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({kind,category:fields.category,branchId:fields.branchId||null,departmentId:fields.departmentId||null,parentRequestId,id:request?.id})}).then(async response=>{const body=await response.json();if(active)setRoutePreview(response.ok?body.data:{steps:[],blockers:[body.error?.message??"Unable to check coverage."]});}).catch(()=>{if(active)setRoutePreview({steps:[],blockers:["Unable to check coverage. Retry before sending."]});});
    return()=>{active=false;};
  },[base,kind,fields.category,fields.branchId,fields.departmentId,parentRequestId,request?.id]);
  async function reviewRequest() {
    setPreviewing(true); setSaveError("");
    try {
      const response = await workFetch(`/api/v1${base}/requests/preview`, {method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({...fields,title:fields.title || items[0]?.description || "",kind,branchId:fields.branchId || null,departmentId:fields.departmentId || null,items:kind === "service" ? [] : items,parentRequestId, id:request?.id})});
      const body = await response.json(); if(!response.ok)throw Error(body.error?.message || "Unable to preview routing.");
      setRoutePreview(body.data); setReviewing(true);
    }catch(cause){setSaveError(cause instanceof Error?cause.message:"Unable to preview routing.");}finally{setPreviewing(false);}
  }
  async function persistDraft(submit: boolean) {
    if (draft.conflict || saving) return;
    setSaving(true);
    setSaveError("");
    try {
      const input = {
        ...fields,
        title: fields.title || items[0]?.description || "",
        kind,
        branchId: fields.branchId || null,
        departmentId: fields.departmentId || null,
        items: kind === "service" ? [] : items,
        parentRequestId: request?.parent_request_id ?? parentRequestId,
        expectedVersion: request?.version ?? 1,
      };
      const saved = createdId
        ? { id: createdId }
        : request
          ? await command.send(`requests/${request.id}`, input)
          : { id: await createOnce.current(base, creationKey, input) };
      if (!saved) return;
      if (!request) {
        setCreatedId(saved.id);
        setRecovering(false);
      }
      for (const file of files) await uploader.current(base, saved.id, file);
      if (submit) {
        const response = await workFetch(`/api/v1${base}/requests/${saved.id}`, {
          cache: "no-store",
        });
        const result = (await response.json()) as {
          data?: { request?: { version: number } };
          error?: { message: string };
        };
        if (!response.ok || !result.data?.request)
          throw Error(result.error?.message ?? "Unable to review the saved request.");
        const submitted = await command.send(`requests/${saved.id}/actions`, {
          action: "submit",
          expectedVersion: result.data.request.version,
        });
        if (!submitted) return;
      }
      draft.clear();
      if (!request)
        router.push(
          canonicalUiHref(`${base}/requests/${saved.id}${returnTo ? `?${new URLSearchParams({ return: returnTo })}` : ""}`),
        );
      else {
        setFiles([]);
        router.refresh();
      }
    } catch (error) {
      if (!request) {
        try {
          setRecovering(!!sessionStorage.getItem(`${creationKey}:creation`));
        } catch {}
      }
      setSaveError(
        error instanceof Error ? error.message : "Unable to save. Your details are preserved.",
      );
    } finally {
      setSaving(false);
    }
  }

  return (
    <Card as="section" appearance="standard" className="ui-panel">
      {request && <h2>Request details</h2>}
      <RequestSteps reviewing={reviewing} />
      <p className={styles.quiet}>
        Tell us what you need. Review the request before sending it; you can also save a draft at
        any time.
      </p>
      {guided && (
        <InlineNotice tone="info" className="notice" role="status">
          <p>
            AI prepared details from your message. Review them before using them in this request.
          </p>
          <Button appearance="standard" variant="secondary" type="button" onClick={applyGuided}>
            Use prepared details
          </Button>{" "}
          <Button appearance="standard"
            variant="ghost"
            type="button"
            onClick={() => {
              if (options.assignmentId)
                clearGuidedIntakeHandoff(
                  options.companyId,
                  options.userId,
                  options.assignmentId,
                  base,
                  "purchase",
                );
              setGuided(null);
            }}
          >
            Discard suggestion
          </Button>
        </InlineNotice>
      )}
      {draft.notice}
      {saveError && <InlineNotice tone="danger" role="alert">{saveError}</InlineNotice>}
      {createdId && (
        <p>
          <Link href={canonicalUiHref(`${base}/requests/${createdId}`)}>Open saved draft</Link> · Retry uploads below
          without creating another request.
        </p>
      )}
      <RequestValidation><form
        onSubmit={(e) => {
          e.preventDefault();
          if (reviewing) void persistDraft(true);
          else void reviewRequest();
        }}
      >
        {(createdId || recovering) && (
          <Button appearance="standard"
            variant="primary"
            type="button"
            disabled={saving}
            onClick={() => void persistDraft(reviewing)}
          >
            {saving ? "Saving…" : recovering ? "Recover saved draft" : "Retry saved request"}
          </Button>
        )}

        <fieldset
          disabled={reviewing || !ready || command.busy || saving || !!createdId || recovering}
          className={styles.form}
          hidden={reviewing}
        >
          {kind === "service" && (
            <div className={styles.pair}>
              <Field appearance="standard">
                Service category
                <Select appearance="standard" value={fields.category} onChange={(e) => set("category", e.target.value)}>
                  {serviceCategories.map(([id, label]) => (
                    <option key={id} value={id}>
                      {label}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field appearance="standard">
                Impact
                <Select appearance="standard" value={fields.impact} onChange={(e) => set("impact", e.target.value)}>
                  <option value="normal">Normal</option>
                  <option value="urgent">Urgent</option>
                  <option value="critical">Critical</option>
                </Select>
                <small>{fields.impact === "critical" ? "Work is stopped or many people are affected." : fields.impact === "urgent" ? "Work is affected and needs prompt attention." : "Routine help; work can continue."}</small>
              </Field>
            </div>
          )}
          {kind !== "service" && (
            <>
              <div className={styles.pair}>
                <Field appearance="standard">
                  Needed by{kind === "purchase" ? " (optional)" : ""}
                  <TextInput appearance="standard"
                    type="date"
                    required={kind === "equipment"}
                    value={fields.neededDate}
                    onChange={(e) => set("neededDate", e.target.value)}
                  />
                </Field>
                <Field appearance="standard">
                  Estimate currency
                  <TextInput appearance="standard"
                    value={fields.currency}
                    maxLength={3}
                    onChange={(e) => set("currency", e.target.value.toUpperCase())}
                    placeholder="e.g. THB"
                  />
                </Field>
              </div>
              {items.map((item, index) => (
                <div className={styles.item} key={index}>
                  <Field appearance="standard">
                    Item {index + 1}
                    <TextInput appearance="standard"
                      value={item.description}
                      maxLength={500}
                      required
                      onChange={(e) => itemChange(index, "description", e.target.value)}
                    />
                  </Field>
                  <Field appearance="standard">
                    Quantity
                    <TextInput appearance="standard"
                      type="number"
                      min="0.001"
                      step="0.001"
                      required
                      value={item.quantity}
                      onChange={(e) => itemChange(index, "quantity", e.target.value)}
                    />
                  </Field>
                  <Field appearance="standard">
                    Unit
                    <TextInput appearance="standard"
                      value={item.unit ?? "each"}
                      maxLength={30}
                      required
                      onChange={(e) => itemChange(index, "unit", e.target.value)}
                    />
                  </Field>
                  <Field appearance="standard">
                    Estimated total
                    <TextInput appearance="standard"
                      inputMode="decimal"
                      value={item.estimatedCost}
                      onChange={(e) => itemChange(index, "estimatedCost", e.target.value)}
                      placeholder="Optional"
                    />
                  </Field>
                  <Button appearance="standard"
                    variant="ghost"
                    type="button"

                    disabled={items.length === 1}
                    aria-label={`Remove item ${index + 1}`}
                    onClick={() =>
                      set("items", JSON.stringify(items.filter((_, i) => i !== index)))
                    }
                  >
                    Remove
                  </Button>
                </div>
              ))}
              {items.length < 20 && (
                <Button appearance="standard"
                  variant="secondary"
                  type="button"

                  onClick={() =>
                    set(
                      "items",
                      JSON.stringify([
                        ...items,
                        { description: "", quantity: "1", unit: "each", estimatedCost: "" },
                      ]),
                    )
                  }
                >
                  Add item
                </Button>
              )}
            </>
          )}
          {kind === "purchase" ? <details className={styles.details}><summary>Customize request title (optional)</summary><Field appearance="standard">Request title<TextInput appearance="standard" maxLength={200} value={fields.title} onChange={e=>set("title",e.target.value)} placeholder="Uses the first item"/></Field></details> : <Field appearance="standard">Title<TextInput appearance="standard" maxLength={200} required value={fields.title} onChange={e=>set("title",e.target.value)} placeholder={kind === "service" ? "What needs attention?" : "What do you need?"}/></Field>}
          <Field appearance="standard">
            {kind === "service" ? "Describe the issue" : "Why is it needed?"}
            <TextArea appearance="standard"
              maxLength={10000}
              required
              value={fields.description}
              onChange={(e) => set("description", e.target.value)}
            />
          </Field>
          <details className={styles.details}>
            <summary>Change branch or department (if needed)</summary>
            <div className={styles.pair}>
              <Field appearance="standard">
                Branch
                <Select appearance="standard"
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
                </Select>
              </Field>
              <Field appearance="standard">
                Department
                <Select appearance="standard"
                  value={fields.departmentId}
                  onChange={(e) => set("departmentId", e.target.value)}
                >
                  <option value="">No department selected</option>
                  {options.nodes
                    .filter(
                      (n) => n.kind === "department" && (n.branch_id ?? "") === fields.branchId,
                    )
                    .map((n) => (
                      <option key={n.id} value={n.id}>
                        {n.display_name}
                      </option>
                    ))}
                </Select>
              </Field>
            </div>
          </details>
          <Field appearance="standard">
            {kind === "purchase" ? "Where will it be used or delivered?" : "Location"}
            <TextInput appearance="standard"
              maxLength={300}
              required
              value={fields.location}
              onChange={(e) => set("location", e.target.value)}
              placeholder="Building, floor, room, or shared workplace"
            />
          </Field>
        </fieldset>
        <Field appearance="standard">
          Supporting files (optional)
          <TextInput appearance="standard"
            type="file"
            multiple
            accept="image/jpeg,image/png,image/webp,application/pdf"
            disabled={!ready || saving}
            onChange={(e) => setFiles(Array.from(e.target.files ?? []))}
          />
        </Field>
        {!!files.length && <p>{files.map((file) => file.name).join(", ")}</p>}
        {reviewing && (
          <Card as="section" appearance="standard" className="ui-panel" aria-label="Review request" role="region">
            <h3>Review before you send</h3>
            <p>
              <strong>{fields.title || items[0]?.description || "Untitled request"}</strong> ·{" "}
              {fields.location || "Location needed"}
            </p>
            <p>{fields.description}</p>
            {kind !== "service" && (
              <ul>
                {items.map((item, index) => (
                  <li key={index}>
                    {item.quantity || "?"} {item.unit || "each"} ·{" "}
                    {item.description || "Item needed"}
                    {item.estimatedCost
                      ? ` · Estimated ${fields.currency} ${item.estimatedCost}`
                      : ""}
                  </li>
                ))}
              </ul>
            )}
            <p>
              {fields.neededDate ? `Needed by ${fields.neededDate}` : "No requested date"} ·{" "}
              {files.length} new supporting files
            </p>
            <p className={styles.quiet}>
              {kind === "purchase"
                ? "Sending asks the company to review this purchase. Purchasing will source approved items using the same request."
                : "Sending places this request with the responsible team for review."}
            </p>
          </Card>
        )}
        {(reviewing || kind === "service") && <RoutePreview data={routePreview} base={base} />}
        <div className={`${styles.actions} request-footer`}>
          {!reviewing && (
            <Button appearance="standard"
              clientManaged type="submit"
              variant="primary"
              disabled={previewing || draft.conflict || (kind !== "service" && items.length === 0)}
            >
              Review &amp; submit
            </Button>
          )}
          {reviewing && (
            <Button appearance="standard"
              clientManaged type="submit"
              variant="primary"
              disabled={saving || command.busy || draft.conflict || !!routePreview?.blockers.length}
            >
              {saving ? "Sending…" : "Confirm & submit"}
            </Button>
          )}
          <Button appearance="standard"
            variant="secondary"
            type="button"
            disabled={saving || !!createdId || draft.conflict}
            onClick={() => void persistDraft(false)}
          >
            Save draft
          </Button>
          {reviewing && (
            <Button appearance="standard" variant="ghost" type="button" onClick={() => setReviewing(false)}>
              Edit details
            </Button>
          )}
          {request && (
            <Button appearance="standard"
              variant="ghost"

              type="button"
              onClick={() =>
                void command.send(`requests/${request.id}/actions`, {
                  action: "cancel",
                  expectedVersion: request.version,
                })
              }
            >
              Cancel draft
            </Button>
          )}
        </div>
        {request && dirty && !reviewing && (
          <p className={styles.quiet}>Your changes will be saved when you confirm.</p>
        )}
      </form></RequestValidation>
      {command.error && (
        <InlineNotice tone="danger" role="alert">
          {command.error}{" "}
          <Link href={canonicalUiHref(`${base}/people?member=${request?.requester_user_id ?? options.userId}`)}>
            Review People setup
          </Link>
        </InlineNotice>
      )}
    </Card>
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
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>Next action</h2>
      <p>{detail.nextAction}</p>
      {draft.notice}
      <fieldset className={styles.form} disabled={!ready || command.busy}>
        <Field appearance="standard">
          Message or decision evidence
          <TextArea appearance="standard"
            value={fields.note}
            maxLength={10000}
            onChange={(e) => set("note", e.target.value)}
            placeholder="Describe your review, question, or completed work"
          />
        </Field>
        {actions.includes("assign") && (
          <Field appearance="standard">
            Handler
            <Select appearance="standard"
              value={fields.assigneeUserId}
              onChange={(e) => set("assigneeUserId", e.target.value)}
            >
              <option value="">Choose team member</option>
              {detail.handlers.map((h) => (
                <option key={h.user_id} value={h.user_id}>
                  {h.name}
                </option>
              ))}
            </Select>
          </Field>
        )}
        {actions.includes("link-order") && (
          <Field appearance="standard">
            Purchase order ID
            <TextInput appearance="standard"
              value={fields.purchaseOrderId}
              onChange={(e) => set("purchaseOrderId", e.target.value)}
            />
          </Field>
        )}
        {actions.includes("resolve") && row.kind === "equipment" && (
          <Field appearance="standard">
            Existing asset reference (optional)
            <TextInput appearance="standard"
              value={fields.assetReference}
              onChange={(e) => set("assetReference", e.target.value)}
            />
          </Field>
        )}
        <div className={`${styles.actions} request-footer`}>
          {actions.filter(a=>!["approve","reject","return"].includes(a)).map((action) => (
            <Button appearance="standard"
              variant="ghost"
              key={action}
              type="button"
              className={`button ${["reject", "return", "cancel", "reopen"].includes(action) ? "secondary" : ""}`}
              onClick={() => void act(action)}
            >
              {labels[action]}
            </Button>
          ))}
        </div>
      </fieldset>
      {actions.some(a=>["approve","reject","return"].includes(a)) && <ReviewDecisionForm actions={actions.filter((a):a is ReviewDecision=>["approve","reject","return"].includes(a))} note={fields.note} onNote={note=>set("note",note)} busy={command.busy} minimum={()=>2} onConfirm={action=>void act(action)} />}
      {command.error && <InlineNotice tone="danger" role="alert">{command.error}</InlineNotice>}
    </Card>
  );
}
export function RequestFiles({ base, detail }: { base: string; detail: Detail }) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const canUpload =
    detail.actions.includes("edit") ||
    (detail.relationship.assigned &&
      ["queued", "in_progress", "waiting_requester"].includes(detail.request.state)) ||
    (detail.relationship.own && detail.request.state === "waiting_requester");
  async function upload(file: File) {
    setBusy(true);
    setError("");
    try {
      if (file.size > 10 * 1024 * 1024) throw Error("Choose a file smaller than 10 MB.");
      const r = await workFetch(`/api/v1${base}/uploads`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": crypto.randomUUID(),
        },
        body: JSON.stringify({
          ownerType: "employee_request",
          ownerId: detail.request.id,
          purpose: detail.actions.includes("edit") ? "request_evidence" : "fulfillment",
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
      if (!put.ok) throw Error(saved.error?.message ?? "Upload failed. Your request is saved.");
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Upload failed.");
    } finally {
      setBusy(false);
    }
  }
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>Evidence and files</h2>
      {detail.attachments.map((a) => (
        <p key={a.id}>
          <a href={`/api/v1${base}/attachments/${a.id}/content`}>{a.original_name}</a>{" "}
          <small className={styles.quiet}>
            {a.purpose === "fulfillment" ? "Work evidence" : "Request evidence"}
          </small>
        </p>
      ))}
      {!detail.attachments.length && <p className={styles.quiet}>No files attached.</p>}
      {canUpload && (
        <Field appearance="standard" className={styles.form}>
          Attach photo or document
          <TextInput appearance="standard"
            type="file"
            disabled={busy}
            accept="image/jpeg,image/png,image/webp,application/pdf"
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (file) void upload(file);
              e.target.value = "";
            }}
          />
        </Field>
      )}
      {busy && <InlineNotice tone="info" role="status">Uploading…</InlineNotice>}
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
    </Card>
  );
}
export function RequestHistory({
  detail,
  approverNames = {},
}: {
  detail: Detail;
  approverNames?: Record<string, string>;
}) {
  const approval = JSON.parse(detail.request.approval_json) as {
    currentStep?: number;
    route?: {
      steps?: {
        userId: string;
        stage?: string;
        snapshot?: {
          departmentPolicyId?: string | null;
          departmentPolicyVersion?: number;
          selector?: { kind?: string };
          rerouted?: boolean;
        };
      }[];
    };
    decisions?: {
      override?: boolean;
      userId?: string;
      originalApproverUserId?: string;
      actualApproverUserId?: string;
      note?: string;
      at?: number;
    }[];
  };
  const current = approval.route?.steps?.[approval.currentStep ?? 0];
  return (
    <details className={`card ${styles.details}`}>
      <summary>Conversation and history</summary>
      {current && detail.request.state === "awaiting_approval" && (
        <p>
          Current {current.stage?.replaceAll("_", " ") ?? "approval"} step:{" "}
          {approverNames[current.userId] ?? current.userId}
        </p>
      )}
      {!!approval.route?.steps?.length && (
        <>
          <p>
            Saved route at submission · {approval.route.steps.length} steps. Later policy changes do
            not change this route.
          </p>
          <ol>
            {approval.route.steps.map((step, index) => (
              <li key={index}>
                {index + 1}. {step.stage?.replaceAll("_", " ") ?? "Approval"} ·{" "}
                {approverNames[step.userId] ?? step.userId}
                {step.snapshot?.departmentPolicyVersion !== undefined && (
                  <small className="muted">
                    {" "}
                    ·{" "}
                    {step.snapshot.departmentPolicyId
                      ? `Department flow v${step.snapshot.departmentPolicyVersion}`
                      : "Appointed department head"}{" "}
                    · {step.snapshot.selector?.kind ?? "person"}
                    {step.snapshot.rerouted ? " · approver changed" : ""}
                  </small>
                )}
              </li>
            ))}
          </ol>
        </>
      )}
      {approval.decisions
        ?.filter((decision) => decision.override)
        .map((decision, index) => (
          <p key={`override-${index}`}>
            <strong>Decision on behalf of assigned approver:</strong>{" "}
            {approverNames[decision.originalApproverUserId ?? ""] ??
              decision.originalApproverUserId}{" "}
            → {approverNames[decision.actualApproverUserId ?? ""] ?? decision.actualApproverUserId}{" "}
            · {decision.note}
            {decision.at ? (
              <>
                {" "}
                · <TimeText appearance="standard" value={decision.at} />
              </>
            ) : (
              ""
            )}
          </p>
        ))}
      {detail.events.map((e) => (
        <article key={e.id} className={styles.history}>
          <strong>{e.action.replaceAll("-", " ")}</strong>
          <small className={styles.quiet}>
            {" "}
            · <TimeText appearance="standard" value={e.created_at} />
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
  return <span className="ui-badge">{requestStateLabels[state] ?? state}</span>;
}
