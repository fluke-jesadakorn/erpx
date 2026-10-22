import {workFetch, type WorkCommandCompletion} from "@erpx/feature-work_projection/lib/work-fetch";
import {captureWorkCommandContext} from "@erpx/feature-work_projection/lib/command-context";
import {retainWorkCommandCompletion} from "@erpx/feature-work_projection/lib/command-completion";
import {announceWorkChange} from "@erpx/feature-work_projection/lib/work-refresh";
import {validDirectoryLifecyclePreview} from "@erpx/feature-people_payroll/lib/directory-owner-command";
export type OrganizationAccessCommandScope = {base: string; ownerKey: string; source: string; canManage: boolean; draft: number};
type Row = Record<string, unknown>;
type Attempt = {path: string; body: string; key: string; base: string; ownerKey: string; workOwner: string; draft: number; uncertain: boolean; completion: ReturnType<typeof retainWorkCommandCompletion>};
const row = (value: unknown): value is Row => !!value && typeof value === "object" && !Array.isArray(value);
const positive = (value: unknown) => Number.isSafeInteger(value) && Number(value) > 0;
const strings = (value: unknown) => Array.isArray(value) && value.every(item => typeof item === "string");
function owner() {const node = typeof document === "undefined" ? null : document.querySelector<HTMLElement>("[data-work-revision]"); return node ? JSON.stringify([node.dataset.workUser, node.dataset.workCompany, node.dataset.workAssignment]) : "server";}
function changed() {return Object.assign(Error("Your current Organization owner, Company or work role changed. Return to the original owner to recover the request; your draft is retained."), {code: "WORK_CONTEXT_CHANGED"});}
function preview(path: string) {return path.endsWith("/preview") || path.endsWith("/remove-preview");}
function route(path: string) {return /^\/technology\/gpo-rules\/(?:preview|remove-preview|publish|remove|not-required|retry-config)$/.test(path) || path === "/technology/position-gpo/retry" || /^\/people\/organization\/position-moves\/(?:preview|submit|[^/]+\/cancel)$/.test(path) || /^\/people\/lifecycle\/placements\/(?:preview|submit)$/.test(path);}
export function validOrganizationAccessResult(path: string, input: Row, key: string, data: unknown): data is Row {
  if (!row(data)) return false;
  if (path === "/people/lifecycle/placements/preview") return validDirectoryLifecyclePreview(data);
  if (path === "/people/organization/position-moves/preview") return typeof data.previewToken === "string" && !!data.previewToken && positive(data.expiresAt) && typeof data.ready === "boolean" && strings(data.blockers) && Array.isArray(data.people) && data.affectedCount === data.people.length && row(data.before) && row(data.after) && row(data.policyChanges) && data.effectiveAt === input.effectiveAt && data.timezone === input.timezone;
  if (preview(path)) return typeof data.token === "string" && !!data.token && positive(data.expiresAt) && row(data.target) && Array.isArray(data.positions) && Array.isArray(data.holders) && row(data.changes) && [data.changes.add, data.changes.remove, data.changes.retained].every(strings) && data.endpointStatus === "unconfirmed";
  if (path === "/technology/gpo-rules/not-required") return data.positionId === input.positionId && data.version === Number(input.expectedVersion) + 1 && data.status === "not_required";
  if (path === "/technology/gpo-rules/retry-config") return data.id === input.id && data.status === "pending";
  if (path === "/technology/position-gpo/retry") return data.id === input.id && data.version === Number(input.expectedVersion) + 1 && data.status === "queued";
  if (path === "/technology/gpo-rules/publish") return data.id === (input.id ?? `gpo-rule-${key}`) && data.version === Number(input.expectedVersion) + 1 && positive(data.revision) && data.status === "pending" && data.endpointStatus === "unconfirmed";
  if (path === "/technology/gpo-rules/remove") return data.id === input.id && data.version === Number(input.expectedVersion) + 1 && positive(data.revision) && data.active === false && data.queuedForReconciliation === true;
  if (path === "/people/lifecycle/placements/submit") return data.id === (input.id ?? `lifecycle-${key}`) && data.version === Number(input.expectedVersion ?? 0) + 1 && data.status === "scheduled";
  if (path === "/people/organization/position-moves/submit") return data.id === `position-move-${key}` && data.version === 1 && data.status === "scheduled" && data.effectiveAt === input.effectiveAt;
  const id = /^\/people\/organization\/position-moves\/([^/]+)\/cancel$/.exec(path)?.[1];
  return !!id && data.id === decodeURIComponent(id) && data.version === Number(input.expectedVersion) + 1 && data.status === "cancelled";
}
/** Every invocation fences current source/Work through ORIGINAL JSON and explicit completion.
 * A later source image may admit exact history retry, never a different original body/key. */
export function createOrganizationAccessCommands(current: () => OrganizationAccessCommandScope | null, send: typeof workFetch = workFetch, nextKey = () => crypto.randomUUID()) {
  let pending: Attempt | null = null, busy = false;
  const listeners = new Set<() => void>(), emit = () => {for (const listener of listeners) listener();};
  async function execute(attempt: Attempt) {
    if (busy) throw Error("Wait for the current Organization request.");
    const scope = current(); if (!scope?.canManage || scope.base !== attempt.base || scope.ownerKey !== attempt.ownerKey || owner() !== attempt.workOwner) throw changed();
    const captured = {...scope}, work = captureWorkCommandContext(), assertCurrent = () => {work(); const now = current(); if (!now || !now.canManage || now.base !== captured.base || now.ownerKey !== captured.ownerKey || now.source !== captured.source || now.canManage !== captured.canManage) throw changed();};
    const read = preview(attempt.path);
    busy = true; if (!read) {pending = attempt; attempt.uncertain = true;} emit();
    try {
      const response = await send(`/api/v1${attempt.base}${attempt.path}`, {method: "POST", headers: {"Content-Type": "application/json", "Idempotency-Key": attempt.key}, body: attempt.body}, attempt.completion(), "caller");
      const value: unknown = await response.json(); assertCurrent();
      if (!response.ok) {
        const error = row(value) && row(value.error) ? value.error : {};
        // A typed rejection does not attest that the original native receipt is absent.
        // Retain the exact request even on its first HTTP error; recovery never invents a new key.
        throw Object.assign(Error(typeof error.message === "string" ? error.message : "Unable to complete the original Organization request."), {code: error.code});
      }
      const input: unknown = JSON.parse(attempt.body);
      if (!row(input) || !row(value) || !validOrganizationAccessResult(attempt.path, input, attempt.key, value.data)) throw Error("The native Organization acknowledgement is incomplete. Recover the original request.");
      const data = value.data, unchanged = () => {const form = attempt.completion(); return current()?.draft === attempt.draft && (!form || form.form.isConnected && form.form.getAttribute("data-erpx-edit-version") === form.formEditVersion);};
      if (read && !unchanged()) throw Error("Your newer Organization draft is retained. Preview it again before continuing.");
      let acknowledged = false;
      return {data, path: attempt.path, input, assertCurrent, unchanged, acknowledge() {assertCurrent(); if (acknowledged || !read && pending !== attempt) return false; if (read && !unchanged()) throw Error("Your newer Organization draft is retained. Preview it again."); acknowledged = true; if (!read) {pending = null; emit(); window.dispatchEvent(new CustomEvent("erpx:record-command-saved", {detail: attempt.completion()})); announceWorkChange();} return true;}};
    } finally {busy = false; emit();}
  }
  return {
    bind(reader: () => OrganizationAccessCommandScope | null) {current = reader;},
    subscribe(listener: () => void) {listeners.add(listener); return () => {listeners.delete(listener);};},
    async send(path: string, input: unknown, completion?: WorkCommandCompletion) {
      if (pending || busy) throw Error("Recover the original Organization request before submitting another body. Your newer draft is retained.");
      const scope = current(); if (!scope?.canManage) throw changed();
      if (!route(path) || !row(input)) throw Error("Unsupported Organization owner request.");
      return execute({path, body: JSON.stringify(input), key: nextKey(), base: scope.base, ownerKey: scope.ownerKey, workOwner: owner(), draft: scope.draft, uncertain: false, completion: retainWorkCommandCompletion(completion)});
    },
    retry: () => pending ? execute(pending) : Promise.reject(Error("No original Organization request needs recovery.")),
    hasPending: () => pending !== null,
    busy: () => busy,
  };
}
