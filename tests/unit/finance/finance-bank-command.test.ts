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

/** The real BankMatch closure is rendered with only browser/network ports replaced. */
async function mountedBankMatch() {
  const ts = testRequire("typescript") as typeof import("typescript"),
    source = await readFile(resolve("src/components/finance/bank-controls.tsx"), "utf8"),
    compiled = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX } }).outputText,
    cells: { value: unknown }[] = [],
    router = { refresh: vi.fn() };
  let hook = 0, tree: unknown, currentForm: BankMatchNode | undefined, readScope: (() => FinanceBankScope) | undefined;
  const hooks = {
    useState(initial: unknown) {
      const index = hook++;
      cells[index] ??= { value: typeof initial === "function" ? Reflect.apply(initial, undefined, []) : initial };
      return [cells[index].value, (next: unknown) => { cells[index].value = typeof next === "function" ? Reflect.apply(next, undefined, [cells[index].value]) : next; }];
    },
    useRef(initial: unknown) { const index = hook++; cells[index] ??= { value: { current: initial } }; return cells[index].value; },
    useEffect() { hook++; },
    useLayoutEffect(effect: () => void) { hook++; Reflect.apply(effect, undefined, []); },
    useCallback(callback: (...args: never[]) => unknown) { hook++; return callback; },
  };
  const submit = vi.fn(async (request: FinanceBankRequest, form: HTMLFormElement) => {
    if (!request || !form) throw Error("ACTUAL_BANK_MATCH_SUBMISSION_REQUIRED");
    return { acknowledge: () => ({ unchanged: false, completion: null }) };
  }),
    commandPort = { pending: vi.fn(() => null), submit, retry: vi.fn(async () => ({ acknowledge: () => ({ unchanged: true, completion: null }) })) },
    acknowledge = vi.fn(), announce = vi.fn(),
    loaded: { exports: Record<string, unknown> } = { exports: {} },
    foundation = { InlineNotice: "aside", Button: "button", Field: "label", Select: "select", TextInput: "input", Card: "article", Stack: "div", FormGrid: "div", FormLayout: "form" };
  const ports = (name: string): unknown => {
    if (name === "react") return hooks;
    if (name === "react/jsx-runtime") return testRequire(name);
    if (name.endsWith("/destination-registry")) return { canonicalUiHref: (href: string) => href };
    if (name.endsWith("/ui/foundation")) return foundation;
    if (name.endsWith("/ui/dialog")) return { FormDrawer: "section" };
    if (name.endsWith("/destination-router")) return { useRouter: () => router };
    if (name.endsWith("/finance-money")) return { minorToDecimal: (value: number) => (value / 100).toFixed(2) };
    if (name.endsWith("/bank-command")) return { createFinanceBankCommands: (read: () => FinanceBankScope) => { readScope = read; return commandPort; } };
    if (name.endsWith("/purchase-context")) return { useFinanceActionAllowed: () => true };
    if (name.endsWith("/finance-draft-boundary")) return { acknowledgeFinanceForm: acknowledge, useFinanceDraftClose: () => (action: () => void) => action(), useFinanceDraftScope: () => "bank-test-scope" };
    if (name.endsWith("/work-refresh")) return { announceWorkChange: announce };
    if (name.endsWith("/attachment-panel")) return { AttachmentPanel: "attachment-panel" };
    throw Error("Unexpected actual BankMatch import " + name);
  };
  new Function("require", "module", "exports", compiled)(ports, loaded, loaded.exports);
  const BankMatch = loaded.exports.BankMatch as (props: { base: string; transaction: Record<string, unknown>; payments: Record<string, unknown>[]; canWrite: boolean; canApprove: boolean }) => unknown;
  const props = {
    base: "/holdings/acme",
    transaction: { id: "bankrow-1", version: 4, match_status: "unmatched", currency: "THB", bank_account: "bank-account", amount_minor: 10_000 },
    payments: [{ id: "payment-1", kind: "receipt", currency: "THB", bank_account: "bank-account", amount_minor: 10_000, payment_number: "RC-001", payment_date: "2026-10-18" }],
    canWrite: true,
    canApprove: true,
  };
  const render = () => {
    hook = 0;
    tree = Reflect.apply(BankMatch, undefined, [props]);
    currentForm = bankMatchNodes(tree).find(node => node.type === "form");
    return bankMatchNodes(tree);
  };
  const nodes = () => bankMatchNodes(tree), component = (type: string) => nodes().find(node => node.type === type);
  const first = render(), formNode = first.find(node => node.type === "form"), ref = formNode?.props.ref as { current: HTMLFormElement | null } | undefined;
  if (!ref || !currentForm) throw Error("ACTUAL_BANK_MATCH_FORM_REQUIRED");
  const form = { dispatchEvent(event: Event) { if (event.type === "change") callBankHandler(currentForm?.props.onChangeCapture); return true; } } as unknown as HTMLFormElement;
  ref.current = form;
  const getScope = () => { if (!readScope) throw Error("ACTUAL_BANK_COMMAND_SCOPE_REQUIRED"); return readScope(); };
  return { render, nodes, component, form, scope: getScope, submit, commandPort, router, acknowledge, announce };
}

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
    await callBankHandler(submitForm.props.onSubmit, { preventDefault: vi.fn(), currentTarget: mounted.form });
    expect(mounted.submit).toHaveBeenCalledOnce();
    expect(mounted.submit.mock.calls[0]?.[0]).toEqual(request);
    expect(mounted.submit.mock.calls[0]?.[1]).toBe(mounted.form);
    expect(mounted.router.refresh).not.toHaveBeenCalled();
    expect(mounted.acknowledge).not.toHaveBeenCalled();
    expect(mounted.announce).toHaveBeenCalledOnce();
  } finally {
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
    outcome: { unchanged: boolean; completion: HTMLFormElement | null } = { unchanged: false, completion: null };
  const scheduled: (() => void)[] = [], submit = vi.fn(async () => null),
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
  return { render, nodes, node, form, scope, router, submit, retry, setPending: (request: FinanceBankRequest | null) => { pendingRequest = request; }, setOutcome: (next: boolean) => { outcome = { unchanged: next, completion: null }; } };
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
