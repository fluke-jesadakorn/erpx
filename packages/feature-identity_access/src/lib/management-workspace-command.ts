import { workFetch, type WorkCommandCompletion } from "@erpx/feature-work_projection/lib/work-fetch";
import { captureWorkCommandContext } from "@erpx/feature-work_projection/lib/command-context";
import { retainWorkCommandCompletion } from "@erpx/feature-work_projection/lib/command-completion";
import { announceWorkChange } from "@erpx/feature-work_projection/lib/work-refresh";
import type { ManagementChange, ManagementImpact, ManagementSnapshot } from "@erpx/feature-identity_access/lib/user-management";

type Current = { api: string; source: string; draft: number };
export type ManagementPreview = { change: ManagementChange; previewToken: string; impact: ManagementImpact; key: string; draft: number; completion?: WorkCommandCompletion };
type Mutation = { url: string; body: string; key: string; expected: { kind: "save"; token: string; id?: string } | { kind: "review"; id: string; status: string } };
type Attempt = { owner: string; workOwner: string; commands: Mutation[]; cursor: number; uncertain: boolean; draft: number; hadCompletion: boolean; completion: ReturnType<typeof retainWorkCommandCompletion> };
const object = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object" && !Array.isArray(value);
const impactKeys = ["membersAdded", "membersRemoved", "accessRemoved", "accessPending", "accessBlocked", "matchingPeople"] as const;
export function validManagementImpact(value: unknown): value is ManagementImpact {
  return object(value) && Object.keys(value).length === impactKeys.length && impactKeys.every(key => Array.isArray(value[key]) && value[key].every(item => typeof item === "string"));
}
function snapshot(value: unknown): value is ManagementSnapshot {
  return object(value) && ["attributes", "groups", "policies", "attachments", "values", "people", "nodes", "candidates"].every(key => Array.isArray(value[key])) && ["canManage", "canDefine", "canManagePeople", "canManagePersonAttributes", "canManageOrganization", "canReview"].every(key => typeof value[key] === "boolean");
}
function workOwner() {
  const node = typeof document === "undefined" ? null : document.querySelector<HTMLElement>("[data-work-revision]");
  return node ? JSON.stringify([node.dataset.workUser, node.dataset.workCompany, node.dataset.workAssignment]) : "server";
}
function changed() { return Object.assign(Error("Your current account, role or management source changed. Recover the original request after reviewing this owner; your newer draft is retained."), {code: "WORK_CONTEXT_CHANGED"}); }
function completionUnchanged(completion?: WorkCommandCompletion) { return !completion || completion.form.isConnected && completion.form.getAttribute("data-erpx-edit-version") === completion.formEditVersion; }
function acknowledgement(value: unknown, expected: Mutation["expected"]) {
  if (!object(value) || typeof value.id !== "string" || !value.id.length) return false;
  return expected.kind === "save" ? value.previewToken === expected.token && validManagementImpact(value.impact) && (!expected.id || value.id === expected.id) : value.id === expected.id && value.status === expected.status;
}

/** One mounted native owner retains the first body/key through an unknown outcome.
 * Every invocation captures fresh source/Work authority through ORIGINAL JSON and
 * the refreshed owner snapshot. No cloned response acknowledges a command. */
export function createManagementWorkspaceCommands(current: () => Current | null = () => null, send: typeof workFetch = workFetch) {
  let attempt: Attempt | null = null, inFlight = false;
  function fence() {
    const observed = current(), work = captureWorkCommandContext();
    if (!observed) throw changed();
    const before = {...observed};
    return {before, assertCurrent: () => {work(); const now = current(); if (!now || now.api !== before.api || now.source !== before.source) throw changed();}};
  }
  async function json(url: string, init: RequestInit, assertCurrent: () => void, completion?: WorkCommandCompletion) {
    const response = await send(url, init, completion, "caller"), value: unknown = await response.json();
    assertCurrent();
    if (!response.ok) {
      const error = object(value) && object(value.error) ? value.error : {};
      throw Object.assign(Error(typeof error.message === "string" ? error.message : "Unable to update user management."), {status: response.status, code: error.code});
    }
    if (!object(value) || !("data" in value)) throw Error("The management acknowledgement is incomplete. Recover the original request.");
    return value.data;
  }
  async function read(api: string, assertCurrent: () => void) {
    const data = await json(api + "/management", {cache: "no-store"}, assertCurrent);
    if (!snapshot(data)) throw Error("The current management snapshot is incomplete. Recover the original request.");
    return data;
  }
  async function preview(change: ManagementChange, completion?: WorkCommandCompletion): Promise<ManagementPreview> {
    if (attempt || inFlight) throw Error("Recover the original request before preparing another change. Your draft is retained.");
    const {before, assertCurrent} = fence(), body = JSON.stringify({change});
    inFlight = true;
    try {
      const data = await json(before.api.replace(/\/people\/access$/, "/iam") + "/user-management/preview", {method: "POST", headers: {"Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID()}, body}, assertCurrent, completion);
      if (!object(data) || typeof data.previewToken !== "string" || !data.previewToken || !validManagementImpact(data.impact)) throw Error("The impact preview is incomplete. Your draft is retained.");
      if (current()?.draft !== before.draft || !completionUnchanged(completion)) throw Error("Your draft changed while the preview loaded. Review the newer draft before previewing again.");
      return {change: (JSON.parse(body) as {change: ManagementChange}).change, previewToken: data.previewToken, impact: data.impact, key: crypto.randomUUID(), draft: before.draft, completion};
    } finally {inFlight = false;}
  }
  async function execute() {
    if (!attempt || inFlight) throw Error("The original management request is already running or unavailable.");
    const selected = attempt, {before, assertCurrent} = fence();
    if (selected.owner !== before.api || selected.workOwner !== workOwner()) throw changed();
    inFlight = true;
    try {
      for (; selected.cursor < selected.commands.length; selected.cursor++) {
        const command = selected.commands[selected.cursor], wasUncertain = selected.uncertain;
        selected.uncertain = true;
        try {
          const data = await json(command.url, {method: "POST", headers: {"Content-Type": "application/json", "Idempotency-Key": command.key}, body: command.body}, assertCurrent, selected.completion());
          if (!acknowledgement(data, command.expected)) throw Error("The original management receipt is incomplete. Recover the same request.");
          selected.uncertain = false;
        } catch (cause) {
          const failure = cause as {status?: number; code?: string};
          if (selected.cursor === 0 && !wasUncertain && failure.status && failure.status < 500 && failure.code !== "WORK_CONTEXT_CHANGED") attempt = null;
          throw cause;
        }
      }
      const data = await read(selected.owner, assertCurrent);
      assertCurrent();
      const completion = selected.completion(), unchanged = current()?.draft === selected.draft && (!selected.hadCompletion || !!completion) && completionUnchanged(completion);
      if (attempt !== selected) throw changed();
      attempt = null;
      if (typeof window !== "undefined") {
        window.dispatchEvent(new CustomEvent("erpx:record-command-saved", {detail: completion}));
        announceWorkChange();
      }
      return {data, unchanged};
    } finally {inFlight = false;}
  }
  function begin(commands: Mutation[], draft: number, completion?: WorkCommandCompletion) {
    if (attempt || inFlight) return Promise.reject(Error("Recover the original management request first. Your newer draft is retained."));
    const now = current(); if (!now) return Promise.reject(changed());
    attempt = {owner: now.api, workOwner: workOwner(), commands, cursor: 0, uncertain: false, draft, hadCompletion: !!completion, completion: retainWorkCommandCompletion(completion)};
    return execute();
  }
  return {
    bind: (next: () => Current | null) => {current = next;},
    preview,
    save: (value: ManagementPreview) => {
      const now = current(); if (!now) return Promise.reject(changed());
      return begin([{url: now.api.replace(/\/people\/access$/, "/iam") + "/user-management/save", body: JSON.stringify({change: value.change, previewToken: value.previewToken}), key: value.key, expected: {kind: "save", token: value.previewToken, id: "id" in value.change.input ? value.change.input.id : undefined}}], value.draft, value.completion);
    },
    review: (items: {id: string; version: number}[], decision: "approve" | "reject", draft: number) => {
      if (!items.length) return Promise.reject(Error("Select a current independent review before recording a decision."));
      const now = current(); if (!now) return Promise.reject(changed());
      return begin(items.map(item => ({url: now.api.replace(/\/people\/access$/, "/iam") + "/reviews/" + encodeURIComponent(item.id), body: JSON.stringify({expectedVersion: item.version, decision, reason: "User management: " + decision + " reviewed access"}), key: crypto.randomUUID(), expected: {kind: "review" as const, id: item.id, status: decision === "approve" ? "approved" : "rejected"}})), draft);
    },
    retry: execute,
    hasPending: () => attempt !== null,
    busy: () => inFlight,
  };
}
