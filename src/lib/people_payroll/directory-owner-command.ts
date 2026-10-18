import {workFetch, type WorkCommandCompletion} from "@/lib/work_projection/work-fetch";
import {captureWorkCommandContext} from "@/lib/work_projection/command-context";
import {retainWorkCommandCompletion} from "@/lib/work_projection/command-completion";
import {announceWorkChange} from "@/lib/work_projection/work-refresh";
import {peopleLifecyclePurchased, type PeoplePurchaseControls} from "./purchase-controls";
import type {LifecyclePreview} from "./people-lifecycle";

export type DirectoryOwnerScope = {ownerKey: string; source: string; canManage: boolean; draftVersion?: number; hasDraft?: boolean};
export type DirectoryCommandCurrent = DirectoryOwnerScope & {base: string; purchases?: PeoplePurchaseControls; canReview?: boolean; personId?: string; engagementId?: string; recordId?: string; memberUserId?: string; cases?: {id: string; kind: string}[]};
type Attempt = {base: string; ownerKey: string; workOwner: string; path: string; body: string; key: string; uncertain: boolean; hadForm: boolean; draftVersion?: number; hadOtherDraft: boolean; completion: ReturnType<typeof retainWorkCommandCompletion>};
const object = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object" && !Array.isArray(value);
const strings = (value: unknown) => Array.isArray(value) && value.every(item => typeof item === "string");
export function validDirectoryLifecyclePreview(value: unknown): value is LifecyclePreview {
  return object(value) && typeof value.token === "string" && !!value.token && typeof value.ready === "boolean" && ["blockers", "removed", "added", "accounts", "affectedPositions", "affectedPeople", "affectedComputers"].every(key => strings(value[key])) && (value.definition === null || object(value.definition));
}
function workOwner() {const node = typeof document === "undefined" ? null : document.querySelector<HTMLElement>("[data-work-revision]"); return node ? JSON.stringify([node.dataset.workUser, node.dataset.workCompany, node.dataset.workAssignment]) : "server";}
function changed() {return Object.assign(Error("Your current People owner, Company or work role changed. Recover the original request after reviewing the current owner; your draft is retained."), {code: "WORK_CONTEXT_CHANGED"});}
function route(family: "directory" | "lifecycle", path: string) {
  return family === "directory" ? path === "" || /^\/[^/]+(?:\/status)?$/.test(path) : ["placements/preview", "placements/draft", "placements/submit", "rehire"].includes(path) || /^cases\/[^/]+\/(?:cancel|request-access)$/.test(path);
}
function allowed(current: DirectoryCommandCurrent, family: "directory" | "lifecycle", path: string, input: Record<string, unknown>, replay: boolean, key: string) {
  if (family === "directory" && (path === "" ? !!current.recordId && !(replay && current.recordId === `person-${key}` && input.linkedMemberUserId === current.memberUserId) : /^\/([^/]+)/.exec(path)?.[1] !== current.recordId)) return false;
  if (family === "lifecycle" && (path === "rehire" || path.startsWith("placements/")) && (input.personId !== current.personId || !replay && input.engagementId !== current.engagementId)) return false;
  if (family === "lifecycle" && path.startsWith("cases/") && !current.cases?.some(item => path === `cases/${item.id}/cancel` || path === `cases/${item.id}/request-access`)) return false;
  if (family === "directory") return current.canManage && (!current.purchases || (replay ? current.purchases.directory.history : path.endsWith("/status") && input.status !== "active" ? current.purchases.directory.settleExisting : current.purchases.directory.newWork));
  if (path.endsWith("request-access") ? !current.canReview : !current.canManage) return false;
  if (!current.purchases) return true;
  if (replay) return current.purchases.lifecycle.history;
  if (path.endsWith("/cancel")) return current.purchases.lifecycle.settleExisting;
  const kind = typeof input.kind === "string" ? input.kind : current.cases?.find(item => path === `cases/${item.id}/request-access`)?.kind ?? "onboarding";
  return peopleLifecyclePurchased(current.purchases, kind);
}
function acknowledgement(family: "directory" | "lifecycle", selected: Attempt, value: unknown) {
  if (!object(value)) return false;
  const input: Record<string, unknown> = JSON.parse(selected.body);
  if (family === "directory") {
    const match = /^\/([^/]+)(?:\/status)?$/.exec(selected.path), id = match?.[1] ?? `person-${selected.key}`;
    return value.id === id && value.version === Number(input.expectedVersion ?? 0) + 1 && (!selected.path.endsWith("/status") || value.status === input.status);
  }
  if (selected.path === "rehire") return value.id === `eng-rehire-${selected.key}` && value.personId === input.personId && value.status === "onboarding";
  if (selected.path.startsWith("placements/")) return value.id === (input.id ?? `lifecycle-${selected.key}`) && value.version === Number(input.expectedVersion ?? 0) + 1 && value.status === (selected.path.endsWith("/draft") ? "draft" : "scheduled");
  const id = /^cases\/([^/]+)\//.exec(selected.path)?.[1];
  return value.id === id && (selected.path.endsWith("/cancel") ? value.status === "cancelled" : strings(value.reviews));
}

/** Closed owner-family transport. Current authority is rechecked after ORIGINAL
 * JSON; unknown mutations keep the first body/key/form, independent of later edits. */
export function createDirectoryOwnerCommand(family: "directory" | "lifecycle", send: typeof workFetch = workFetch) {
  let current: () => DirectoryCommandCurrent | null = () => null, attempt: Attempt | null = null, busy = false;
  function fence() {const source = current(), work = captureWorkCommandContext(); if (!source) throw changed(); const captured = {...source}, purchaseStamp = JSON.stringify(source.purchases); return {captured, assertCurrent: () => {work(); const now = current(); if (!now || now.base !== captured.base || now.ownerKey !== captured.ownerKey || now.source !== captured.source || JSON.stringify(now.purchases) !== purchaseStamp || now.canManage !== captured.canManage || now.canReview !== captured.canReview) throw changed();}};}
  async function execute(selected: Attempt, preview: boolean, retry: boolean) {
    if (busy) throw Error("The original People request is still running.");
    const {captured, assertCurrent} = fence(), input: unknown = JSON.parse(selected.body);
    if (!object(input) || selected.base !== captured.base || selected.ownerKey !== captured.ownerKey || selected.workOwner !== workOwner()) throw changed();
    if (!allowed(captured, family, selected.path, input, retry, selected.key)) throw Error("The current People purchase or role does not permit this operation.");
    busy = true;
    if (!preview) {attempt = selected; selected.uncertain = true;}
    try {
      const response = await send(`/api/v1${selected.base}/people/${family}${selected.path && family === "lifecycle" ? "/" : ""}${selected.path}`, {method: "POST", headers: {"Content-Type": "application/json", "Idempotency-Key": selected.key}, body: selected.body}, selected.completion(), "caller");
      const value: unknown = await response.json(); assertCurrent();
      if (!response.ok) {
        const error = object(value) && object(value.error) ? value.error : {};
        // A typed error after send can follow a lost COMMIT acknowledgement.
        // Only native acknowledgement may consume this mutation's first key/body.
        throw Error(typeof error.message === "string" ? error.message : "Unable to save People changes.");
      }
      if (!object(value) || !("data" in value) || !(preview ? validDirectoryLifecyclePreview(value.data) : acknowledgement(family, selected, value.data))) throw Error("The native People acknowledgement is incomplete. Recover the original request.");
      assertCurrent();
      const unchanged = () => {const completion = selected.completion(); return current()?.draftVersion === selected.draftVersion && !selected.hadOtherDraft && (!selected.hadForm || !!completion) && (!completion || completion.form.isConnected && completion.form.getAttribute("data-erpx-edit-version") === completion.formEditVersion);};
      if (preview && !unchanged()) throw Error("Your newer employment draft is retained. Preview it again before continuing.");
      let acknowledged = false;
      return {data: value.data, path: selected.path, assertCurrent, unchanged, acknowledge: () => {
        assertCurrent(); if (acknowledged || !preview && attempt !== selected) return false;
        if (preview && !unchanged()) throw Error("Your newer employment draft is retained. Preview it again before continuing.");
        acknowledged = true;
        if (!preview) {attempt = null; if (typeof window !== "undefined") {window.dispatchEvent(new CustomEvent("erpx:record-command-saved", {detail: selected.completion()})); announceWorkChange();}}
        return true;
      }};
    } finally {busy = false;}
  }
  return {
    bind: (reader: () => DirectoryCommandCurrent | null) => {current = reader;},
    send: (path: string, input: unknown, completion?: WorkCommandCompletion) => {
      if (attempt || busy) return Promise.reject(Error("Recover the original People request first. Your newer draft is retained."));
      if (!route(family, path)) return Promise.reject(Error("Unsupported People owner operation."));
      const now = current(); if (!now) return Promise.reject(changed());
      const body = JSON.stringify(input); if (!body || !object(input)) return Promise.reject(Error("Review the People request before submitting."));
      const selected = {base: now.base, ownerKey: now.ownerKey, workOwner: workOwner(), path, body, key: crypto.randomUUID(), uncertain: false, hadForm: !!completion, draftVersion: now.draftVersion, hadOtherDraft: !completion && !!now.hasDraft && (family === "directory" || path.startsWith("cases/")), completion: retainWorkCommandCompletion(completion)};
      return execute(selected, path === "placements/preview", false);
    },
    retry: () => attempt ? execute(attempt, false, true) : Promise.reject(Error("No original People request needs recovery.")),
    hasPending: () => attempt !== null,
    busy: () => busy,
  };
}
