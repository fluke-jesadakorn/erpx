import { afterEach, expect, it, vi } from "vitest";
import { createRequire } from "node:module";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import type { workFetch } from "../../../src/lib/work_projection/work-fetch";
import { createFinanceBankCommands, type FinanceBankRequest, type FinanceBankScope } from "../../../src/lib/finance/bank-command";

const testRequire = createRequire(import.meta.url);

afterEach(() => vi.unstubAllGlobals());

const importRequest: FinanceBankRequest = { kind: "import", input: {
  bankAccount: "bank-account", statementNumber: "ST-100", statementDate: "2026-10-18",
  opening: "100.00", closing: "101.00", sourceHash: "server-computed",
  rows: [{ externalId: "bank-ref-1", transactionDate: "2026-10-18", description: "Receipt", amount: "1.00", currency: "THB" }],
} };

function bankHarness(kind: "import" | "reconcile" = "import", reply: Array<unknown | Error> = []) {
  const node = { isConnected: true, dataset: { workUser: "reviewer", workRevision: "17", workCompany: "company-original", workAssignment: "finance-approver", workStale: "false" } };
  const attrs = new Map<string, string>([["data-erpx-edit-version", "submitted-edit"]]);
  const fields = new Map<string, FormDataEntryValue>([["statementNumber", "ST-100"], ["description-0", "Receipt"]]);
  const form = { isConnected: true, getAttribute: (name: string) => attrs.get(name) ?? null, setAttribute: (name: string, value: string) => attrs.set(name, value) } as unknown as HTMLFormElement;
  class CapturedFormData {
    private readonly values: [string, FormDataEntryValue][];
    constructor(target?: HTMLFormElement) { this.values = target === form ? [...fields] : []; }
    [Symbol.iterator](): IterableIterator<[string, FormDataEntryValue]> { return this.values[Symbol.iterator](); }
  }
  const browser = Object.assign(new EventTarget(), { location: { origin: "https://finance-dev-erpx.jesadakorn.com", pathname: "/holdings/acme/finance/bank", search: "?chart=primary" } });
  vi.stubGlobal("window", browser);
  vi.stubGlobal("document", { querySelector: (selector: string) => selector === "[data-work-revision]" ? node : null });
  vi.stubGlobal("FormData", CapturedFormData);
  const stored = new Map<string, string>();
  const storage = { getItem: (key: string) => stored.get(key) ?? null, setItem: (key: string, value: string) => { stored.set(key, value); }, removeItem: (key: string) => { stored.delete(key); } };
  const calls: { url: string; body: string; key: string; completion: unknown; mode: unknown }[] = [];
  const send: typeof workFetch = vi.fn(async (...args: Parameters<typeof workFetch>) => {
    const [url, init, completion, mode] = args;
    calls.push({ url: String(url), body: String(init?.body), key: new Headers(init?.headers).get("Idempotency-Key") ?? "", completion, mode });
    const next = reply.shift();
    if (next instanceof Error) throw next;
    return Response.json({ data: next });
  });
  let draft = 1, request: FinanceBankRequest | null = kind === "import" ? importRequest : {
    kind: "reconcile", id: "bankrow-1", input: { expectedVersion: 4, matchedType: "payment", matchedId: "payment-1", proof: "Reviewed original bank receipt", attachmentSetIds: [] },
  };
  const scope: FinanceBankScope = {
    base: "/holdings/acme", ownerKey: kind === "import" ? "statement-import" : "transaction:bankrow-1", source: "original-bank-source",
    draft: () => draft, draftRequest: () => request, canStart: true, canRecover: true,
  };
  let serial = 0;
  const commands = createFinanceBankCommands(() => scope, send, () => storage, () => `bank-key-${++serial}`);
  return { commands, calls, stored, fields, attrs, form, node, scope, setDraft: (value: number) => { draft = value; }, setRequest: (value: FinanceBankRequest | null) => { request = value; }, reply };
}

it("recovers an uncertain import with its exact body and key, and clears only after a parsed native receipt", async () => {
  const c = bankHarness("import", [new Error("The native import committed but its response was lost"), { id: "statement-bank-key-1", status: "imported", imported: 1 }]);
  await expect(c.commands.submit(importRequest, c.form)).rejects.toThrow("response was lost");
  const original = c.commands.pending();
  expect(original).toMatchObject({ request: importRequest, uncertain: true, key: "bank-key-1" });
  const changed: FinanceBankRequest = { ...importRequest, input: { ...importRequest.input, statementNumber: "ST-101" } };
  await expect(c.commands.submit(changed, c.form)).rejects.toThrow("Recover the original Bank request");
  expect(c.calls).toHaveLength(1);

  const recovered = await c.commands.retry(c.form);
  expect(recovered.data).toEqual({ id: "statement-bank-key-1", status: "imported", imported: 1 });
  expect(c.calls).toHaveLength(2);
  expect(c.calls.map(call => call.url)).toEqual(Array(2).fill("/api/v1/holdings/acme/finance/bank/statements/import"));
  expect(c.calls.map(call => call.body)).toEqual(Array(2).fill(JSON.stringify(importRequest.input)));
  expect(new Set(c.calls.map(call => call.key))).toEqual(new Set(["bank-key-1"]));
  expect(c.calls.every(call => call.mode === "caller")).toBe(true);
  expect(recovered.acknowledge()).toMatchObject({ unchanged: true, completion: { form: c.form, formEditVersion: "submitted-edit" } });
  expect(c.commands.pending()).toBeNull();
  expect(c.stored.size).toBe(0);
});

it("retains the exact request and newer form fields when an OK response lacks the matching native receipt", async () => {
  const c = bankHarness("import", [{ id: "statement-other", status: "imported", imported: 1 }, { id: "statement-bank-key-1", status: "imported", imported: 1 }]);
  await expect(c.commands.submit(importRequest, c.form)).rejects.toThrow("native Bank receipt");
  const original = c.commands.pending();
  expect(original?.key).toBe("bank-key-1");
  c.attrs.set("data-erpx-edit-version", "newer-edit");
  c.fields.set("statementNumber", "ST-200");
  c.setDraft(2);
  const recovered = await c.commands.retry(c.form);
  expect(recovered.data).toMatchObject({ id: "statement-bank-key-1", status: "imported", imported: 1 });
  expect(recovered.acknowledge()).toMatchObject({ unchanged: false, completion: { form: c.form, formEditVersion: "submitted-edit" } });
  expect(c.fields.get("statementNumber")).toBe("ST-200");
  expect(c.attrs.get("data-erpx-edit-version")).toBe("newer-edit");
  expect(c.calls.map(call => call.body)).toEqual(Array(2).fill(JSON.stringify(importRequest.input)));
  expect(c.calls.map(call => call.key)).toEqual(["bank-key-1", "bank-key-1"]);
  expect(c.commands.pending()).toBeNull();
});

it("recovers only the same reconciliation movement at its parsed next version", async () => {
  const c = bankHarness("reconcile", [new Error("Lost match receipt"), { id: "bankrow-1", status: "matched", version: 5 }]);
  const request = c.commands.pending();
  expect(request).toBeNull();
  const input: FinanceBankRequest = { kind: "reconcile", id: "bankrow-1", input: { expectedVersion: 4, matchedType: "payment", matchedId: "payment-1", proof: "Reviewed original bank receipt", attachmentSetIds: [] } };
  await expect(c.commands.submit(input, c.form)).rejects.toThrow("Lost match receipt");
  const recovered = await c.commands.retry(c.form);
  expect(recovered.data).toEqual({ id: "bankrow-1", status: "matched", version: 5 });
  expect(recovered.acknowledge()).toMatchObject({ unchanged: true });
  expect(c.calls.map(call => call.url)).toEqual(Array(2).fill("/api/v1/holdings/acme/finance/bank/transactions/bankrow-1/reconcile"));
  expect(c.calls.map(call => call.body)).toEqual(Array(2).fill(JSON.stringify(input.input)));
  expect(c.calls.map(call => call.key)).toEqual(["bank-key-1", "bank-key-1"]);
});

it("keeps a newer BankMatch draft when the uncertain original movement is replayed", async () => {
  const c = bankHarness("reconcile", [new Error("Lost match receipt"), { id: "bankrow-1", status: "matched", version: 5 }]);
  const original: FinanceBankRequest = { kind: "reconcile", id: "bankrow-1", input: { expectedVersion: 4, matchedType: "payment", matchedId: "payment-1", proof: "Original reviewed bank receipt", attachmentSetIds: ["original-workpaper"] } };
  await expect(c.commands.submit(original, c.form)).rejects.toThrow("Lost match receipt");

  const newer: FinanceBankRequest = { kind: "reconcile", id: "bankrow-1", input: { expectedVersion: 4, matchedType: "payment", matchedId: "payment-2", proof: "Newer unsent review evidence", attachmentSetIds: ["newer-workpaper"] } };
  c.attrs.set("data-erpx-edit-version", "newer-match-edit");
  c.fields.set("payment", "payment-2"); c.fields.set("proof", "Newer unsent review evidence"); c.fields.set("attachmentSet", "newer-workpaper");
  c.setDraft(2); c.setRequest(newer);
  const recovered = await c.commands.retry(c.form);

  expect(recovered.acknowledge()).toMatchObject({ unchanged: false, completion: { form: c.form, formEditVersion: "submitted-edit" } });
  expect(c.fields.get("payment")).toBe("payment-2");
  expect(c.fields.get("proof")).toBe("Newer unsent review evidence");
  expect(c.fields.get("attachmentSet")).toBe("newer-workpaper");
  expect(c.attrs.get("data-erpx-edit-version")).toBe("newer-match-edit");
  expect(c.calls.map(call => call.url)).toEqual(Array(2).fill("/api/v1/holdings/acme/finance/bank/transactions/bankrow-1/reconcile"));
  expect(c.calls.map(call => call.body)).toEqual(Array(2).fill(JSON.stringify(original.input)));
  expect(c.calls.map(call => call.key)).toEqual(["bank-key-1", "bank-key-1"]);
  expect(c.commands.pending()).toBeNull();
});

it("quarantines a retained request when the current Company differs, then permits original-context recovery", async () => {
  const c = bankHarness("import", [new Error("Lost after import"), { id: "statement-bank-key-1", status: "imported", imported: 1 }]);
  await expect(c.commands.submit(importRequest, c.form)).rejects.toThrow("Lost after import");
  c.node.dataset.workCompany = "company-other";
  await expect(c.commands.retry(c.form)).rejects.toThrow("retained Bank request");
  expect(c.calls).toHaveLength(1);
  c.node.dataset.workCompany = "company-original";
  expect((await c.commands.retry(c.form)).acknowledge()).toMatchObject({ unchanged: true });
  expect(c.calls).toHaveLength(2);
  expect(c.calls[0].body).toBe(c.calls[1].body);
  expect(c.calls[0].key).toBe(c.calls[1].key);
});

type BankMatchNode = { type: unknown; props: Record<string, unknown> };
function bankMatchNodes(value: unknown): BankMatchNode[] {
  if (Array.isArray(value)) return value.flatMap(bankMatchNodes);
  if (!value || typeof value !== "object" || !("props" in value) || !("type" in value)) return [];
  const node = value as BankMatchNode;
  return [node, ...bankMatchNodes(node.props.children)];
}
function callBankHandler(handler: unknown, ...args: unknown[]) {
  if (typeof handler !== "function") throw Error("ACTUAL_BANK_MATCH_HANDLER_REQUIRED");
  return Reflect.apply(handler, undefined, args);
}

/** The real BankMatch and FinanceDraftBoundary closures run with only browser/network ports replaced. */
async function mountedBankMatch(options: { pending?: FinanceBankRequest } = {}) {
  const ts = testRequire("typescript") as typeof import("typescript"),
    contextSwitch = await import("../../../src/lib/work_projection/context-switch"),
    source = await readFile(resolve("src/components/finance/bank-controls.tsx"), "utf8"),
    boundarySource = await readFile(resolve("src/components/finance/finance-draft-boundary.tsx"), "utf8"),
    compilerOptions = { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX },
    compiled = ts.transpileModule(source, { compilerOptions }).outputText,
    compiledBoundary = ts.transpileModule(boundarySource, { compilerOptions }).outputText,
    scopeId = "actual-bank-draft-scope",
    draftContext = { value: undefined as unknown, Provider: Symbol("FinanceDraftScope") },
    router = { refresh: vi.fn() },
    pendingMicrotasks: (() => void)[] = [],
    documentListeners = new Map<string, EventListener>(),
    address = new URL("https://finance-dev-erpx.jesadakorn.com/holdings/acme/finance/bank?match=bankrow-1"),
    browser = Object.assign(new EventTarget(), { location: { origin: address.origin, href: address.href, pathname: address.pathname, search: address.search, hash: address.hash, assign: vi.fn() }, history: { state: {}, pushState: vi.fn() } }),
    documentPort = {
      querySelector: () => null,
      addEventListener: (type: string, listener: EventListener) => { documentListeners.set(type, listener); },
      removeEventListener: (type: string, listener: EventListener) => { if (documentListeners.get(type) === listener) documentListeners.delete(type); },
    };
  class UiElement {
    isConnected = true;
    constructor(readonly tagName: string, private readonly attributes = new Map<string, string>()) {}
    closest(selector: string) { return selector === "form[data-finance-draft]" && this.tagName === "form" && this.getAttribute("data-finance-draft") ? this : selector === "a[href]" && this.tagName === "a" ? this : selector === "button" && this.tagName === "button" ? this : null; }
    getAttribute(name: string) { return this.attributes.get(name) ?? null; }
    setAttribute(name: string, value: string) { this.attributes.set(name, value); }
    hasAttribute(name: string) { return this.attributes.has(name); }
    contains(value: unknown) { return value === this; }
  }
  class UiForm extends UiElement {
    constructor() { super("form", new Map([["data-finance-draft", scopeId]])); }
    dispatchEvent(event: Event) { if (event.type === "change") callBankHandler(currentFormNode?.props.onChangeCapture); return true; }
  }
  class UiCustomEvent<T = unknown> extends Event { readonly detail: T; constructor(type: string, init?: { detail: T }) { super(type); this.detail = init?.detail as T; } }
  vi.stubGlobal("window", browser);
  vi.stubGlobal("document", documentPort);
  vi.stubGlobal("Element", UiElement);
  vi.stubGlobal("CustomEvent", UiCustomEvent);
  vi.stubGlobal("queueMicrotask", (callback: () => void) => { pendingMicrotasks.push(callback); });

  type Effect = { deps: unknown[]; cleanup?: () => void };
  type Controller = { cells: { value: unknown }[]; effects: Map<number, Effect>; callbacks: Map<number, { deps: unknown[]; value: unknown }>; index: number; queued: (() => void)[] };
  let active: Controller | null = null;
  const current = () => { if (!active) throw Error("ACTUAL_BANK_HOOK_RENDER_REQUIRED"); return active; };
  const equalDeps = (a: unknown[], b: unknown[]) => a.length === b.length && a.every((value, index) => value === b[index]);
  const hooks = {
    createContext: () => draftContext,
    useContext: () => { current().index++; return draftContext.value; },
    useId: () => { current().index++; return scopeId; },
    useState(initial: unknown) {
      const owner = current(), index = owner.index++;
      owner.cells[index] ??= { value: typeof initial === "function" ? Reflect.apply(initial, undefined, []) : initial };
      return [owner.cells[index].value, (next: unknown) => { owner.cells[index].value = typeof next === "function" ? Reflect.apply(next, undefined, [owner.cells[index].value]) : next; }];
    },
    useRef(initial: unknown) { const owner = current(), index = owner.index++; owner.cells[index] ??= { value: { current: initial } }; return owner.cells[index].value; },
    useEffect(effect: () => void | (() => void), deps: unknown[] = []) {
      const owner = current(), index = owner.index++, old = owner.effects.get(index);
      if (!old || !equalDeps(deps, old.deps)) owner.queued.push(() => { old?.cleanup?.(); const cleanup = effect(); owner.effects.set(index, { deps, ...(typeof cleanup === "function" ? { cleanup } : {}) }); });
    },
    useLayoutEffect(effect: () => void) { current().index++; Reflect.apply(effect, undefined, []); },
    useCallback(callback: (...args: never[]) => unknown, deps: unknown[] = []) {
      const owner = current(), index = owner.index++, old = owner.callbacks.get(index);
      if (old && equalDeps(deps, old.deps)) return old.value;
      owner.callbacks.set(index, { deps, value: callback }); return callback;
    },
  };
  const node = (type: unknown, props: Record<string, unknown> | null) => {
    const value = { type, props: props ?? {} };
    if (type === draftContext.Provider) draftContext.value = value.props.value;
    return value;
  }, jsxRuntime = { jsx: node, jsxs: node, Fragment: "Fragment" };
  function controller(renderTree: () => unknown) {
    const owner: Controller = { cells: [], effects: new Map(), callbacks: new Map(), index: 0, queued: [] };
    return {
      render() { owner.index = 0; active = owner; let tree: unknown; try { tree = renderTree(); } finally { active = null; } for (const effect of owner.queued.splice(0)) effect(); return tree; },
      close() { for (const effect of owner.effects.values()) effect.cleanup?.(); },
    };
  }
  const boundaryModule: { exports: Record<string, unknown> } = { exports: {} };
  const requireBoundary = (name: string): unknown => {
    if (name === "react") return hooks;
    if (name === "react/jsx-runtime") return jsxRuntime;
    if (name === "next/navigation") return { usePathname: () => address.pathname, useSearchParams: () => new URLSearchParams(address.search) };
    if (name === "@/lib/work_projection/context-switch") return contextSwitch;
    if (name === "@/components/ui/dialog") return { Dialog: "dialog" };
    if (name === "@/components/ui/foundation") return { Button: "button" };
    throw Error("Unexpected FinanceDraftBoundary import " + name);
  };
  new Function("require", "module", "exports", compiledBoundary)(requireBoundary, boundaryModule, boundaryModule.exports);
  const Boundary = boundaryModule.exports.FinanceDraftBoundary as (props: { children: unknown }) => unknown,
    boundaryController = controller(() => Reflect.apply(Boundary, undefined, [{ children: null }]));
  boundaryController.render();

  let currentFormNode: BankMatchNode | undefined, readScope: (() => FinanceBankScope) | undefined, pendingRequest = options.pending ?? null,
    submitOperation: Promise<{ acknowledge: () => { unchanged: boolean; completion: { form: HTMLFormElement; formEditVersion: string | null } | null } }> | null = null,
    retryOperation: Promise<{ acknowledge: () => { unchanged: boolean; completion: { form: HTMLFormElement; formEditVersion: string | null } | null } }> | null = null;
  const submit = vi.fn((request: FinanceBankRequest, form: HTMLFormElement) => {
      if (!request || !form) throw Error("ACTUAL_BANK_MATCH_SUBMISSION_REQUIRED");
      return submitOperation ?? Promise.resolve({ acknowledge: () => ({ unchanged: false, completion: null }) });
    }),
    retry = vi.fn(async (_form?: HTMLFormElement) => { const result = await (retryOperation ?? Promise.resolve({ acknowledge: () => ({ unchanged: false, completion: null }) })); pendingRequest = null; return result; }),
    commandPort = { pending: vi.fn(() => pendingRequest ? { request: pendingRequest } : null), submit, retry },
    acknowledge = vi.fn((completion: { form: HTMLFormElement; formEditVersion: string | null } | null) => Reflect.apply(boundaryModule.exports.acknowledgeFinanceForm as (...args: unknown[]) => unknown, undefined, [completion])),
    announce = vi.fn(), loaded: { exports: Record<string, unknown> } = { exports: {} },
    foundation = { InlineNotice: "aside", Button: "button", Field: "label", Select: "select", TextInput: "input", Card: "article", Stack: "div", FormGrid: "div", FormLayout: "form" };
  const ports = (name: string): unknown => {
    if (name === "react") return hooks;
    if (name === "react/jsx-runtime") return jsxRuntime;
    if (name.endsWith("/destination-registry")) return { canonicalUiHref: (href: string) => href };
    if (name.endsWith("/ui/foundation")) return foundation;
    if (name.endsWith("/ui/dialog")) return { FormDrawer: "section" };
    if (name.endsWith("/destination-router")) return { useRouter: () => router };
    if (name.endsWith("/finance-money")) return { minorToDecimal: (value: number) => (value / 100).toFixed(2) };
    if (name.endsWith("/bank-command")) return { createFinanceBankCommands: (read: () => FinanceBankScope) => { readScope = read; return commandPort; } };
    if (name.endsWith("/purchase-context")) return { useFinanceActionAllowed: () => true };
    if (name.endsWith("/finance-draft-boundary")) return { acknowledgeFinanceForm: acknowledge, captureFinanceForm: () => undefined, useFinanceDraftClose: () => (action: () => void) => action(), useFinanceDraftCommandPending: boundaryModule.exports.useFinanceDraftCommandPending, useFinanceDraftScope: boundaryModule.exports.useFinanceDraftScope };
    if (name.endsWith("/work-refresh")) return { announceWorkChange: announce };
    if (name.endsWith("/attachment-panel")) return { AttachmentPanel: "attachment-panel" };
    throw Error("Unexpected actual BankMatch import " + name);
  };
  new Function("require", "module", "exports", compiled)(ports, loaded, loaded.exports);
  const BankMatch = loaded.exports.BankMatch as (props: { base: string; transaction: Record<string, unknown>; payments: Record<string, unknown>[]; canWrite: boolean; canApprove: boolean }) => unknown,
    props = {
      base: "/holdings/acme",
      transaction: { id: "bankrow-1", version: 4, match_status: "unmatched", currency: "THB", bank_account: "bank-account", amount_minor: 10_000 },
      payments: [
        { id: "payment-1", kind: "receipt", currency: "THB", bank_account: "bank-account", amount_minor: 10_000, payment_number: "RC-001", payment_date: "2026-10-18" },
        { id: "payment-2", kind: "receipt", currency: "THB", bank_account: "bank-account", amount_minor: 10_000, payment_number: "RC-002", payment_date: "2026-10-18" },
      ], canWrite: true, canApprove: true,
    },
    componentController = controller(() => Reflect.apply(BankMatch, undefined, [props]));
  let tree: unknown, currentForm: UiForm | undefined;
  const render = () => {
    tree = componentController.render(); currentFormNode = bankMatchNodes(tree).find(item => item.type === "form");
    if (!currentFormNode) throw Error("ACTUAL_BANK_MATCH_FORM_REQUIRED");
    const ref = currentFormNode.props.ref as { current: HTMLFormElement | null } | undefined;
    if (!ref) throw Error("ACTUAL_BANK_MATCH_FORM_REF_REQUIRED");
    if (!currentForm) currentForm = new UiForm();
    ref.current = currentForm as unknown as HTMLFormElement;
    return bankMatchNodes(tree);
  };
  const renderBoundary = () => bankMatchNodes(boundaryController.render()),
    nodes = () => bankMatchNodes(tree), component = (type: string) => nodes().find(item => item.type === type),
    form = () => { if (!currentForm) throw Error("ACTUAL_BANK_MATCH_FORM_REQUIRED"); return currentForm as unknown as HTMLFormElement; },
    getScope = () => { if (!readScope) throw Error("ACTUAL_BANK_COMMAND_SCOPE_REQUIRED"); return readScope(); },
    flushMicrotasks = () => { for (const callback of pendingMicrotasks.splice(0)) callback(); },
    markInput = () => { const formNode = component("form"); if (!formNode) throw Error("ACTUAL_BANK_MATCH_FORM_REQUIRED"); callBankHandler(formNode.props.onInputCapture); documentListeners.get("input")?.({ target: form() } as unknown as Event); },
    closeLink = new UiElement("a", new Map([["href", "https://finance-dev-erpx.jesadakorn.com/holdings/acme/finance/bank"]])) as UiElement & { href: string; target: string };
  Object.assign(closeLink, { href: "https://finance-dev-erpx.jesadakorn.com/holdings/acme/finance/bank", target: "" });
  const requestDeparture = () => {
    const listener = documentListeners.get("click"); if (!listener) throw Error("ACTUAL_FINANCE_NAVIGATION_GUARD_REQUIRED");
    let prevented = false, stopped = false;
    listener({ defaultPrevented: false, button: 0, metaKey: false, ctrlKey: false, shiftKey: false, altKey: false, target: closeLink,
      preventDefault() { prevented = true; }, stopImmediatePropagation() { stopped = true; } } as unknown as MouseEvent);
    return { prevented, stopped };
  };
  render();
  return {
    render, renderBoundary, nodes, component, form, scope: getScope, submit, retry, commandPort, router, acknowledge, announce,
    setPending(value: FinanceBankRequest | null) { pendingRequest = value; },
    setMatched(value: boolean) { props.transaction.match_status = value ? "matched" : "unmatched"; },
    setSubmitOperation(value: typeof submitOperation) { submitOperation = value; },
    setRetryOperation(value: typeof retryOperation) { retryOperation = value; },
    flushMicrotasks, markInput, hasUnsavedWork: contextSwitch.hasUnsavedWork, contextSwitchBlockReason: contextSwitch.contextSwitchBlockReason, confirmContextSwitch: contextSwitch.confirmContextSwitch, requestDeparture,
    close() { componentController.close(); boundaryController.close(); },
  };
}

it("actual BankMatch blocks context switching while a Bank receipt is uncertain and releases only after original acknowledgement", async () => {
  const mounted = await mountedBankMatch(), original: FinanceBankRequest = { kind: "reconcile", id: "bankrow-1", input: { expectedVersion: 4, matchedType: "payment", matchedId: "payment-1", proof: "Original reviewed evidence", attachmentSetIds: [] } };
  try {
    const payment = mounted.component("select"), proof = mounted.component("input"), form = mounted.component("form");
    if (!payment || !proof || !form) throw Error("ACTUAL_BANK_MATCH_FIELDS_REQUIRED");
    callBankHandler(payment.props.onChange, { target: { value: "payment-1" } });
    callBankHandler(proof.props.onChange, { target: { value: "Original reviewed evidence" } });
    mounted.render();
    mounted.setPending(original);
    mounted.setSubmitOperation(Promise.reject(Error("Lost original Bank receipt after commit")));
    await callBankHandler(mounted.component("form")?.props.onSubmit, { preventDefault: vi.fn(), currentTarget: mounted.form() });
    expect(mounted.contextSwitchBlockReason()).toContain("original Finance request is awaiting confirmation");
    expect(mounted.confirmContextSwitch()).toBe(false);

    mounted.setRetryOperation(Promise.resolve({ acknowledge: () => ({ unchanged: true, completion: { form: mounted.form(), formEditVersion: null } }) }));
    const retry = mounted.render().find(node => node.type === "button" && node.props.children === "Retry original bank match");
    if (!retry) throw Error("ACTUAL_BANK_MATCH_RECOVERY_ACTION_REQUIRED");
    callBankHandler(retry.props.onClick);
    await new Promise(resolve => setTimeout(resolve, 0));
    expect(mounted.contextSwitchBlockReason()).toBeNull();
    expect(mounted.confirmContextSwitch()).toBe(true);
  } finally {
    mounted.close();
    vi.unstubAllGlobals();
  }
});

it("actual BankMatch excludes attachment hydration and status reads from dirty state, tracks evidence mutations, and submits the exact reviewed movement", async () => {
  const mounted = await mountedBankMatch();
  class TargetElement { constructor(private readonly button: { textContent: string; getAttribute(name: string): string | null }) {} closest(selector: string) { return selector === "button" ? this.button : null; } }
  vi.stubGlobal("Element", TargetElement);
  try {
    const panel = mounted.component("attachment-panel"), wrapper = mounted.nodes().find(node => typeof node.props.onClickCapture === "function"), formNode = mounted.component("form");
    if (!panel || !wrapper || !formNode) throw Error("ACTUAL_BANK_MATCH_ATTACHMENT_BOUNDARY_REQUIRED");
    expect(mounted.scope().draft()).toBe(0);
    callBankHandler(panel.props.onChange, { setId: null, links: [], readyCount: 0, requiredCount: 0, bindable: false });
    mounted.render();
    expect(mounted.scope().draft()).toBe(0);
    const currentWrapper = mounted.nodes().find(node => typeof node.props.onClickCapture === "function");
    if (!currentWrapper) throw Error("ACTUAL_BANK_MATCH_ATTACHMENT_WRAPPER_REQUIRED");
    callBankHandler(currentWrapper.props.onClickCapture, { target: new TargetElement({ textContent: "", getAttribute: name => name === "aria-label" ? "Reload status for proof.pdf" : null }) });
    expect(mounted.scope().draft()).toBe(0);
    callBankHandler(currentWrapper.props.onClickCapture, { target: new TargetElement({ textContent: "", getAttribute: name => name === "aria-label" ? "Retry upload for proof.pdf" : null }) });
    expect(mounted.scope().draft()).toBe(1);
    callBankHandler(currentWrapper.props.onDropCapture, { dataTransfer: { files: { length: 0 } } });
    expect(mounted.scope().draft()).toBe(1);
    callBankHandler(currentWrapper.props.onDropCapture, { dataTransfer: { files: { length: 1 } } });
    expect(mounted.scope().draft()).toBe(2);

    let nodes = mounted.render();
    const payment = nodes.find(node => node.type === "select" && node.props.name === "payment"), proof = nodes.find(node => node.type === "input" && node.props.name === "proof"), form = mounted.component("form");
    if (!payment || !proof || !form) throw Error("ACTUAL_BANK_MATCH_FIELDS_REQUIRED");
    callBankHandler(payment.props.onChange, { target: { value: "payment-1" } });
    callBankHandler(proof.props.onChange, { target: { value: "Reviewed original bank workpaper" } });
    callBankHandler(form.props.onInputCapture);
    const currentPanel = mounted.component("attachment-panel");
    if (!currentPanel) throw Error("ACTUAL_BANK_MATCH_PANEL_REQUIRED");
    callBankHandler(currentPanel.props.onChange, { setId: "workpaper-set-7", links: [], readyCount: 1, requiredCount: 0, bindable: true });
    nodes = mounted.render();
    const request: FinanceBankRequest = { kind: "reconcile", id: "bankrow-1", input: { expectedVersion: 4, matchedType: "payment", matchedId: "payment-1", proof: "Reviewed original bank workpaper", attachmentSetIds: ["workpaper-set-7"] } };
    expect(mounted.scope().draftRequest?.()).toEqual(request);
    const submitForm = mounted.component("form");
    if (!submitForm) throw Error("ACTUAL_BANK_MATCH_SUBMIT_REQUIRED");
    await callBankHandler(submitForm.props.onSubmit, { preventDefault: vi.fn(), currentTarget: mounted.form() });
    expect(mounted.submit).toHaveBeenCalledOnce();
    expect(mounted.submit.mock.calls[0]?.[0]).toEqual(request);
    expect(mounted.submit.mock.calls[0]?.[1]).toBe(mounted.form());
    expect(mounted.router.refresh).not.toHaveBeenCalled();
    expect(mounted.acknowledge).not.toHaveBeenCalled();
    expect(mounted.announce).toHaveBeenCalledOnce();
  } finally {
    mounted.close();
    vi.unstubAllGlobals();
  }
});

it("actual BankMatch leaves newer fields intact when pending recovery is discovered after editing", async () => {
  const original: FinanceBankRequest = { kind: "reconcile", id: "bankrow-1", input: { expectedVersion: 4, matchedType: "payment", matchedId: "payment-1", proof: "Original reviewed bank evidence", attachmentSetIds: [] } },
    mounted = await mountedBankMatch({ pending: original });
  try {
    let nodes = mounted.render();
    const payment = nodes.find(node => node.type === "select" && node.props.name === "payment"), proof = nodes.find(node => node.type === "input" && node.props.name === "proof");
    if (!payment || !proof) throw Error("ACTUAL_BANK_MATCH_FIELDS_REQUIRED");
    callBankHandler(payment.props.onChange, { target: { value: "payment-2" } });
    callBankHandler(proof.props.onChange, { target: { value: "Newer unsent evidence" } });
    mounted.markInput();
    expect(mounted.scope().draft()).toBe(1);
    expect(mounted.hasUnsavedWork()).toBe(true);

    mounted.flushMicrotasks();
    nodes = mounted.render(); nodes = mounted.render();
    expect(nodes.some(node => node.type === "aside" && String(node.props.children).includes("edits below remain separate"))).toBe(true);
    expect(mounted.component("select")?.props.value).toBe("payment-2");
    expect(mounted.component("input")?.props.value).toBe("Newer unsent evidence");
    expect(mounted.scope().draftRequest?.()).toMatchObject({ kind: "reconcile", id: "bankrow-1", input: { matchedId: "payment-2", proof: "Newer unsent evidence" } });

    const retryOriginal = nodes.find(node => node.type === "button" && node.props.children === "Retry original bank match");
    if (!retryOriginal) throw Error("ACTUAL_BANK_MATCH_RECOVERY_ACTION_REQUIRED");
    callBankHandler(retryOriginal.props.onClick);
    await new Promise(resolve => setTimeout(resolve, 0));
    nodes = mounted.render();
    expect(mounted.retry).toHaveBeenCalledOnce();
    expect(mounted.retry.mock.calls[0]?.[0]).toBe(mounted.form());
    expect(mounted.component("select")?.props.value).toBe("payment-2");
    expect(mounted.component("input")?.props.value).toBe("Newer unsent evidence");
    expect(nodes.some(node => node.type === "aside" && String(node.props.children).includes("Your newer evidence remains here"))).toBe(true);
    expect(mounted.router.refresh).not.toHaveBeenCalled();
    expect(mounted.hasUnsavedWork()).toBe(true);
  } finally {
    mounted.close();
    vi.unstubAllGlobals();
  }
});

it("actual BankMatch keeps newer evidence visible and recovers the original request after the movement becomes matched", async () => {
  const original: FinanceBankRequest = { kind: "reconcile", id: "bankrow-1", input: { expectedVersion: 4, matchedType: "payment", matchedId: "payment-1", proof: "Original reviewed bank evidence", attachmentSetIds: [] } },
    mounted = await mountedBankMatch({ pending: original });
  try {
    let nodes = mounted.render();
    const payment = nodes.find(node => node.type === "select" && node.props.name === "payment"), proof = nodes.find(node => node.type === "input" && node.props.name === "proof"), panel = mounted.component("attachment-panel");
    if (!payment || !proof || !panel) throw Error("ACTUAL_BANK_MATCH_FIELDS_REQUIRED");
    callBankHandler(payment.props.onChange, { target: { value: "payment-2" } });
    callBankHandler(proof.props.onChange, { target: { value: "Newer unsent evidence" } });
    callBankHandler(panel.props.onChange, { setId: "newer-workpaper-set", links: [], readyCount: 1, requiredCount: 0, bindable: true });
    mounted.markInput();
    mounted.flushMicrotasks();
    mounted.setMatched(true);
    nodes = mounted.render();

    expect(nodes.some(node => node.type === "aside" && String(node.props.children).includes("already marked matched") && String(node.props.children).includes("edits below remain separate"))).toBe(true);
    expect(mounted.component("select")?.props.value).toBe("payment-2");
    expect(mounted.component("input")?.props.value).toBe("Newer unsent evidence");
    expect(mounted.scope().draftRequest?.()).toMatchObject({ kind: "reconcile", id: "bankrow-1", input: { matchedId: "payment-2", proof: "Newer unsent evidence", attachmentSetIds: ["newer-workpaper-set"] } });
    expect(nodes.find(node => node.type === "button" && node.props.children === "Confirm match")?.props.disabled).toBe(true);

    const retryOriginal = nodes.find(node => node.type === "button" && node.props.children === "Retry original bank match");
    if (!retryOriginal) throw Error("ACTUAL_BANK_MATCH_RECOVERY_ACTION_REQUIRED");
    callBankHandler(retryOriginal.props.onClick);
    await new Promise(resolve => setTimeout(resolve, 0));
    nodes = mounted.render();
    expect(mounted.retry).toHaveBeenCalledOnce();
    expect(mounted.retry.mock.calls[0]?.[0]).toBe(mounted.form());
    expect(mounted.component("select")?.props.value).toBe("payment-2");
    expect(mounted.component("input")?.props.value).toBe("Newer unsent evidence");
    expect(nodes.some(node => node.type === "aside" && String(node.props.children).includes("Your newer evidence remains here"))).toBe(true);
    expect(mounted.scope().draftRequest?.()).toMatchObject({ kind: "reconcile", input: { matchedId: "payment-2", proof: "Newer unsent evidence", attachmentSetIds: ["newer-workpaper-set"] } });
    expect(nodes.some(node => node.type === "button" && node.props.children === "Retry original bank match")).toBe(false);
    expect(mounted.router.refresh).not.toHaveBeenCalled();
    expect(mounted.hasUnsavedWork()).toBe(true);
  } finally {
    mounted.close();
    vi.unstubAllGlobals();
  }
});

it("actual BankMatch preserves newer dirty fields after a delayed receipt and guards Close review", async () => {
  const mounted = await mountedBankMatch();
  try {
    let nodes = mounted.render();
    const formNode = mounted.component("form"), payment = mounted.component("select"), proof = mounted.component("input");
    if (!formNode || !payment || !proof) throw Error("ACTUAL_BANK_MATCH_FIELDS_REQUIRED");
    callBankHandler(payment.props.onChange, { target: { value: "payment-1" } });
    callBankHandler(proof.props.onChange, { target: { value: "Original reviewed evidence" } });
    mounted.markInput();
    nodes = mounted.render();
    const submittedFormNode = mounted.component("form");
    if (!submittedFormNode) throw Error("ACTUAL_BANK_MATCH_SUBMIT_REQUIRED");
    const form = mounted.form(), submittedEdit = form.getAttribute("data-erpx-edit-version");
    if (!submittedEdit) throw Error("ACTUAL_BANK_MATCH_SUBMITTED_EDIT_VERSION_REQUIRED");
    const submitted: FinanceBankRequest = { kind: "reconcile", id: "bankrow-1", input: { expectedVersion: 4, matchedType: "payment", matchedId: "payment-1", proof: "Original reviewed evidence", attachmentSetIds: [] } };
    let resolveReceipt!: (value: { acknowledge: () => { unchanged: boolean; completion: { form: HTMLFormElement; formEditVersion: string | null } } }) => void;
    mounted.setSubmitOperation(new Promise(resolve => { resolveReceipt = resolve; }));
    const pendingSubmit = callBankHandler(submittedFormNode.props.onSubmit, { preventDefault: vi.fn(), currentTarget: form }) as Promise<void>;
    expect(mounted.submit).toHaveBeenCalledOnce();
    expect(mounted.submit.mock.calls[0]?.[0]).toEqual(submitted);

    nodes = mounted.render();
    const newerPayment = nodes.find(node => node.type === "select" && node.props.name === "payment"), newerProof = nodes.find(node => node.type === "input" && node.props.name === "proof");
    if (!newerPayment || !newerProof) throw Error("ACTUAL_BANK_MATCH_NEWER_FIELDS_REQUIRED");
    callBankHandler(newerPayment.props.onChange, { target: { value: "payment-2" } });
    callBankHandler(newerProof.props.onChange, { target: { value: "Newer unsent evidence" } });
    mounted.markInput();
    nodes = mounted.render();
    const newerEdit = form.getAttribute("data-erpx-edit-version");
    expect(newerEdit).not.toBe(submittedEdit);
    expect(mounted.hasUnsavedWork()).toBe(true);

    resolveReceipt({ acknowledge: () => ({ unchanged: false, completion: { form, formEditVersion: submittedEdit } }) });
    await pendingSubmit;
    nodes = mounted.render();
    expect(mounted.acknowledge).toHaveBeenCalledExactlyOnceWith({ form, formEditVersion: submittedEdit });
    expect(form.getAttribute("data-erpx-edit-version")).toBe(newerEdit);
    expect(mounted.component("select")?.props.value).toBe("payment-2");
    expect(mounted.component("input")?.props.value).toBe("Newer unsent evidence");
    expect(mounted.scope().draftRequest?.()).toMatchObject({ kind: "reconcile", input: { matchedId: "payment-2", proof: "Newer unsent evidence" } });
    expect(mounted.component("button")?.props.disabled).toBe(true);
    expect(nodes.some(node => node.type === "aside" && String(node.props.children).includes("Your newer evidence remains here"))).toBe(true);
    expect(mounted.router.refresh).not.toHaveBeenCalled();
    expect(mounted.hasUnsavedWork()).toBe(true);

    expect(mounted.requestDeparture()).toMatchObject({ prevented: true, stopped: true });
    const dialog = mounted.renderBoundary().find(node => node.type === "dialog");
    expect(dialog?.props).toMatchObject({ open: true, title: "Leave unsaved Finance work?" });
    expect(mounted.hasUnsavedWork()).toBe(true);
  } finally {
    mounted.close();
    vi.unstubAllGlobals();
  }
});

/** Only the React scheduler and command hook are controlled; BankImport remains the checked-in owner component. */
async function mountedBankImport() {
  const ts = testRequire("typescript") as typeof import("typescript"),
    source = await readFile(resolve("src/components/finance/bank-controls.tsx"), "utf8"),
    ast = ts.createSourceFile("bank-controls.tsx", source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX),
    declaration = ast.statements.find(statement => ts.isFunctionDeclaration(statement) && statement.name?.text === "BankImport");
  if (!declaration) throw Error("ACTUAL_BANK_IMPORT_OWNER_REQUIRED");
  const compiled = ts.transpileModule(declaration.getText(ast), { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.React, jsxFactory: "createElement" } }).outputText,
    cells: { value: unknown }[] = [], effects = new Map<number, { deps: unknown[]; cleanup?: () => void }>(),
    router = { refresh: vi.fn(), push: vi.fn() }, formScope = "bank-import-test-scope";
  let hook = 0, tree: unknown, currentScope: FinanceBankScope | null = null, pendingRequest: FinanceBankRequest | null = null,
    outcome: { unchanged: boolean; completion: HTMLFormElement | null } = { unchanged: false, completion: null },
    submitOperation: Promise<{ unchanged: boolean; completion: { form: HTMLFormElement; formEditVersion: string | null } | null }> | null = null;
  const scheduled: (() => void)[] = [], submit = vi.fn((_request: FinanceBankRequest, _form: HTMLFormElement) => submitOperation ?? Promise.resolve(null)),
    retry = vi.fn(async (form?: HTMLFormElement) => ({ ...outcome, completion: form ?? null }));
  const useState = (initial: unknown) => {
    const index = hook++;
    cells[index] ??= { value: typeof initial === "function" ? Reflect.apply(initial, undefined, []) : initial };
    return [cells[index].value, (next: unknown) => { cells[index].value = typeof next === "function" ? Reflect.apply(next, undefined, [cells[index].value]) : next; }];
  };
  const useRef = (initial: unknown) => { const index = hook++; cells[index] ??= { value: { current: initial } }; return cells[index].value; };
  const useEffect = (effect: () => void | (() => void), deps: unknown[]) => {
    const index = hook++, old = effects.get(index);
    if (!old || deps.some((value, position) => value !== old.deps[position])) scheduled.push(() => {
      old?.cleanup?.(); const cleanup = effect(); effects.set(index, { deps, ...(typeof cleanup === "function" ? { cleanup } : {}) });
    });
  };
  const createElement = (type: unknown, props: Record<string, unknown> | null, ...children: unknown[]) => ({ type, props: { ...props, children: children.length === 1 ? children[0] : children } }),
    componentModule: { exports: Record<string, unknown> } = { exports: {} },
    command = {
      get error() { return ""; }, get notice() { return ""; }, get busy() { return false; }, get pendingRequest() { return pendingRequest; }, get pending() { return pendingRequest !== null; },
      send: submit, retry, refreshPending: vi.fn(),
    },
    useBankCommand = (scope: FinanceBankScope) => { currentScope = scope; return command; },
    components = { FormDrawer: "form-drawer", Button: "button", InlineNotice: "aside", Card: "article", Stack: "div", FormGrid: "div", Field: "label", Select: "select", TextInput: "input" };
  new Function("module", "exports", "createElement", "useRouter", "useFinanceDraftScope", "useFinanceActionAllowed", "useRef", "useState", "useEffect", "useBankCommand", "useFinanceDraftClose", "canonicalUiHref", "FormDrawer", "Button", "InlineNotice", "Card", "Stack", "FormGrid", "Field", "Select", "TextInput", compiled)(
    componentModule, componentModule.exports, createElement, () => router, () => formScope, () => true, useRef, useState, useEffect, useBankCommand,
    (action: () => void) => () => action(), (href: string) => href, components.FormDrawer, components.Button, components.InlineNotice, components.Card, components.Stack, components.FormGrid, components.Field, components.Select, components.TextInput,
  );
  const Component = componentModule.exports.BankImport as (props: { base: string; accounts: Record<string, unknown>[]; initialOpen: boolean; returnHref: string }) => unknown;
  const props = {
    base: "/holdings/acme",
    accounts: [{ id: "bank-account", code: "1001", name: "Operating bank", currency: "THB", base_currency: "THB" }],
    initialOpen: true,
    returnHref: "/holdings/acme/finance/bank?chart=primary",
  };
  const render = () => {
    hook = 0; tree = Reflect.apply(Component, undefined, [props]);
    for (const run of scheduled.splice(0)) run();
    return bankMatchNodes(tree);
  };
  const nodes = () => bankMatchNodes(tree), node = (type: string, name?: string) => nodes().find(item => item.type === type && (!name || item.props.name === name));
  const scope = () => { if (!currentScope) throw Error("ACTUAL_BANK_IMPORT_SCOPE_REQUIRED"); return currentScope; };
  const form = () => {
    const current = node("form"), ref = current?.props.ref as { current: HTMLFormElement | null } | undefined;
    if (!ref) throw Error("ACTUAL_BANK_IMPORT_FORM_REQUIRED");
    const nativeForm = { isConnected: true, dispatchEvent: vi.fn(() => true) } as unknown as HTMLFormElement;
    ref.current = nativeForm; return nativeForm;
  };
  return { render, nodes, node, form, scope, router, submit, retry, setPending: (request: FinanceBankRequest | null) => { pendingRequest = request; }, setOutcome: (next: boolean) => { outcome = { unchanged: next, completion: null }; }, setSubmitOperation: (operation: typeof submitOperation) => { submitOperation = operation; } };
}

it("actual BankImport restores the retained request into untouched controlled fields without dirtying the form", async () => {
  const mounted = await mountedBankImport();
  mounted.render(); mounted.form();
  mounted.setPending(importRequest);
  mounted.render();
  const nodes = mounted.render(), scope = mounted.scope(), number = mounted.node("input", "number"), account = mounted.node("select", "bankAccount");
  expect(account?.props.value).toBe("bank-account");
  expect(number?.props.value).toBe("ST-100");
  expect(scope.draft()).toBe(0);
  expect(scope.draftRequest?.()).toEqual(importRequest);
  expect(nodes.some(node => node.type === "form-drawer" && node.props.open === true)).toBe(true);
  expect(nodes.some(node => node.type === "button" && node.props.children === "Retry original bank import")).toBe(true);
});

it("actual BankImport keeps a newer draft if the uncertain original request is discovered after the user starts editing", async () => {
  const mounted = await mountedBankImport();
  mounted.render(); const form = mounted.form(), select = mounted.node("select", "bankAccount"), number = mounted.node("input", "number"), formNode = mounted.node("form");
  if (!select || !number || !formNode) throw Error("ACTUAL_BANK_IMPORT_FIELDS_REQUIRED");
  callBankHandler(select.props.onChange, { target: { value: "bank-account" } });
  callBankHandler(number.props.onChange, { target: { value: "ST-200" } });
  callBankHandler(formNode.props.onInputCapture);
  expect(mounted.scope().draft()).toBe(1);
  mounted.setPending(importRequest);
  let nodes = mounted.render(); nodes = mounted.render();
  expect(mounted.node("input", "number")?.props.value).toBe("ST-200");
  expect(mounted.scope().draftRequest?.()).toMatchObject({ kind: "import", input: { bankAccount: "bank-account", statementNumber: "ST-200" } });
  expect(nodes.some(node => node.type === "aside" && String(node.props.children).includes("Any edits below remain separate."))).toBe(true);
  const retry = nodes.find(node => node.type === "button" && node.props.children === "Retry original bank import");
  if (!retry) throw Error("ACTUAL_BANK_IMPORT_RECOVERY_ACTION_REQUIRED");
  await callBankHandler(retry.props.onClick);
  nodes = mounted.render();
  expect(mounted.retry).toHaveBeenCalledOnce();
  expect(mounted.retry.mock.calls[0]?.[0]).toBe(form);
  expect(mounted.node("input", "number")?.props.value).toBe("ST-200");
  expect(nodes.some(node => node.type === "form-drawer" && node.props.open === true)).toBe(true);
  expect(mounted.router.refresh).not.toHaveBeenCalled();
  expect(mounted.router.push).not.toHaveBeenCalled();
});

it("actual BankImport retains newer edits after the original acknowledgement arrives", async () => {
  const mounted = await mountedBankImport();
  let resolveReceipt!: (value: { unchanged: boolean; completion: { form: HTMLFormElement; formEditVersion: string | null } | null }) => void;
  mounted.render();
  const form = mounted.form(), account = mounted.node("select", "bankAccount"), number = mounted.node("input", "number"), formNode = mounted.node("form");
  if (!formNode || !account || !number) throw Error("ACTUAL_BANK_IMPORT_FIELDS_REQUIRED");
  callBankHandler(account.props.onChange, { target: { value: "bank-account" } });
  callBankHandler(number.props.onChange, { target: { value: "ST-100" } });
  for (const [name, value] of Object.entries({ date: "2026-10-18", opening: "100.00", closing: "101.00" })) {
    const field = mounted.node("input", name);
    if (!field) throw Error(`ACTUAL_BANK_IMPORT_FIELD_REQUIRED:${name}`);
    callBankHandler(field.props.onChange, { target: { value } });
    mounted.render();
  }
  callBankHandler(mounted.node("form")?.props.onInputCapture);
  const originalEdit = "submitted-edit";
  mounted.setSubmitOperation(new Promise(resolve => { resolveReceipt = resolve; }));
  const pendingSubmit = callBankHandler(mounted.node("form")?.props.onSubmit, { preventDefault: vi.fn(), currentTarget: form }) as Promise<void>;
  expect(mounted.submit).toHaveBeenCalledOnce();
  expect(mounted.submit.mock.calls[0]?.[0]).toMatchObject({ kind: "import", input: { bankAccount: "bank-account", statementNumber: "ST-100", statementDate: "2026-10-18", opening: "100.00", closing: "101.00" } });
  expect(mounted.submit.mock.calls[0]?.[1]).toBe(form);

  const newerNumber = mounted.node("input", "number");
  if (!newerNumber) throw Error("ACTUAL_BANK_IMPORT_NEWER_FIELD_REQUIRED");
  callBankHandler(newerNumber.props.onChange, { target: { value: "ST-200" } });
  mounted.render();
  callBankHandler(mounted.node("form")?.props.onInputCapture);
  resolveReceipt({ unchanged: false, completion: { form, formEditVersion: originalEdit } });
  await pendingSubmit;
  const nodes = mounted.render();
  expect(mounted.node("input", "number")?.props.value).toBe("ST-200");
  expect(mounted.node("select", "bankAccount")?.props.value).toBe("bank-account");
  expect(nodes.some(node => node.type === "aside" && String(node.props.children).includes("Your newer fields remain here for review"))).toBe(true);
  expect(mounted.router.push).not.toHaveBeenCalled();
  expect(mounted.router.refresh).not.toHaveBeenCalled();
});
