import { afterEach, expect, it, vi } from "vitest";
import { createRequire } from "node:module";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import type { workFetch } from "../../../src/lib/work_projection/work-fetch";
import { createFinanceBankCommands, type FinanceBankDraft, type FinanceBankRequest, type FinanceBankScope } from "../../../src/lib/finance/bank-command";

const testRequire = createRequire(import.meta.url);

afterEach(() => vi.unstubAllGlobals());

const importRequest: FinanceBankRequest = { kind: "import", input: {
  bankAccount: "bank-account", statementNumber: "ST-100", statementDate: "2026-10-18",
  opening: "100.00", closing: "101.00", sourceHash: "server-computed",
  rows: [{ externalId: "bank-ref-1", transactionDate: "2026-10-18", description: "Receipt", amount: "1.00", currency: "THB" }],
} };

function bankHarness(kind: "import" | "reconcile" = "import", reply: Array<unknown | Response | Error | Promise<unknown | Response | Error>> = []) {
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
  const stored = new Map<string, string>();let rejectDraftWrites=false;
  const storage = { getItem: (key: string) => stored.get(key) ?? null, setItem: (key: string, value: string) => { if(rejectDraftWrites&&key.endsWith(":draft"))throw new DOMException("Storage quota exceeded","QuotaExceededError");stored.set(key, value); }, removeItem: (key: string) => { stored.delete(key); } };
  const calls: { url: string; body: string; key: string; completion: unknown; mode: unknown }[] = [];
  const send: typeof workFetch = vi.fn(async (...args: Parameters<typeof workFetch>) => {
    const [url, init, completion, mode] = args;
    calls.push({ url: String(url), body: String(init?.body), key: new Headers(init?.headers).get("Idempotency-Key") ?? "", completion, mode });
    const next = await reply.shift();
    if (next instanceof Error) throw next;
    if (next instanceof Response) return next;
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
  const createCommands = () => createFinanceBankCommands(() => scope, send, () => storage, () => `bank-key-${++serial}`);
  const commands = createCommands();
  return { commands, createCommands, calls, stored, fields, attrs, form, node, scope, setDraft: (value: number) => { draft = value; }, setRequest: (value: FinanceBankRequest | null) => { request = value; }, setDraftStorageFailure:(value:boolean)=>{rejectDraftWrites=value;},reply };
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

it("recreates the Bank command manager without replacing its retained request or newer draft", async () => {
  const c = bankHarness("import", [new Error("The native import committed but its response was lost"), { id: "statement-bank-key-1", status: "imported", imported: 1 }]);
  await expect(c.commands.submit(importRequest, c.form)).rejects.toThrow("response was lost");
  const newer: FinanceBankRequest = { ...importRequest, input: { ...importRequest.input, statementNumber: "ST-200" } };
  c.attrs.set("data-erpx-edit-version", "newer-edit");
  c.fields.set("statementNumber", "ST-200");
  c.setDraft(2);
  c.setRequest(newer);

  const reloadedCommands = c.createCommands();
  expect(reloadedCommands.pending()).toMatchObject({ request: importRequest, uncertain: true, key: "bank-key-1" });
  const recovered = await reloadedCommands.retry(c.form);

  const acknowledgement = recovered.acknowledge();
  if (!acknowledgement) throw Error("BANK_IMPORT_REENTRY_ACK_REQUIRED");
  expect(acknowledgement).toMatchObject({ unchanged: false });
  expect(acknowledgement.completion).toBeUndefined();
  expect(c.fields.get("statementNumber")).toBe("ST-200");
  expect(c.attrs.get("data-erpx-edit-version")).toBe("newer-edit");
  expect(c.calls.map(call => call.body)).toEqual(Array(2).fill(JSON.stringify(importRequest.input)));
  expect(c.calls.map(call => call.key)).toEqual(["bank-key-1", "bank-key-1"]);
  expect(reloadedCommands.pending()).toBeNull();
});

it("keeps an uncertain BankImport after parser validation bypasses receipt replay, then recovers the exact request", async () => {
  const validation = Response.json({ error: { code: "VALIDATION_FAILED", message: "The statement is invalid." } }, { status: 422 });
  const c = bankHarness("import", [new Error("The original import outcome was uncertain"), validation, { id: "statement-bank-key-1", status: "imported", imported: 1 }]);
  await expect(c.commands.submit(importRequest, c.form)).rejects.toThrow("outcome was uncertain");

  const newer: FinanceBankRequest = { ...importRequest, input: { ...importRequest.input, statementNumber: "ST-200" } };
  c.attrs.set("data-erpx-edit-version", "newer-edit");
  c.fields.set("statementNumber", "ST-200");
  c.fields.set("description-0", "Newer unsent description");
  c.setDraft(2);
  c.setRequest(newer);

  await expect(c.commands.retry(c.form)).rejects.toMatchObject({ status: 422, code: "VALIDATION_FAILED" });

  expect(c.calls.map(call => call.body)).toEqual(Array(2).fill(JSON.stringify(importRequest.input)));
  expect(c.calls.map(call => call.key)).toEqual(["bank-key-1", "bank-key-1"]);
  expect(c.fields.get("statementNumber")).toBe("ST-200");
  expect(c.fields.get("description-0")).toBe("Newer unsent description");
  expect(c.attrs.get("data-erpx-edit-version")).toBe("newer-edit");
  expect(c.commands.pending()).toMatchObject({ request: importRequest, key: "bank-key-1", uncertain: true });
  expect(c.stored.size).toBe(1);

  const recovered = await c.commands.retry(c.form);
  expect(recovered.data).toEqual({ id: "statement-bank-key-1", status: "imported", imported: 1 });
  expect(recovered.acknowledge()).toMatchObject({ unchanged: false });
  expect(c.calls.map(call => call.body)).toEqual(Array(3).fill(JSON.stringify(importRequest.input)));
  expect(c.calls.map(call => call.key)).toEqual(Array(3).fill("bank-key-1"));
  expect(c.commands.pending()).toBeNull();
  expect(c.stored.size).toBe(0);
});

it("retains a newer Bank form snapshot separately from the immutable uncertain command and binds it to its owner", async () => {
  const c=bankHarness("import",[new Error("The committed import response was lost"),{id:"statement-bank-key-1",status:"imported",imported:1}]);
  await expect(c.commands.submit(importRequest,c.form)).rejects.toThrow("response was lost");
  const newer:FinanceBankDraft={kind:"import",fields:{bankAccount:"bank-account",number:"ST-200",date:"2026-10-19",opening:"101.00",closing:"102.00"},rows:[{externalId:"bank-ref-2",transactionDate:"2026-10-19",description:"Newer unsent statement",amount:"1.00"}]};
  c.commands.saveDraft(newer);
  c.setDraft(2);c.setRequest({...importRequest,input:{...importRequest.input,statementNumber:"ST-200"}});
  expect(c.stored.size).toBe(2);
  expect(c.commands.pending()).toMatchObject({request:importRequest,body:JSON.stringify(importRequest.input),key:"bank-key-1",uncertain:true});
  expect(c.commands.readDraft()).toEqual(newer);

  const reloaded=c.createCommands();
  expect(reloaded.pending()).toMatchObject({request:importRequest,body:JSON.stringify(importRequest.input),key:"bank-key-1",uncertain:true});
  expect(reloaded.readDraft()).toEqual(newer);
  c.node.dataset.workAssignment="finance-reviewer";
  expect(()=>reloaded.readDraft()).toThrow("retained Bank request");
  c.node.dataset.workAssignment="finance-approver";
  const replay=await reloaded.retry(c.form);
  expect(c.calls.map(call=>call.body)).toEqual([JSON.stringify(importRequest.input),JSON.stringify(importRequest.input)]);
  expect(c.calls.map(call=>call.key)).toEqual(["bank-key-1","bank-key-1"]);
  expect(replay.acknowledge()).toMatchObject({unchanged:false});
  expect(reloaded.readDraft()).toBeNull();
  expect(c.stored.size).toBe(0);
});

it("restores the separately retained BankMatch payment and evidence draft after manager recreation", async()=>{
  const c=bankHarness("reconcile",[new Error("The committed match response was lost"),{id:"bankrow-1",status:"matched",version:5}]),original:FinanceBankRequest={kind:"reconcile",id:"bankrow-1",input:{expectedVersion:4,matchedType:"payment",matchedId:"payment-1",proof:"Original evidence",attachmentSetIds:[]}};
  await expect(c.commands.submit(original,c.form)).rejects.toThrow("response was lost");
  const newer:FinanceBankDraft={kind:"reconcile",id:"bankrow-1",selectedPayment:"payment-2",proof:"Newer evidence after the uncertain result"};
  c.commands.saveDraft(newer);c.setDraft(2);c.setRequest({kind:"reconcile",id:"bankrow-1",input:{...original.input,matchedId:"payment-2",proof:newer.proof}});
  const recoveredCommands=c.createCommands();
  expect(recoveredCommands.pending()).toMatchObject({request:original,key:"bank-key-1",uncertain:true});
  expect(recoveredCommands.readDraft()).toEqual(newer);
  const result=await recoveredCommands.retry(c.form);
  expect(c.calls.map(call=>call.body)).toEqual([JSON.stringify(original.input),JSON.stringify(original.input)]);
  expect(c.calls.map(call=>call.key)).toEqual(["bank-key-1","bank-key-1"]);
  expect(result.acknowledge()).toMatchObject({unchanged:false});
  expect(c.stored.size).toBe(0);
});

it("validates a retained Bank draft against its own owner independently of the original command",async()=>{
  const c=bankHarness();await expect(c.commands.submit(importRequest,c.form)).rejects.toThrow();
  const newer:FinanceBankDraft={kind:"import",fields:{bankAccount:"bank-account",number:"ST-200",date:"2026-10-19",opening:"101.00",closing:"102.00"},rows:[{externalId:"bank-ref-2",transactionDate:"2026-10-19",description:"Newer unsent statement",amount:"1.00"}]};
  c.commands.saveDraft(newer);
  const [draftKey,draftRaw]=[...c.stored.entries()].find(([key])=>key.endsWith(":draft"))??[];
  if(!draftKey||!draftRaw)throw Error("BANK_DRAFT_ENVELOPE_REQUIRED");
  const envelope=JSON.parse(draftRaw) as {owner:Record<string,string>};
  c.stored.set(draftKey,JSON.stringify({...envelope,owner:{...envelope.owner,assignmentId:"another-assignment"}}));
  expect(c.commands.pending()).toMatchObject({request:importRequest,uncertain:true,key:"bank-key-1"});
  expect(()=>c.commands.readDraft()).toThrow("retained Bank request");
});

it("does not restore a stale Bank draft when the latest snapshot exceeds storage bounds or quota",async()=>{
  const c=bankHarness();await expect(c.commands.submit(importRequest,c.form)).rejects.toThrow();
  const saved:FinanceBankDraft={kind:"import",fields:{bankAccount:"bank-account",number:"ST-200",date:"2026-10-19",opening:"101.00",closing:"102.00"},rows:[{externalId:"bank-ref-2",transactionDate:"2026-10-19",description:"Most recent smaller snapshot",amount:"1.00"}]};
  c.commands.saveDraft(saved);
  const oversized:FinanceBankDraft={kind:"import",fields:saved.fields,rows:Array.from({length:80},(_,index)=>({externalId:`bank-ref-${index}`,transactionDate:"2026-10-19",description:"x".repeat(2000),amount:"1.00"}))};
  expect(()=>c.commands.saveDraft(oversized)).toThrow("too large to retain safely");
  expect(()=>c.commands.readDraft()).not.toThrow();expect(c.commands.readDraft()).toBeNull();
  c.commands.saveDraft(saved);c.setDraftStorageFailure(true);
  expect(()=>c.commands.saveDraft({...saved,fields:{...saved.fields,number:"ST-300"}})).toThrow("Storage quota exceeded");
  c.setDraftStorageFailure(false);
  expect(c.commands.readDraft()).toBeNull();
});

it("keeps an uncertain BankMatch when pre-replay validation rejects its original proof", async () => {
  const invalid = Response.json({ error: { code: "MATCH_INPUT_INVALID", message: "Select a payment and provide review evidence." } }, { status: 422 });
  const c = bankHarness("reconcile", [new Error("The original reconciliation outcome was uncertain"), invalid, { id: "bankrow-1", status: "matched", version: 5 }]);
  const original: FinanceBankRequest = { kind: "reconcile", id: "bankrow-1", input: {
    expectedVersion: 4, matchedType: "payment", matchedId: "payment-1", proof: "", attachmentSetIds: [],
  } };
  c.setRequest(original);
  await expect(c.commands.submit(original, c.form)).rejects.toThrow("outcome was uncertain");

  const newer: FinanceBankRequest = { ...original, input: { ...original.input, proof: "Newer reviewed evidence" } };
  c.setDraft(2);
  c.setRequest(newer);
  c.fields.set("proof", "Newer reviewed evidence");
  c.attrs.set("data-erpx-edit-version", "newer-match-edit");

  await expect(c.commands.retry(c.form)).rejects.toMatchObject({ status: 422, code: "MATCH_INPUT_INVALID" });
  expect(c.commands.pending()).toMatchObject({ request: original, key: "bank-key-1", uncertain: true });
  expect(c.stored.size).toBe(1);

  const recovered = await c.commands.retry(c.form);
  expect(recovered.data).toEqual({ id: "bankrow-1", status: "matched", version: 5 });
  expect(recovered.acknowledge()).toMatchObject({ unchanged: false });
  expect(c.calls.map(call => call.body)).toEqual(Array(3).fill(JSON.stringify(original.input)));
  expect(c.calls.map(call => call.key)).toEqual(Array(3).fill("bank-key-1"));
  expect(c.fields.get("proof")).toBe("Newer reviewed evidence");
  expect(c.attrs.get("data-erpx-edit-version")).toBe("newer-match-edit");
  expect(c.commands.pending()).toBeNull();
  expect(c.stored.size).toBe(0);
});

it("releases a first BankImport parser rejection so the retained fields can be corrected", async () => {
  const validation = Response.json({ error: { code: "VALIDATION_FAILED", message: "The statement is invalid." } }, { status: 422 }),
    invalid: FinanceBankRequest = { ...importRequest, input: { ...importRequest.input, statementDate: "not-a-date" } },
    c = bankHarness("import", [validation, { id: "statement-bank-key-2", status: "imported", imported: 1 }]);
  await expect(c.commands.submit(invalid, c.form)).rejects.toMatchObject({ status: 422, code: "VALIDATION_FAILED" });

  expect(c.commands.pending()).toBeNull();
  expect(c.stored.size).toBe(0);
  expect(c.fields.get("statementNumber")).toBe("ST-100");

  const corrected: FinanceBankRequest = { ...importRequest, input: { ...importRequest.input, statementNumber: "ST-200" } };
  c.fields.set("statementNumber", "ST-200"); c.setDraft(2); c.setRequest(corrected);
  const accepted = await c.commands.submit(corrected, c.form);
  expect(c.calls.map(call => call.key)).toEqual(["bank-key-1", "bank-key-2"]);
  expect(c.calls.map(call => call.body)).toEqual([JSON.stringify(invalid.input), JSON.stringify(corrected.input)]);
  expect(accepted.acknowledge()).toMatchObject({ unchanged: true });
});

it("releases a first BankMatch validation rejection while keeping the original review fields", async () => {
  const original: FinanceBankRequest = { kind: "reconcile", id: "bankrow-1", input: { expectedVersion: 4, matchedType: "payment", matchedId: "payment-1", proof: " ", attachmentSetIds: [] } },
    invalid = Response.json({ error: { code: "MATCH_INPUT_INVALID", message: "Select a payment and provide review evidence." } }, { status: 422 }),
    c = bankHarness("reconcile", [invalid, { id: "bankrow-1", status: "matched", version: 5 }]);
  c.setRequest(original); c.fields.set("proof", " ");
  await expect(c.commands.submit(original, c.form)).rejects.toMatchObject({ status: 422, code: "MATCH_INPUT_INVALID" });

  expect(c.commands.pending()).toBeNull();
  expect(c.stored.size).toBe(0);
  expect(c.fields.get("proof")).toBe(" ");

  const corrected: FinanceBankRequest = { kind: "reconcile", id: "bankrow-1", input: { ...original.input, matchedId: "payment-1", proof: "Reviewed evidence" } };
  c.fields.set("proof", "Reviewed evidence"); c.setDraft(2); c.setRequest(corrected);
  const accepted = await c.commands.submit(corrected, c.form);
  expect(c.calls.map(call => call.key)).toEqual(["bank-key-1", "bank-key-2"]);
  expect(c.calls.map(call => call.body)).toEqual([JSON.stringify(original.input), JSON.stringify(corrected.input)]);
  expect(accepted.acknowledge()).toMatchObject({ unchanged: true });
});

it.each(["MANUAL_RATE_REQUIRED", "FX_RATE_REQUIRED", "CURRENCY_SCALE_REQUIRED"] as const)("clears an uncertain BankImport after the owner rejects %s before commit", async code => {
  const rejected = Response.json({ error: { code, message: "Correct the statement's functional-currency settings." } }, { status: 422 });
  const c = bankHarness("import", [new Error("The original import outcome was uncertain"), rejected]);
  await expect(c.commands.submit(importRequest, c.form)).rejects.toThrow("outcome was uncertain");
  const newer: FinanceBankRequest = { ...importRequest, input: { ...importRequest.input, statementNumber: "ST-200" } };
  c.attrs.set("data-erpx-edit-version", "newer-edit");
  c.fields.set("statementNumber", "ST-200");
  c.setDraft(2);
  c.setRequest(newer);

  await expect(c.commands.retry(c.form)).rejects.toMatchObject({ status: 422, code });

  expect(c.calls.map(call => call.body)).toEqual(Array(2).fill(JSON.stringify(importRequest.input)));
  expect(c.calls.map(call => call.key)).toEqual(["bank-key-1", "bank-key-1"]);
  expect(c.fields.get("statementNumber")).toBe("ST-200");
  expect(c.attrs.get("data-erpx-edit-version")).toBe("newer-edit");
  expect(c.commands.pending()).toBeNull();
  expect(c.stored.size).toBe(0);
});

it("clears an uncertain BankMatch after exact attachment validation rejects before commit and retains newer fields", async () => {
  const rejected = Response.json({ error: { code: "ATTACHMENT_SET_TARGET", message: "The workpaper belongs to another movement." } }, { status: 422 });
  const c = bankHarness("reconcile", [new Error("The original reconciliation outcome was uncertain"), rejected]);
  const original: FinanceBankRequest = { kind: "reconcile", id: "bankrow-1", input: {
    expectedVersion: 4, matchedType: "payment", matchedId: "payment-1", proof: "Original reviewed workpaper", attachmentSetIds: ["workpaper-original"],
  } };
  await expect(c.commands.submit(original, c.form)).rejects.toThrow("outcome was uncertain");
  const newer: FinanceBankRequest = { ...original, input: { ...original.input, matchedId: "payment-2", proof: "Newer review", attachmentSetIds: ["workpaper-newer"] } };
  c.attrs.set("data-erpx-edit-version", "newer-match-edit");
  c.fields.set("payment", "payment-2");
  c.fields.set("proof", "Newer review");
  c.fields.set("attachmentSet", "workpaper-newer");
  c.setDraft(2);
  c.setRequest(newer);

  await expect(c.commands.retry(c.form)).rejects.toMatchObject({ status: 422, code: "ATTACHMENT_SET_TARGET" });

  expect(c.calls.map(call => call.body)).toEqual(Array(2).fill(JSON.stringify(original.input)));
  expect(c.calls.map(call => call.key)).toEqual(["bank-key-1", "bank-key-1"]);
  expect(c.fields.get("payment")).toBe("payment-2");
  expect(c.fields.get("proof")).toBe("Newer review");
  expect(c.fields.get("attachmentSet")).toBe("workpaper-newer");
  expect(c.attrs.get("data-erpx-edit-version")).toBe("newer-match-edit");
  expect(c.commands.pending()).toBeNull();
  expect(c.stored.size).toBe(0);
});

it("keeps an uncertain Bank request quarantined after a non-allowlisted retry error", async () => {
  const conflict = Response.json({ error: { code: "REPLAY_CONFLICT", message: "The original request is still unresolved." } }, { status: 422 });
  const c = bankHarness("import", [new Error("The original import outcome was uncertain"), conflict]);
  await expect(c.commands.submit(importRequest, c.form)).rejects.toThrow("outcome was uncertain");

  const newer: FinanceBankRequest = { ...importRequest, input: { ...importRequest.input, statementNumber: "ST-200" } };
  c.attrs.set("data-erpx-edit-version", "newer-edit");
  c.fields.set("statementNumber", "ST-200");
  c.setDraft(2);
  c.setRequest(newer);

  await expect(c.commands.retry(c.form)).rejects.toMatchObject({ status: 422, code: "REPLAY_CONFLICT" });

  expect(c.calls.map(call => call.body)).toEqual(Array(2).fill(JSON.stringify(importRequest.input)));
  expect(c.calls.map(call => call.key)).toEqual(["bank-key-1", "bank-key-1"]);
  expect(c.fields.get("statementNumber")).toBe("ST-200");
  expect(c.attrs.get("data-erpx-edit-version")).toBe("newer-edit");
  expect(c.commands.pending()).toMatchObject({ request: importRequest, key: "bank-key-1", uncertain: true });
  expect(c.stored.size).toBe(1);
});

it("keeps BankImport edits made during a delayed exact-request retry", async () => {
  let resolveReceipt!: (value: unknown) => void;
  const delayedReceipt = new Promise<unknown>(resolve => { resolveReceipt = resolve; }),
    c = bankHarness("import", [new Error("The original import receipt was lost"), delayedReceipt]);
  await expect(c.commands.submit(importRequest, c.form)).rejects.toThrow("receipt was lost");

  const recovery = c.commands.retry(c.form);
  const newer: FinanceBankRequest = { ...importRequest, input: { ...importRequest.input, statementNumber: "ST-200" } };
  c.attrs.set("data-erpx-edit-version", "newer-during-retry");
  c.fields.set("statementNumber", "ST-200");
  c.setDraft(2);
  c.setRequest(newer);
  await expect(c.commands.submit(newer, c.form)).rejects.toThrow("Recover the original Bank request");
  expect(c.calls).toHaveLength(2);
  expect(c.commands.pending()).toMatchObject({ request: importRequest, key: "bank-key-1", uncertain: true });

  resolveReceipt({ id: "statement-bank-key-1", status: "imported", imported: 1 });
  const recovered = await recovery;
  expect(recovered.acknowledge()).toMatchObject({ unchanged: false, completion: { form: c.form, formEditVersion: "submitted-edit" } });
  expect(c.fields.get("statementNumber")).toBe("ST-200");
  expect(c.attrs.get("data-erpx-edit-version")).toBe("newer-during-retry");
  expect(c.calls.map(call => call.body)).toEqual(Array(2).fill(JSON.stringify(importRequest.input)));
  expect(c.calls.map(call => call.key)).toEqual(["bank-key-1", "bank-key-1"]);
  expect(c.commands.pending()).toBeNull();
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

it("re-enters BankMatch with a fresh command manager and replays the original while retaining newer edits", async () => {
  const c = bankHarness("reconcile", [new Error("Lost match receipt after commit"), { id: "bankrow-1", status: "matched", version: 5 }]);
  const original: FinanceBankRequest = { kind: "reconcile", id: "bankrow-1", input: { expectedVersion: 4, matchedType: "payment", matchedId: "payment-1", proof: "Original reviewed workpaper", attachmentSetIds: ["original-workpaper"] } };
  await expect(c.commands.submit(original, c.form)).rejects.toThrow("Lost match receipt");

  const newer: FinanceBankRequest = { kind: "reconcile", id: "bankrow-1", input: { expectedVersion: 4, matchedType: "payment", matchedId: "payment-2", proof: "Newer unsent workpaper", attachmentSetIds: ["newer-workpaper"] } };
  c.attrs.set("data-erpx-edit-version", "newer-match-edit");
  c.fields.set("payment", "payment-2"); c.fields.set("proof", "Newer unsent workpaper"); c.fields.set("attachmentSet", "newer-workpaper");
  c.setDraft(2); c.setRequest(newer);

  const reenteredCommands = c.createCommands();
  expect(reenteredCommands.pending()).toMatchObject({ request: original, uncertain: true, key: "bank-key-1" });
  const recovered = await reenteredCommands.retry(c.form);

  const acknowledgement = recovered.acknowledge();
  if (!acknowledgement) throw Error("BANK_MATCH_REENTRY_ACK_REQUIRED");
  expect(acknowledgement).toMatchObject({ unchanged: false });
  expect(acknowledgement.completion).toBeUndefined();
  expect(c.fields.get("payment")).toBe("payment-2");
  expect(c.fields.get("proof")).toBe("Newer unsent workpaper");
  expect(c.fields.get("attachmentSet")).toBe("newer-workpaper");
  expect(c.calls.map(call => call.url)).toEqual(Array(2).fill("/api/v1/holdings/acme/finance/bank/transactions/bankrow-1/reconcile"));
  expect(c.calls.map(call => call.body)).toEqual(Array(2).fill(JSON.stringify(original.input)));
  expect(c.calls.map(call => call.key)).toEqual(["bank-key-1", "bank-key-1"]);
  expect(reenteredCommands.pending()).toBeNull();
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
async function mountedBankMatch(options: { pending?: FinanceBankRequest; deferEffects?: boolean; realCommands?: boolean; replies?: Array<unknown | Response | Error | Promise<unknown | Response | Error>> } = {}) {
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
    nativeQueueMicrotask = globalThis.queueMicrotask.bind(globalThis),
    identityNode = { isConnected: true, dataset: { workUser: "reviewer", workRevision: "17", workCompany: "company-original", workAssignment: "finance-approver", workStale: "false" } },
    browser = Object.assign(new EventTarget(), { location: { origin: address.origin, href: address.href, pathname: address.pathname, search: address.search, hash: address.hash, assign: vi.fn() }, history: { state: {}, pushState: vi.fn() } }),
    documentPort = {
      documentElement: { dataset: {} },
      querySelector: (selector: string) => options.realCommands && selector === "[data-work-revision]" ? identityNode : null,
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
    constructor() { super("form", new Map([["data-finance-draft", scopeId], ["data-erpx-edit-version", "original-edit"]])); }
    dispatchEvent(event: Event) { if (event.type === "change") callBankHandler(currentFormNode?.props.onChangeCapture, { target: this, currentTarget: this }); return true; }
  }
  let currentForm: UiForm | undefined;
  const formValues = new Map<string, FormDataEntryValue>([["payment", ""], ["proof", ""]]),
    replies = [...(options.replies ?? [])],
    calls: { url: string; body: string; key: string; mode: unknown }[] = [],
    stored = new Map<string, string>(),
    storage = { getItem: (key: string) => stored.get(key) ?? null, setItem: (key: string, value: string) => { stored.set(key, value); }, removeItem: (key: string) => { stored.delete(key); } };
  class CapturedFormData {
    private readonly values: [string, FormDataEntryValue][];
    constructor(target?: HTMLFormElement) { this.values = target === currentForm ? [...formValues] : []; }
    [Symbol.iterator](): IterableIterator<[string, FormDataEntryValue]> { return this.values[Symbol.iterator](); }
    keys(): IterableIterator<string> { return this.values.map(([key])=>key)[Symbol.iterator](); }
    get(name:string):FormDataEntryValue|null { return this.values.find(([key])=>key===name)?.[1]??null; }
  }
  const send: typeof workFetch = vi.fn(async (...args: Parameters<typeof workFetch>) => {
    const [url, init, _completion, mode] = args;
    calls.push({ url: String(url), body: String(init?.body), key: new Headers(init?.headers).get("Idempotency-Key") ?? "", mode });
    const next = await replies.shift();
    if (next instanceof Error) throw next;
    if (next instanceof Response) return next;
    return Response.json({ data: next });
  });
  class UiCustomEvent<T = unknown> extends Event { readonly detail: T; constructor(type: string, init?: { detail: T }) { super(type); this.detail = init?.detail as T; } }
  vi.stubGlobal("window", browser);
  vi.stubGlobal("document", documentPort);
  vi.stubGlobal("Element", UiElement);
  vi.stubGlobal("CustomEvent", UiCustomEvent);
  vi.stubGlobal("queueMicrotask", options.realCommands ? nativeQueueMicrotask : (callback: () => void) => { pendingMicrotasks.push(callback); });
  if (options.realCommands) vi.stubGlobal("FormData", CapturedFormData);

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
    useMemo(factory: () => unknown, deps: unknown[] = []) {
      const owner = current(), index = owner.index++, old = owner.callbacks.get(index);
      if (old && equalDeps(deps, old.deps)) return old.value;
      const value = Reflect.apply(factory, undefined, []);
      owner.callbacks.set(index, { deps, value }); return value;
    },
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
  const trackerEvents: { token: object | string; pending: boolean }[] = [];
  const node = (type: unknown, props: Record<string, unknown> | null) => {
    const value = { type, props: props ?? {} };
    if (type === draftContext.Provider) {
      const context = value.props.value as { trackCommandPending(token: object | string, form: HTMLFormElement | undefined, pending: boolean, returnHref?: string): void };
      const track = context.trackCommandPending;
      context.trackCommandPending = (token, form, pending, returnHref) => { trackerEvents.push({ token, pending }); track(token, form, pending, returnHref); };
      draftContext.value = context;
    }
    return value;
  }, jsxRuntime = { jsx: node, jsxs: node, Fragment: "Fragment" };
  function controller(renderTree: () => unknown, deferEffects = false) {
    const owner: Controller = { cells: [], effects: new Map(), callbacks: new Map(), index: 0, queued: [] };
    return {
      render() { owner.index = 0; active = owner; let tree: unknown; try { tree = renderTree(); } finally { active = null; } if (!deferEffects) for (const effect of owner.queued.splice(0)) effect(); return tree; },
      flushFirstEffect() { const effect = owner.queued.shift(); if (effect) effect(); },
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
    actualCommands: ReturnType<typeof createFinanceBankCommands> | null = null,
    submitOperation: Promise<{ acknowledge: () => { unchanged: boolean; completion: { form: HTMLFormElement; formEditVersion: string | null } | null } }> | null = null,
    retryOperation: Promise<{ acknowledge: () => { unchanged: boolean; completion: { form: HTMLFormElement; formEditVersion: string | null } | null } }> | null = null;
  const submit = vi.fn((request: FinanceBankRequest, form: HTMLFormElement) => {
      if (!request || !form) throw Error("ACTUAL_BANK_MATCH_SUBMISSION_REQUIRED");
      return submitOperation ?? Promise.resolve({ acknowledge: () => ({ unchanged: false, completion: null }) });
    }),
    retry = vi.fn(async (form?: HTMLFormElement) => { void form; const result = await (retryOperation ?? Promise.resolve({ acknowledge: () => ({ unchanged: false, completion: null }) })); pendingRequest = null; return result; }),
    commandPort = { pending: vi.fn(() => pendingRequest ? { request: pendingRequest } : null), readDraft:vi.fn(()=>null), saveDraft:vi.fn(()=>{}), clearDraft:vi.fn(), submit, retry },
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
    if (name.endsWith("/bank-command")) return { createFinanceBankCommands: (read: () => FinanceBankScope) => {
      readScope = read;
      if (!options.realCommands) return commandPort;
      actualCommands = createFinanceBankCommands(read, send, () => storage, () => "bank-key-1"); return actualCommands;
    } };
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
    };
  let componentController = controller(() => Reflect.apply(BankMatch, undefined, [props]), options.deferEffects ?? false);
  let tree: unknown;
  const render = () => {
    tree = componentController.render(); currentFormNode = bankMatchNodes(tree).find(item => item.type === "form");
    if (!currentFormNode) throw Error("ACTUAL_BANK_MATCH_FORM_REQUIRED");
    const ref = currentFormNode.props.ref as { current: HTMLFormElement | null } | undefined;
    if (!ref) throw Error("ACTUAL_BANK_MATCH_FORM_REF_REQUIRED");
    if (!currentForm) currentForm = new UiForm();
    ref.current = currentForm as unknown as HTMLFormElement;
    return bankMatchNodes(tree);
  };
  const remount = () => {
    componentController.close();
    if (currentForm) currentForm.isConnected = false;
    currentForm = undefined;
    componentController = controller(() => Reflect.apply(BankMatch, undefined, [props]), options.deferEffects ?? false);
    return render();
  };
  const renderBoundary = () => bankMatchNodes(boundaryController.render()),
    nodes = () => bankMatchNodes(tree), component = (type: string) => nodes().find(item => item.type === type),
    form = () => { if (!currentForm) throw Error("ACTUAL_BANK_MATCH_FORM_REQUIRED"); return currentForm as unknown as HTMLFormElement; },
    getScope = () => { if (!readScope) throw Error("ACTUAL_BANK_COMMAND_SCOPE_REQUIRED"); return readScope(); },
    flushMicrotasks = () => { for (const callback of pendingMicrotasks.splice(0)) callback(); },
    markInput = () => { const formNode = component("form"); if (!formNode) throw Error("ACTUAL_BANK_MATCH_FORM_REQUIRED"); if (typeof formNode.props.onInputCapture === "function") callBankHandler(formNode.props.onInputCapture, { target: form(), currentTarget: form() }); documentListeners.get("input")?.({ target: form() } as unknown as Event); },
    setFormField = (name: string, value: string) => { formValues.set(name, value); },
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
    render, remount, renderBoundary, nodes, component, form, scope: getScope, submit, retry, commandPort, router, acknowledge, announce,
    commands: () => actualCommands, calls, stored, trackerEvents, setFormField, currentHref:()=>browser.location.href, assigned:()=>browser.location.assign,
    setSwitching:(value:boolean)=>contextSwitch.setContextSwitching(value),
    setOwner(base: string, transactionId: string, companyId: string) {
      props.base = base;
      props.transaction = { ...props.transaction, id: transactionId };
      identityNode.dataset.workCompany = companyId;
      identityNode.dataset.workRevision = companyId==="company-original"?"17":"18";
      const next=new URL(`${base}/finance/bank?match=${encodeURIComponent(transactionId)}`,address.origin);
      address.pathname=next.pathname;address.search=next.search;browser.location.href=next.href;browser.location.pathname=next.pathname;browser.location.search=next.search;
    },
    setPending(value: FinanceBankRequest | null) { pendingRequest = value; },
    setMatched(value: boolean) { props.transaction.match_status = value ? "matched" : "unmatched"; },
    setPayments(value: typeof props.payments) { props.payments = value; },
    setSubmitOperation(value: typeof submitOperation) { submitOperation = value; },
    setRetryOperation(value: typeof retryOperation) { retryOperation = value; },
    flushEffects: () => componentController.flushFirstEffect(), flushMicrotasks, markInput, hasUnsavedWork: contextSwitch.hasUnsavedWork, contextSwitchBlockReason: contextSwitch.contextSwitchBlockReason, confirmContextSwitch: contextSwitch.confirmContextSwitch, requestDeparture,
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

it("retains an uncertain BankMatch boundary while another owner is selected and returns to the exact original request", async () => {
  const mounted = await mountedBankMatch({ realCommands: true, replies: [
    Error("The committed Bank match response was lost"),
    Response.json({ data: { id: "bankrow-1", status: "matched", version: 5 } }),
  ] }), originalHref=mounted.currentHref();
  try {
    await new Promise(resolve=>setTimeout(resolve,0));
    let nodes=mounted.render();
    const payment=nodes.find(node=>node.type==="select"&&node.props.name==="payment"),proof=nodes.find(node=>node.type==="input"&&node.props.name==="proof"),form=mounted.component("form");
    if(!payment||!proof||!form)throw Error("ACTUAL_BANK_MATCH_FIELDS_REQUIRED");
    callBankHandler(payment.props.onChange,{target:{value:"payment-1"}});mounted.setFormField("payment","payment-1");
    callBankHandler(proof.props.onChange,{target:{value:"Original owner evidence"}});mounted.setFormField("proof","Original owner evidence");mounted.markInput();
    nodes=mounted.render();const original=mounted.scope().draftRequest?.();if(!original)throw Error("ACTUAL_BANK_MATCH_REQUEST_REQUIRED");
    await callBankHandler(mounted.component("form")?.props.onSubmit,{preventDefault:vi.fn(),currentTarget:mounted.form()});
    const submitted=mounted.calls[0];expect(submitted?.body).toBe(JSON.stringify(original.input));expect(submitted?.key).toBe("bank-key-1");
    expect(mounted.commands()?.pending()).toMatchObject({request:original,key:"bank-key-1",uncertain:true});
    const ownerPendingIndex=mounted.trackerEvents.findIndex(event=>event.pending),ownerToken=mounted.trackerEvents[ownerPendingIndex]?.token;if(typeof ownerToken!=="string")throw Error("ACTUAL_BANK_PENDING_OWNER_TOKEN_REQUIRED");

    mounted.setOwner("/holdings/other","bankrow-2","company-other");mounted.render();mounted.renderBoundary();
    expect(mounted.contextSwitchBlockReason()).toContain("original Finance request is awaiting confirmation");
    expect(mounted.trackerEvents.slice(ownerPendingIndex+1).some(event=>event.token===ownerToken&&!event.pending)).toBe(false);
    expect(mounted.requestDeparture()).toEqual({prevented:true,stopped:true});
    const dialog=mounted.renderBoundary().find(node=>node.type==="dialog");
    expect(dialog?.props).toMatchObject({open:true,title:"Confirm pending Finance action"});
    const returnButton=bankMatchNodes(dialog).find(node=>node.type==="button"&&node.props.children==="Return to the original request");
    if(!returnButton)throw Error("FINANCE_PENDING_RETURN_ACTION_REQUIRED");
    callBankHandler(returnButton.props.onClick);
    expect(mounted.assigned()).toHaveBeenCalledWith(originalHref);

    mounted.setSwitching(false);mounted.setOwner("/holdings/acme","bankrow-1","company-original");mounted.render();
    await new Promise(resolve=>setTimeout(resolve,0));nodes=mounted.render();
    const retry=nodes.find(node=>node.type==="button"&&node.props.children==="Retry original bank match");
    if(!retry)throw Error("ACTUAL_BANK_MATCH_RECOVERY_ACTION_REQUIRED");
    callBankHandler(retry.props.onClick);await new Promise(resolve=>setTimeout(resolve,0));
    expect(mounted.calls).toHaveLength(2);
    expect(mounted.calls[1]?.body).toBe(submitted?.body);expect(mounted.calls[1]?.key).toBe(submitted?.key);
    expect(mounted.commands()?.pending()).toBeNull();expect(mounted.stored.size).toBe(0);
    expect(mounted.contextSwitchBlockReason()).toBeNull();
    expect(mounted.trackerEvents.slice(ownerPendingIndex+1).some(event=>event.token===ownerToken&&!event.pending)).toBe(true);
  }finally{mounted.setSwitching(false);mounted.close();vi.unstubAllGlobals();}
});

it("keeps a remounted uncertain BankMatch registered if its owner changes before recovery effects flush", async () => {
  const mounted = await mountedBankMatch({ realCommands: true, replies: [
    Error("The committed Bank match response was lost"),
    Response.json({ data: { id: "bankrow-1", status: "matched", version: 5 } }),
  ] }), originalHref=mounted.currentHref();
  try {
    await new Promise(resolve=>setTimeout(resolve,0));
    let nodes=mounted.render();
    const payment=nodes.find(node=>node.type==="select"&&node.props.name==="payment"),proof=nodes.find(node=>node.type==="input"&&node.props.name==="proof");
    if(!payment||!proof)throw Error("ACTUAL_BANK_MATCH_FIELDS_REQUIRED");
    callBankHandler(payment.props.onChange,{target:{value:"payment-1"}});mounted.setFormField("payment","payment-1");
    callBankHandler(proof.props.onChange,{target:{value:"Original owner evidence"}});mounted.setFormField("proof","Original owner evidence");mounted.markInput();
    nodes=mounted.render();const original=mounted.scope().draftRequest?.();if(!original)throw Error("ACTUAL_BANK_MATCH_REQUEST_REQUIRED");
    await callBankHandler(mounted.component("form")?.props.onSubmit,{preventDefault:vi.fn(),currentTarget:mounted.form()});
    const submitted=mounted.calls[0],ownerPendingIndex=mounted.trackerEvents.findIndex(event=>event.pending),ownerToken=mounted.trackerEvents[ownerPendingIndex]?.token;
    if(typeof ownerToken!=="string")throw Error("ACTUAL_BANK_PENDING_OWNER_TOKEN_REQUIRED");
    expect(submitted?.body).toBe(JSON.stringify(original.input));expect(submitted?.key).toBe("bank-key-1");
    expect(mounted.commands()?.pending()).toMatchObject({request:original,key:"bank-key-1",uncertain:true});

    // A new hook instance does not know the boundary's inherited token yet.
    // Change owners before the remount's passive recovery effect can republish it.
    mounted.remount();
    mounted.setOwner("/holdings/other","bankrow-2","company-other");
    mounted.render();
    expect(mounted.trackerEvents.slice(ownerPendingIndex+1).some(event=>event.token===ownerToken&&!event.pending)).toBe(false);
    expect(mounted.contextSwitchBlockReason()).toContain("original Finance request is awaiting confirmation");
    expect(mounted.requestDeparture()).toEqual({prevented:true,stopped:true});
    const dialog=mounted.renderBoundary().find(node=>node.type==="dialog"),returnButton=bankMatchNodes(dialog).find(node=>node.type==="button"&&node.props.children==="Return to the original request");
    if(!returnButton)throw Error("FINANCE_PENDING_RETURN_ACTION_REQUIRED");
    callBankHandler(returnButton.props.onClick);
    expect(mounted.assigned()).toHaveBeenCalledWith(originalHref);

    mounted.setSwitching(false);mounted.setOwner("/holdings/acme","bankrow-1","company-original");mounted.render();
    await new Promise(resolve=>setTimeout(resolve,0));nodes=mounted.render();
    const retry=nodes.find(node=>node.type==="button"&&node.props.children==="Retry original bank match");
    if(!retry)throw Error("ACTUAL_BANK_MATCH_RECOVERY_ACTION_REQUIRED");
    callBankHandler(retry.props.onClick);await new Promise(resolve=>setTimeout(resolve,0));
    expect(mounted.calls).toHaveLength(2);
    expect(mounted.calls[1]?.body).toBe(submitted?.body);expect(mounted.calls[1]?.key).toBe(submitted?.key);
    expect(mounted.commands()?.pending()).toBeNull();expect(mounted.stored.size).toBe(0);
    expect(mounted.contextSwitchBlockReason()).toBeNull();
    expect(mounted.trackerEvents.slice(ownerPendingIndex+1).some(event=>event.token===ownerToken&&!event.pending)).toBe(true);
  }finally{mounted.setSwitching(false);mounted.close();vi.unstubAllGlobals();}
});

it("actual BankMatch clears the same Finance boundary pending registration after remount recovery", async () => {
  const mounted = await mountedBankMatch({ realCommands: true, replies: [
    Error("The committed Bank match response was lost"),
    Response.json({ data: { id: "bankrow-1", status: "matched", version: 5 } }),
  ] });
  try {
    mounted.flushMicrotasks();
    let nodes = mounted.render();
    const payment = mounted.component("select"), proof = mounted.component("input");
    if (!payment || !proof) throw Error("ACTUAL_BANK_MATCH_FIELDS_REQUIRED");
    callBankHandler(payment.props.onChange, { target: { value: "payment-1" } });
    mounted.setFormField("payment", "payment-1");
    callBankHandler(proof.props.onChange, { target: { value: "Original reviewed evidence" } });
    mounted.setFormField("proof", "Original reviewed evidence");
    mounted.markInput();
    nodes = mounted.render();
    const original = mounted.scope().draftRequest?.();
    if (!original) throw Error("ACTUAL_BANK_MATCH_REQUEST_REQUIRED");
    const formNode = mounted.component("form");
    if (!formNode) throw Error("ACTUAL_BANK_MATCH_FORM_REQUIRED");
    await callBankHandler(formNode.props.onSubmit, { preventDefault: vi.fn(), currentTarget: mounted.form() });

    const firstCommands = mounted.commands();
    if (!firstCommands) throw Error("ACTUAL_BANK_MATCH_REAL_COMMANDS_REQUIRED");
    const originalCall = mounted.calls[0];
    expect(firstCommands.pending()).toMatchObject({ request: original, key: "bank-key-1", uncertain: true });
    expect(mounted.contextSwitchBlockReason()).toContain("original Finance request is awaiting confirmation");

    mounted.remount();
    await new Promise(resolve => setTimeout(resolve, 0));
    nodes = mounted.render(); nodes = mounted.render();
    expect(mounted.component("select")?.props.value).toBe("payment-1");
    expect(mounted.component("input")?.props.value).toBe("Original reviewed evidence");
    const retry = nodes.find(node => node.type === "button" && node.props.children === "Retry original bank match");
    if (!retry) throw Error("ACTUAL_BANK_MATCH_RECOVERY_ACTION_REQUIRED");
    callBankHandler(retry.props.onClick);
    await new Promise(resolve => setTimeout(resolve, 0));

    const recoveredCommands = mounted.commands();
    if (!recoveredCommands) throw Error("ACTUAL_BANK_MATCH_REAL_COMMANDS_REQUIRED");
    expect(mounted.calls).toHaveLength(2);
    expect(mounted.calls[1]?.body).toBe(originalCall?.body);
    expect(mounted.calls[1]?.key).toBe(originalCall?.key);
    expect(JSON.parse(mounted.calls[1]!.body)).toEqual(original.input);
    expect(recoveredCommands.pending()).toBeNull();
    expect(mounted.stored.size).toBe(0);
    expect(mounted.contextSwitchBlockReason()).toBeNull();
    expect(mounted.confirmContextSwitch()).toBe(true);
  } finally {
    mounted.close();
    vi.unstubAllGlobals();
  }
});

it("actual BankMatch tracks the reactive owner identity when base and transaction change while mounted", async () => {
  const mounted = await mountedBankMatch({ realCommands: true, replies: [
    Error("The new owner's committed Bank match response was lost"),
  ] });
  const nextIdentity = 'finance-bank:["/holdings/contoso","transaction:bankrow-2"]';
  try {
    mounted.setOwner("/holdings/contoso", "bankrow-2", "company-contoso");
    mounted.render();
    await new Promise(resolve => setTimeout(resolve, 0));

    const payment = mounted.component("select"), proof = mounted.component("input");
    if (!payment || !proof) throw Error("ACTUAL_BANK_MATCH_FIELDS_REQUIRED");
    callBankHandler(payment.props.onChange, { target: { value: "payment-1" } });
    mounted.setFormField("payment", "payment-1");
    callBankHandler(proof.props.onChange, { target: { value: "Reviewed evidence for the new owner" } });
    mounted.setFormField("proof", "Reviewed evidence for the new owner");
    mounted.markInput();
    const form = mounted.render().find(node => node.type === "form");
    if (!form) throw Error("ACTUAL_BANK_MATCH_FORM_REQUIRED");
    await callBankHandler(form.props.onSubmit, { preventDefault: vi.fn(), currentTarget: mounted.form() });

    expect(mounted.scope()).toMatchObject({ base: "/holdings/contoso", ownerKey: "transaction:bankrow-2" });
    expect(mounted.calls).toHaveLength(1);
    expect(mounted.calls[0]).toMatchObject({
      url: "/api/v1/holdings/contoso/finance/bank/transactions/bankrow-2/reconcile",
      key: "bank-key-1",
    });
    expect(JSON.parse(mounted.calls[0]!.body)).toMatchObject({
      expectedVersion: 4,
      matchedId: "payment-1",
      proof: "Reviewed evidence for the new owner",
    });
    const retained = JSON.parse([...mounted.stored.values()][0]!) as { owner: { base: string; ownerKey: string; companyId: string }; request: FinanceBankRequest };
    expect(retained.owner).toMatchObject({ base: "/holdings/contoso", ownerKey: "transaction:bankrow-2", companyId: "company-contoso" });
    expect(retained.request).toMatchObject({ kind: "reconcile", id: "bankrow-2" });
    expect(mounted.trackerEvents).toContainEqual({ token: nextIdentity, pending: true });
    expect(new Set(mounted.trackerEvents.filter(event => event.pending).map(event => event.token))).toEqual(new Set([nextIdentity]));
    expect(mounted.contextSwitchBlockReason()).toContain("original Finance request is awaiting confirmation");
  } finally {
    mounted.close();
    vi.unstubAllGlobals();
  }
});

it("actual BankMatch clears an existing review before a mounted owner change can submit it", async () => {
  const mounted = await mountedBankMatch({ realCommands: true });
  try {
    mounted.flushMicrotasks();
    const payment = mounted.component("select"), proof = mounted.component("input"), attachment = mounted.component("attachment-panel");
    if (!payment || !proof || !attachment) throw Error("ACTUAL_BANK_MATCH_FIELDS_REQUIRED");
    callBankHandler(payment.props.onChange, { target: { value: "payment-1" } });
    mounted.setFormField("payment", "payment-1");
    callBankHandler(proof.props.onChange, { target: { value: "Evidence reviewed for the original movement" } });
    mounted.setFormField("proof", "Evidence reviewed for the original movement");
    callBankHandler(attachment.props.onChange, { setId: "original-workpaper-set", links: [], readyCount: 1, requiredCount: 0, bindable: true });
    mounted.markInput();
    mounted.render();
    expect(mounted.scope().draftRequest?.()).toMatchObject({
      kind: "reconcile", id: "bankrow-1", input: { matchedId: "payment-1", proof: "Evidence reviewed for the original movement", attachmentSetIds: ["original-workpaper-set"] },
    });
    expect(mounted.hasUnsavedWork()).toBe(true);

    mounted.setOwner("/holdings/contoso", "bankrow-2", "company-contoso");
    mounted.render();
    mounted.render();

    expect(mounted.component("select")?.props.value).toBe("");
    expect(mounted.component("input")?.props.value).toBe("");
    expect(mounted.component("attachment-panel")?.props).toMatchObject({ base: "/holdings/contoso", targetId: "bankrow-2" });
    expect(mounted.scope().draft()).toBe(0);
    expect(mounted.scope().draftRequest?.()).toBeNull();
    expect(mounted.hasUnsavedWork()).toBe(false);

    mounted.setOwner("/holdings/acme", "bankrow-1", "company-original");
    mounted.render();
    mounted.render();
    expect(mounted.component("select")?.props.value).toBe("");
    expect(mounted.component("input")?.props.value).toBe("");
    expect(mounted.component("attachment-panel")?.props).toMatchObject({ base: "/holdings/acme", targetId: "bankrow-1" });
    expect(mounted.scope().draft()).toBe(0);
    expect(mounted.scope().draftRequest?.()).toBeNull();
    expect(mounted.hasUnsavedWork()).toBe(false);

    const form = mounted.component("form");
    if (!form) throw Error("ACTUAL_BANK_MATCH_FORM_REQUIRED");
    await callBankHandler(form.props.onSubmit, { preventDefault: vi.fn(), currentTarget: mounted.form() });
    expect(mounted.calls).toHaveLength(0);
    expect(mounted.stored.size).toBe(0);
  } finally {
    mounted.close();
    vi.unstubAllGlobals();
  }
});

it("actual BankMatch excludes attachment hydration and status reads from dirty state, tracks evidence mutations, and submits the exact reviewed movement", async () => {
  const mounted = await mountedBankMatch();
  class TargetElement { constructor(private readonly button: { textContent: string; getAttribute(name: string): string | null }) {} closest(selector: string) { return selector === "button" ? this.button : null; } }
  class TargetInput { readonly type = "file"; readonly files = { length: 1 }; }
  vi.stubGlobal("Element", TargetElement);
  vi.stubGlobal("HTMLInputElement", TargetInput);
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
    callBankHandler(formNode.props.onChangeCapture, { target: new TargetInput(), currentTarget: mounted.form() });
    expect(mounted.scope().draft()).toBe(3);

    let nodes = mounted.render();
    const payment = nodes.find(node => node.type === "select" && node.props.name === "payment"), proof = nodes.find(node => node.type === "input" && node.props.name === "proof"), form = mounted.component("form");
    if (!payment || !proof || !form) throw Error("ACTUAL_BANK_MATCH_FIELDS_REQUIRED");
    callBankHandler(payment.props.onChange, { target: { value: "payment-1" } });
    callBankHandler(proof.props.onChange, { target: { value: "Reviewed original bank workpaper" } });
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

it("actual BankMatch restores the original review before an immediate retry can mark it as newer evidence", async () => {
  const original: FinanceBankRequest = { kind: "reconcile", id: "bankrow-1", input: { expectedVersion: 4, matchedType: "payment", matchedId: "payment-1", proof: "Original reviewed bank evidence", attachmentSetIds: [] } },
    mounted = await mountedBankMatch({ pending: original, deferEffects: true });
  try {
    const panel = mounted.component("attachment-panel");
    if (!panel) throw Error("ACTUAL_BANK_MATCH_PANEL_REQUIRED");
    callBankHandler(panel.props.onChange, { setId: null, links: [], readyCount: 0, requiredCount: 0, bindable: false });
    expect(mounted.scope().draft()).toBe(0);

    // The test harness leaves passive effects queued to model the interval before
    // first paint. A layout restore must be visible to the immediate retry.
    mounted.flushEffects(); mounted.flushMicrotasks();
    mounted.render();
    const nodes = mounted.render(), payment = nodes.find(node => node.type === "select" && node.props.name === "payment"),
      proof = nodes.find(node => node.type === "input" && node.props.name === "proof");
    expect(payment?.props.value).toBe("payment-1");
    expect(proof?.props.value).toBe("Original reviewed bank evidence");
    expect(mounted.scope().draft()).toBe(0);
    expect(mounted.scope().draftRequest?.()).toEqual(original);

    mounted.setRetryOperation(Promise.resolve({ acknowledge: () => ({ unchanged: true, completion: null }) }));
    const retry = nodes.find(node => node.type === "button" && node.props.children === "Retry original bank match");
    if (!retry) throw Error("ACTUAL_BANK_MATCH_RECOVERY_ACTION_REQUIRED");
    callBankHandler(retry.props.onClick);
    await new Promise(resolve => setTimeout(resolve, 0));

    expect(mounted.retry).toHaveBeenCalledOnce();
    expect(mounted.retry.mock.calls[0]?.[0]).toBe(mounted.form());
    expect(mounted.router.refresh).toHaveBeenCalledOnce();
    expect(mounted.render().some(node => node.type === "aside" && String(node.props.children).includes("Your newer evidence remains here"))).toBe(false);
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
    expect(mounted.scope().draft()).toBe(2);
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
    expect(mounted.acknowledge).not.toHaveBeenCalled();
    expect(mounted.router.refresh).not.toHaveBeenCalled();
    expect(mounted.hasUnsavedWork()).toBe(true);
    expect(mounted.requestDeparture()).toMatchObject({ prevented: true, stopped: true });
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

it("actual BankMatch keeps the newer payment and evidence draft visible after the matched payment leaves the eligible list", async () => {
  const original: FinanceBankRequest = { kind: "reconcile", id: "bankrow-1", input: { expectedVersion: 4, matchedType: "payment", matchedId: "payment-1", proof: "Original reviewed bank evidence", attachmentSetIds: [] } },
    mounted = await mountedBankMatch({ pending: original });
  try {
    const payment = mounted.component("select"), proof = mounted.component("input");
    if (!payment || !proof) throw Error("ACTUAL_BANK_MATCH_FIELDS_REQUIRED");
    callBankHandler(payment.props.onChange, { target: { value: "payment-2" } });
    callBankHandler(proof.props.onChange, { target: { value: "Newer evidence after the original match" } });
    mounted.markInput();
    mounted.flushMicrotasks();
    mounted.setMatched(true);
    mounted.setPayments([]);
    const nodes = mounted.render(), select = mounted.component("select");

    expect(select?.props.value).toBe("payment-2");
    expect(bankMatchNodes(select?.props.children).some(node => node.type === "option" && String(node.props.children).includes("unavailable after match"))).toBe(true);
    expect(mounted.component("input")?.props.value).toBe("Newer evidence after the original match");
    expect(nodes.find(node => node.type === "button" && node.props.children === "Confirm match")?.props.disabled).toBe(true);
    expect(mounted.scope().draftRequest?.()).toMatchObject({ kind: "reconcile", id: "bankrow-1", input: { matchedId: "payment-2", proof: "Newer evidence after the original match" } });

    mounted.setRetryOperation(Promise.resolve({ acknowledge: () => ({ unchanged: false, completion: null }) }));
    const retry = nodes.find(node => node.type === "button" && node.props.children === "Retry original bank match");
    if (!retry) throw Error("ACTUAL_BANK_MATCH_RECOVERY_ACTION_REQUIRED");
    callBankHandler(retry.props.onClick);
    await new Promise(resolve => setTimeout(resolve, 0));
    const confirmedNodes = mounted.render();
    expect(confirmedNodes.some(node => node.type === "aside" && String(node.props.children).includes("The earlier match was saved"))).toBe(true);
    expect(mounted.component("select")?.props.value).toBe("payment-2");
    expect(mounted.component("input")?.props.value).toBe("Newer evidence after the original match");
    expect(confirmedNodes.some(node => node.type === "button" && node.props.children === "Retry original bank match")).toBe(false);
    expect(confirmedNodes.find(node => node.type === "button" && node.props.children === "Confirm match")?.props.disabled).toBe(true);
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

it("actual BankMatch retries the exact movement through the real command hook and keeps newer evidence until its receipt matches", async () => {
  let resolveReceipt!: (value: Response) => void;
  const finalReceipt = new Promise<Response>(resolve => { resolveReceipt = resolve; }),
    mounted = await mountedBankMatch({ realCommands: true, replies: [
      new Error("The committed Bank match response was lost"),
      Response.json({ data: { id: "bankrow-1", status: "matched", version: 6 } }),
      finalReceipt,
    ] });
  try {
    let nodes = mounted.render();
    mounted.flushMicrotasks();
    const form = mounted.form(), payment = mounted.component("select"), proof = mounted.component("input");
    if (!payment || !proof) throw Error("ACTUAL_BANK_MATCH_FIELDS_REQUIRED");
    callBankHandler(payment.props.onChange, { target: { value: "payment-1" } });
    mounted.setFormField("payment", "payment-1");
    callBankHandler(proof.props.onChange, { target: { value: "Original reviewed bank evidence" } });
    mounted.setFormField("proof", "Original reviewed bank evidence");
    mounted.markInput();
    nodes = mounted.render();
    const submitted = mounted.scope().draftRequest?.();
    if (!submitted) throw Error("ACTUAL_BANK_MATCH_REQUEST_REQUIRED");
    const formNode = mounted.component("form");
    if (!formNode) throw Error("ACTUAL_BANK_MATCH_FORM_REQUIRED");
    await callBankHandler(formNode.props.onSubmit, { preventDefault: vi.fn(), currentTarget: form });

    const commands = mounted.commands();
    if (!commands) throw Error("ACTUAL_BANK_MATCH_REAL_COMMANDS_REQUIRED");
    expect(commands.pending()).toMatchObject({ request: submitted, key: "bank-key-1", uncertain: true });
    expect(mounted.calls).toHaveLength(1);
    const originalBody = mounted.calls[0].body;
    expect(JSON.parse(originalBody)).toEqual(submitted.input);
    expect(mounted.calls[0].key).toBe("bank-key-1");
    expect(mounted.stored.size).toBe(1);

    mounted.setMatched(true);
    nodes = mounted.render();
    const newerPayment = mounted.component("select"), newerProof = mounted.component("input");
    if (!newerPayment || !newerProof) throw Error("ACTUAL_BANK_MATCH_NEWER_FIELDS_REQUIRED");
    callBankHandler(newerPayment.props.onChange, { target: { value: "payment-2" } });
    mounted.setFormField("payment", "payment-2");
    callBankHandler(newerProof.props.onChange, { target: { value: "Newer reviewed evidence" } });
    mounted.setFormField("proof", "Newer reviewed evidence");
    mounted.markInput();
    nodes = mounted.render();
    expect(nodes.some(node => node.type === "aside" && String(node.props.children).includes("already marked matched") && String(node.props.children).includes("edits below remain separate"))).toBe(true);
    expect(mounted.component("select")?.props.value).toBe("payment-2");
    expect(mounted.component("input")?.props.value).toBe("Newer reviewed evidence");

    const wrongReceiptRetry = nodes.find(node => node.type === "button" && node.props.children === "Retry original bank match");
    if (!wrongReceiptRetry) throw Error("ACTUAL_BANK_MATCH_RECOVERY_ACTION_REQUIRED");
    callBankHandler(wrongReceiptRetry.props.onClick);
    await new Promise(resolve => setTimeout(resolve, 0));
    mounted.flushMicrotasks();
    nodes = mounted.render();
    expect(mounted.calls).toHaveLength(2);
    expect(mounted.calls.map(call => call.body)).toEqual([originalBody, originalBody]);
    expect(mounted.calls.map(call => call.key)).toEqual(["bank-key-1", "bank-key-1"]);
    expect(commands.pending()).toMatchObject({ request: submitted, key: "bank-key-1", uncertain: true });
    expect(mounted.stored.size).toBe(2);
    expect(mounted.component("select")?.props.value).toBe("payment-2");
    expect(mounted.component("input")?.props.value).toBe("Newer reviewed evidence");
    expect(nodes.some(node => node.type === "aside" && String(node.props.children).includes("native Bank receipt"))).toBe(true);
    expect(mounted.router.refresh).not.toHaveBeenCalled();

    const matchingReceiptRetry = nodes.find(node => node.type === "button" && node.props.children === "Retry original bank match");
    if (!matchingReceiptRetry) throw Error("ACTUAL_BANK_MATCH_RECOVERY_ACTION_REQUIRED");
    callBankHandler(matchingReceiptRetry.props.onClick);
    expect(mounted.calls).toHaveLength(3);
    expect(mounted.calls.map(call => call.body)).toEqual([originalBody, originalBody, originalBody]);
    expect(mounted.calls.map(call => call.key)).toEqual(["bank-key-1", "bank-key-1", "bank-key-1"]);

    const latestProof = mounted.component("input");
    if (!latestProof) throw Error("ACTUAL_BANK_MATCH_LATEST_PROOF_REQUIRED");
    callBankHandler(latestProof.props.onChange, { target: { value: "Newest unsent evidence" } });
    mounted.setFormField("proof", "Newest unsent evidence");
    mounted.markInput();
    mounted.render();
    resolveReceipt(Response.json({ data: { id: "bankrow-1", status: "matched", version: 5 } }));
    await vi.waitFor(() => expect(mounted.acknowledge).toHaveBeenCalled());
    mounted.flushMicrotasks();

    nodes = mounted.render();
    expect(commands.pending()).toBeNull();
    expect(mounted.stored.size).toBe(0);
    expect(mounted.calls.map(call => call.body)).toEqual([originalBody, originalBody, originalBody]);
    expect(mounted.calls.map(call => call.key)).toEqual(["bank-key-1", "bank-key-1", "bank-key-1"]);
    expect(mounted.component("select")?.props.value).toBe("payment-2");
    expect(mounted.component("input")?.props.value).toBe("Newest unsent evidence");
    expect(mounted.component("button")?.props.disabled).toBe(true);
    expect(nodes.some(node => node.type === "aside" && String(node.props.children).includes("Your newer evidence remains here for review"))).toBe(true);
    expect(mounted.router.refresh).not.toHaveBeenCalled();
  } finally {
    mounted.close();
    vi.unstubAllGlobals();
  }
});

it("actual BankMatch persists edits made during the first POST and restores them before exact replay after reload", async () => {
  let loseFirstResponse!: (value: Error) => void;
  const firstResponse = new Promise<Error>(resolve => { loseFirstResponse = resolve; }),
    mounted = await mountedBankMatch({ realCommands: true, replies: [
      firstResponse,
      Response.json({ data: { id: "bankrow-other", status: "matched", version: 5 } }),
    ] });
  try {
    await Promise.resolve();
    let nodes = mounted.render();
    const form = mounted.form(), payment = mounted.component("select"), proof = mounted.component("input");
    if (!payment || !proof) throw Error("ACTUAL_BANK_MATCH_FIELDS_REQUIRED");
    callBankHandler(payment.props.onChange, { target: { value: "payment-1" } });
    mounted.setFormField("payment", "payment-1");
    callBankHandler(proof.props.onChange, { target: { value: "Original reviewed bank evidence" } });
    mounted.setFormField("proof", "Original reviewed bank evidence");
    mounted.markInput();
    nodes = mounted.render();
    const submitted = mounted.scope().draftRequest?.();
    if (!submitted) throw Error("ACTUAL_BANK_MATCH_REQUEST_REQUIRED");
    const firstForm = mounted.component("form");
    if (!firstForm) throw Error("ACTUAL_BANK_MATCH_FORM_REQUIRED");
    const firstSubmit = callBankHandler(firstForm.props.onSubmit, { preventDefault: vi.fn(), currentTarget: form }) as Promise<void>;
    mounted.render();
    expect(mounted.commands()?.pending()).toMatchObject({ request: submitted, key: "bank-key-1", uncertain: true });

    const newerPayment = mounted.component("select"), newerProof = mounted.component("input");
    if (!newerPayment || !newerProof) throw Error("ACTUAL_BANK_MATCH_NEWER_FIELDS_REQUIRED");
    callBankHandler(newerPayment.props.onChange, { target: { value: "payment-2" } });
    mounted.setFormField("payment", "payment-2");
    callBankHandler(newerProof.props.onChange, { target: { value: "Newer reviewed evidence" } });
    mounted.setFormField("proof", "Newer reviewed evidence");
    mounted.markInput();
    nodes = mounted.render();
    expect(mounted.stored.size).toBe(2);

    loseFirstResponse(new Error("The first Bank match response was lost"));
    await firstSubmit;
    mounted.remount();
    await Promise.resolve();
    nodes = mounted.render();
    await Promise.resolve();
    nodes = mounted.render();
    expect(mounted.component("select")?.props.value).toBe("payment-2");
    expect(mounted.component("input")?.props.value).toBe("Newer reviewed evidence");
    expect(mounted.commands()?.pending()).toMatchObject({ request: submitted, key: "bank-key-1", uncertain: true });

    const replay = nodes.find(node => node.type === "button" && node.props.children === "Retry original bank match");
    if (!replay) throw Error("ACTUAL_BANK_MATCH_RECOVERY_ACTION_REQUIRED");
    await callBankHandler(replay.props.onClick);
    expect(mounted.calls).toHaveLength(2);
    expect(mounted.calls.map(call => call.body)).toEqual([JSON.stringify(submitted.input), JSON.stringify(submitted.input)]);
    expect(mounted.calls.map(call => call.key)).toEqual(["bank-key-1", "bank-key-1"]);
    expect(mounted.commands()?.pending()).toMatchObject({ request: submitted, key: "bank-key-1", uncertain: true });
    expect(mounted.stored.size).toBe(2);
  } finally {
    mounted.close();
    vi.unstubAllGlobals();
  }
});

/** Only the React scheduler and command hook are controlled; BankImport remains the checked-in owner component. */
async function mountedBankImport(options: {
  realCommands?: boolean;
  replies?: Array<unknown | Response | Error | Promise<unknown | Response | Error>>;
  storage?:Map<string,string>;
} = {}) {
  const ts = testRequire("typescript") as typeof import("typescript"),
    source = await readFile(resolve("src/components/finance/bank-controls.tsx"), "utf8"),
    ast = ts.createSourceFile("bank-controls.tsx", source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX),
    declaration = ast.statements.find(statement => ts.isFunctionDeclaration(statement) && statement.name?.text === "BankImport"),
    commandDeclaration = ast.statements.find(statement => ts.isFunctionDeclaration(statement) && statement.name?.text === "useBankCommand");
  if (!declaration || options.realCommands && !commandDeclaration) throw Error("ACTUAL_BANK_IMPORT_OWNER_REQUIRED");
  const componentSource = declaration.getText(ast), commandSource = commandDeclaration?.getText(ast),
    compiledSource = options.realCommands
      ? `${commandSource!.replace("function useBankCommand(", "function useBankCommandImpl(")}\n${componentSource.replaceAll("useBankCommand(", "useBankCommandImpl(")}`
      : componentSource,
    compiled = ts.transpileModule(compiledSource, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.React, jsxFactory: "createElement" } }).outputText,
    cells: { value: unknown }[] = [], effects = new Map<number, { deps: unknown[]; cleanup?: () => void }>(),
    callbacks = new Map<number, { deps: unknown[]; value: unknown }>(),
    router = { refresh: vi.fn(), push: vi.fn() }, formScope = "bank-import-test-scope";
  let hook = 0, tree: unknown, currentScope: FinanceBankScope | null = null, pendingRequest: FinanceBankRequest | null = null,
    currentScopeReader: (() => FinanceBankScope) | null = null,
    actualCommands: ReturnType<typeof createFinanceBankCommands> | null = null,
    outcome: { unchanged: boolean; completion: HTMLFormElement | null } = { unchanged: false, completion: null },
    submitOperation: Promise<{ unchanged: boolean; completion: { form: HTMLFormElement; formEditVersion: string | null } | null }> | null = null;
  const replies = [...(options.replies ?? [])],
    calls: { url: string; body: string; key: string; mode: unknown }[] = [],
    stored = options.storage??new Map<string, string>(),
    storage = { getItem: (key: string) => stored.get(key) ?? null, setItem: (key: string, value: string) => { stored.set(key, value); }, removeItem: (key: string) => { stored.delete(key); } },
    identityNode = { isConnected: true, dataset: { workUser: "reviewer", workRevision: "17", workCompany: "company-original", workAssignment: "finance-approver", workStale: "false" } },
    browser = Object.assign(new EventTarget(), { location: { origin: "https://finance-dev-erpx.jesadakorn.com", pathname: "/holdings/acme/finance/bank", search: "?chart=primary" } }),
    formValues = new Map<string, FormDataEntryValue>([["bankAccount", ""], ["number", ""], ["date", ""], ["opening", ""], ["closing", ""], ["externalId-0", ""], ["transactionDate-0", ""], ["description-0", ""], ["amount-0", "0.00"]]);
  let nativeForm: { isConnected: boolean; dispatchEvent: (event: Event) => boolean; getAttribute: (name: string) => string | null; setAttribute: (name: string, value: string) => void } | null = null;
  class CapturedFormData {
    private readonly values: [string, FormDataEntryValue][];
    constructor(target?: HTMLFormElement) { this.values = target === nativeForm ? [...formValues] : []; }
    [Symbol.iterator](): IterableIterator<[string, FormDataEntryValue]> { return this.values[Symbol.iterator](); }
    keys(): IterableIterator<string> { return this.values.map(([key])=>key)[Symbol.iterator](); }
    get(name:string):FormDataEntryValue|null { return this.values.find(([key])=>key===name)?.[1]??null; }
  }
  if (options.realCommands) {
    vi.stubGlobal("window", browser);
    vi.stubGlobal("document", { querySelector: (selector: string) => selector === "[data-work-revision]" ? identityNode : null });
    vi.stubGlobal("FormData", CapturedFormData);
  }
  const send: typeof workFetch = vi.fn(async (...args: Parameters<typeof workFetch>) => {
    const [url, init, _completion, mode] = args;
    calls.push({ url: String(url), body: String(init?.body), key: new Headers(init?.headers).get("Idempotency-Key") ?? "", mode });
    const next = await replies.shift();
    if (next instanceof Error) throw next;
    if (next instanceof Response) return next;
    return Response.json({ data: next });
  });
  const scheduled: (() => void)[] = [], layoutScheduled: (() => void)[] = [], submit = vi.fn((request: FinanceBankRequest, form: HTMLFormElement) => { void request; void form; return submitOperation ?? Promise.resolve(null); }),
    retry = vi.fn(async (form?: HTMLFormElement) => ({ ...outcome, completion: form ?? null }));
  const useState = (initial: unknown) => {
    const index = hook++;
    cells[index] ??= { value: typeof initial === "function" ? Reflect.apply(initial, undefined, []) : initial };
    return [cells[index].value, (next: unknown) => { cells[index].value = typeof next === "function" ? Reflect.apply(next, undefined, [cells[index].value]) : next; }];
  };
  const useRef = (initial: unknown) => { const index = hook++; cells[index] ??= { value: { current: initial } }; return cells[index].value; };
  const registerEffect = (queue: (() => void)[], effect: () => void | (() => void), deps: unknown[]) => {
    const index = hook++, old = effects.get(index);
    if (!old || deps.some((value, position) => value !== old.deps[position])) queue.push(() => {
      old?.cleanup?.(); const cleanup = effect(); effects.set(index, { deps, ...(typeof cleanup === "function" ? { cleanup } : {}) });
    });
  };
  const useEffect = (effect: () => void | (() => void), deps: unknown[]) => registerEffect(scheduled, effect, deps),
    useLayoutEffect = (effect: () => void | (() => void), deps: unknown[]) => registerEffect(layoutScheduled, effect, deps);
  const useCallback = (callback: (...args: never[]) => unknown, deps: unknown[] = []) => {
    const index = hook++, old = callbacks.get(index);
    if (old && deps.length === old.deps.length && deps.every((value, position) => value === old.deps[position])) return old.value;
    callbacks.set(index, { deps, value: callback }); return callback;
  };
  const useMemo = (factory: () => unknown, deps: unknown[] = []) => {
    const index = hook++, old = callbacks.get(index);
    if (old && deps.length === old.deps.length && deps.every((value, position) => value === old.deps[position])) return old.value;
    const value = Reflect.apply(factory, undefined, []);
    callbacks.set(index, { deps, value }); return value;
  };
  const createElement = (type: unknown, props: Record<string, unknown> | null, ...children: unknown[]) => ({ type, props: { ...props, children: children.length === 1 ? children[0] : children } }),
    componentModule: { exports: Record<string, unknown> } = { exports: {} },
    command = {
      get error() { return ""; }, get notice() { return ""; }, get busy() { return false; }, get pendingRequest() { return pendingRequest; }, get pending() { return pendingRequest !== null; },
      send: submit, retry, refreshPending: vi.fn(), readDraft:vi.fn(()=>null),saveDraft:vi.fn(()=>true),clearSavedDraft:vi.fn(),
    },
    useBankCommand = (scope: FinanceBankScope) => { currentScope = scope; return command; },
    financeDraftCommandPending = () => () => {},
    createCommands = (read: () => FinanceBankScope) => { currentScopeReader = read; actualCommands = createFinanceBankCommands(read, send, () => storage, () => "bank-key-1"); return actualCommands; },
    components = { FormDrawer: "form-drawer", Button: "button", InlineNotice: "aside", Card: "article", Stack: "div", FormGrid: "div", Field: "label", Select: "select", TextInput: "input" };
  new Function("module", "exports", "createElement", "useRouter", "useFinanceDraftScope", "useFinanceActionAllowed", "useRef", "useState", "useEffect", "useLayoutEffect", "useBankCommand", "useBankCommandImpl", "useCallback", "useMemo", "createFinanceBankCommands", "useFinanceDraftCommandPending", "captureFinanceForm", "acknowledgeFinanceForm", "announceWorkChange", "useFinanceDraftClose", "canonicalUiHref", "FormDrawer", "Button", "InlineNotice", "Card", "Stack", "FormGrid", "Field", "Select", "TextInput", compiled)(
    componentModule, componentModule.exports, createElement, () => router, () => formScope, () => true, useRef, useState, useEffect, useLayoutEffect, useBankCommand, undefined, useCallback, useMemo, createCommands, financeDraftCommandPending, () => undefined, vi.fn(), vi.fn(),
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
    const layoutRuns = layoutScheduled.splice(0);
    for (const run of layoutRuns) run();
    if (layoutRuns.length) {
      hook = 0; tree = Reflect.apply(Component, undefined, [props]);
      for (const run of layoutScheduled.splice(0)) run();
    }
    return bankMatchNodes(tree);
  };
  const nodes = () => bankMatchNodes(tree), node = (type: string, name?: string) => nodes().find(item => item.type === type && (!name || item.props.name === name));
  const scope = () => { const value = currentScopeReader?.() ?? currentScope; if (!value) throw Error("ACTUAL_BANK_IMPORT_SCOPE_REQUIRED"); return value; };
  const form = () => {
    const current = node("form"), ref = current?.props.ref as { current: HTMLFormElement | null } | undefined;
    if (!ref) throw Error("ACTUAL_BANK_IMPORT_FORM_REQUIRED");
    if (!nativeForm) {
      const attrs = new Map<string, string>([["data-erpx-edit-version", "original-edit"]]);
      nativeForm = { isConnected: true, dispatchEvent: vi.fn(() => true), getAttribute: (name: string) => attrs.get(name) ?? null, setAttribute: (name: string, value: string) => { attrs.set(name, value); } };
    }
    ref.current = nativeForm as unknown as HTMLFormElement; return nativeForm as unknown as HTMLFormElement;
  };
  const setField = (field: BankMatchNode, value: string) => {
    if (typeof field.props.name !== "string") throw Error("ACTUAL_BANK_IMPORT_FIELD_NAME_REQUIRED");
    callBankHandler(field.props.onChange, { target: { value } }); formValues.set(field.props.name, value);
  };
  let editSerial = 0;
  const markInput = () => {
    const formNode = node("form"); if (!formNode) throw Error("ACTUAL_BANK_IMPORT_FORM_REQUIRED");
    callBankHandler(formNode.props.onInputCapture);
    nativeForm?.setAttribute("data-erpx-edit-version", `edited-${++editSerial}`);
  };
  const flush = () => new Promise<void>(resolve => setTimeout(resolve, 0));
  return { render, nodes, node, form, scope, router, submit, retry, setPending: (request: FinanceBankRequest | null) => { pendingRequest = request; }, setOutcome: (next: boolean) => { outcome = { unchanged: next, completion: null }; }, setSubmitOperation: (operation: typeof submitOperation) => { submitOperation = operation; }, commands: () => actualCommands, calls, stored, setField, markInput, setBase:(base:string)=>{props.base=base;props.returnHref=`${base}/finance/bank?chart=primary`;browser.location.pathname=`${base}/finance/bank`;},replies,flushEffects:()=>{for(const run of scheduled.splice(0))run();},flush };
}

it("actual BankImport restores the retained request before an immediate retry can classify it as a newer draft", async () => {
  const mounted = await mountedBankImport();
  mounted.render(); const form = mounted.form();
  mounted.setPending(importRequest);
  mounted.setOutcome(true);
  mounted.render();
  const nodes = mounted.render(), scope = mounted.scope(), number = mounted.node("input", "number"), account = mounted.node("select", "bankAccount");
  expect(account?.props.value).toBe("bank-account");
  expect(number?.props.value).toBe("ST-100");
  expect(scope.draft()).toBe(0);
  expect(scope.draftRequest?.()).toEqual(importRequest);
  expect(nodes.some(node => node.type === "form-drawer" && node.props.open === true)).toBe(true);
  const retry = nodes.find(node => node.type === "button" && node.props.children === "Retry original bank import");
  if (!retry) throw Error("ACTUAL_BANK_IMPORT_RECOVERY_ACTION_REQUIRED");
  await callBankHandler(retry.props.onClick);
  expect(mounted.retry).toHaveBeenCalledOnce();
  expect(mounted.retry.mock.calls[0]?.[0]).toBe(form);
  expect(mounted.router.push).toHaveBeenCalledOnce();
  expect(nodes.some(node => node.type === "aside" && String(node.props.children).includes("Your newer fields remain here for review"))).toBe(false);
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

it("does not apply a queued BankImport draft after its holding owner changes",async()=>{
  const seeded=await mountedBankImport({realCommands:true,replies:[new Error("The original Bank import response was lost")]});
  try{
    seeded.render();const form=seeded.form();
    for(const [type,name,value] of [
      ["select","bankAccount","bank-account"],["input","number","ST-100"],["input","date","2026-10-18"],
      ["input","opening","100.00"],["input","closing","101.00"],["input","externalId-0","bank-ref-1"],
      ["input","transactionDate-0","2026-10-18"],["input","amount-0","1.00"],
    ]){const field=seeded.node(type,name);if(!field)throw Error(`ACTUAL_BANK_IMPORT_FIELD_REQUIRED:${name}`);seeded.setField(field,value);}
    seeded.markInput();seeded.render();
    await callBankHandler(seeded.node("form")?.props.onSubmit,{preventDefault:vi.fn(),currentTarget:form});
    await seeded.flush();seeded.render();
    const commands=seeded.commands();if(!commands?.pending())throw Error("ACTUAL_BANK_IMPORT_PENDING_COMMAND_REQUIRED");
    const latestNumber=seeded.node("input","number");if(!latestNumber)throw Error("ACTUAL_BANK_IMPORT_NEWER_FIELD_REQUIRED");
    seeded.setField(latestNumber,"ST-200");seeded.markInput();seeded.render();
    expect(seeded.stored.size).toBe(2);

    const reloaded=await mountedBankImport({realCommands:true,storage:seeded.stored});
    reloaded.render();
    // The layout effect schedules restoration. Change owner before queued state
    // setters run, matching a reused dynamic Holding route.
    reloaded.setBase("/holdings/other");reloaded.render();
    await Promise.resolve();reloaded.render();
    expect(reloaded.node("select","bankAccount")?.props.value).toBe("");
    expect(reloaded.node("input","number")?.props.value).toBe("");
  }finally{vi.unstubAllGlobals();}
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

it("actual BankImport persists edits made during the first POST and restores them before exact replay after reload", async () => {
  let loseFirstResponse!: (value: Error) => void;
  const firstResponse = new Promise<Error>(resolve => { loseFirstResponse = resolve; }),
    mounted = await mountedBankImport({ realCommands: true, replies: [firstResponse] });
  try {
    mounted.render();
    mounted.flushEffects();
    await mounted.flush();
    mounted.render();
    const form = mounted.form();
    for (const [type, name, value] of [
      ["select", "bankAccount", "bank-account"],
      ["input", "number", "ST-100"],
      ["input", "date", "2026-10-18"],
      ["input", "opening", "100.00"],
      ["input", "closing", "101.00"],
    ]) {
      const field = mounted.node(type, name);
      if (!field) throw Error(`ACTUAL_BANK_IMPORT_FIELD_REQUIRED:${name}`);
      mounted.setField(field, value);
    }
    mounted.markInput();
    mounted.render();
    const submitted = mounted.scope().draftRequest?.();
    if (!submitted || submitted.kind !== "import") throw Error("ACTUAL_BANK_IMPORT_REQUEST_REQUIRED");
    const firstForm = mounted.node("form");
    if (!firstForm) throw Error("ACTUAL_BANK_IMPORT_FORM_REQUIRED");
    const firstSubmit = callBankHandler(firstForm.props.onSubmit, { preventDefault: vi.fn(), currentTarget: form }) as Promise<void>;
    mounted.render();

    const inFlightCommand = mounted.commands()?.pending();
    expect(inFlightCommand).toMatchObject({ request: submitted, key: "bank-key-1", uncertain: true });
    const newerNumber = mounted.node("input", "number");
    if (!newerNumber) throw Error("ACTUAL_BANK_IMPORT_NEWER_FIELD_REQUIRED");
    mounted.setField(newerNumber, "ST-200");
    mounted.markInput();
    mounted.render();
    expect(mounted.stored.size).toBe(2);

    loseFirstResponse(new Error("The first Bank import response was lost"));
    await firstSubmit;
    expect(mounted.commands()?.pending()).toMatchObject({ request: submitted, key: "bank-key-1", uncertain: true });

    const reloaded = await mountedBankImport({
      realCommands: true,
      storage: mounted.stored,
      replies: [Response.json({ data: { id: "statement-other", status: "imported", imported: submitted.input.rows.length } })],
    });
    reloaded.render();
    reloaded.flushEffects();
    await reloaded.flush();
    reloaded.render();
    await Promise.resolve();
    reloaded.render();
    expect(reloaded.node("input", "number")?.props.value).toBe("ST-200");
    expect(reloaded.commands()?.pending()).toMatchObject({ request: submitted, key: "bank-key-1", uncertain: true });

    const replay = reloaded.nodes().find(node => node.type === "button" && node.props.children === "Retry original bank import");
    if (!replay) throw Error("ACTUAL_BANK_IMPORT_RECOVERY_ACTION_REQUIRED");
    await callBankHandler(replay.props.onClick);
    expect(mounted.calls).toHaveLength(1);
    expect(reloaded.calls).toHaveLength(1);
    expect(mounted.calls[0]?.body).toBe(JSON.stringify(submitted.input));
    expect(reloaded.calls[0]?.body).toBe(mounted.calls[0]?.body);
    expect(reloaded.calls[0]?.key).toBe(mounted.calls[0]?.key);
    expect(reloaded.calls[0]?.key).toBe("bank-key-1");
    expect(reloaded.commands()?.pending()).toMatchObject({ request: submitted, key: "bank-key-1", uncertain: true });
    expect(reloaded.stored.size).toBe(2);
  } finally {
    vi.unstubAllGlobals();
  }
});

it("actual BankImport retries the exact request through the real command hook and clears only on its matching receipt", async () => {
  let resolveReceipt!: (value: Response) => void;
  const finalReceipt = new Promise<Response>(resolve => { resolveReceipt = resolve; }),
    mounted = await mountedBankImport({ realCommands: true, replies: [
      new Error("The committed Bank import response was lost"),
      Response.json({ data: { id: "statement-other", status: "imported", imported: 1 } }),
      finalReceipt,
    ] });
  try {
    mounted.render();
    const form = mounted.form();
    for (const [type, name, value] of [
      ["select", "bankAccount", "bank-account"],
      ["input", "number", "ST-100"],
      ["input", "date", "2026-10-18"],
      ["input", "opening", "100.00"],
      ["input", "closing", "101.00"],
    ]) {
      const field = mounted.node(type, name);
      if (!field) throw Error(`ACTUAL_BANK_IMPORT_FIELD_REQUIRED:${name}`);
      mounted.setField(field, value);
    }
    mounted.markInput();
    mounted.render();
    const submitted = mounted.scope().draftRequest?.();
    if (!submitted || submitted.kind !== "import") throw Error("ACTUAL_BANK_IMPORT_REQUEST_REQUIRED");
    const initialForm = mounted.node("form");
    if (!initialForm) throw Error("ACTUAL_BANK_IMPORT_FORM_REQUIRED");
    await callBankHandler(initialForm.props.onSubmit, { preventDefault: vi.fn(), currentTarget: form });
    mounted.render();

    const commands = mounted.commands();
    if (!commands) throw Error("ACTUAL_BANK_IMPORT_REAL_COMMANDS_REQUIRED");
    expect(commands.pending()).toMatchObject({ request: submitted, key: "bank-key-1", uncertain: true });
    expect(mounted.calls).toHaveLength(1);
    const originalBody = mounted.calls[0].body;
    expect(JSON.parse(originalBody)).toEqual(submitted.input);
    expect(mounted.calls[0].key).toBe("bank-key-1");
    expect(mounted.stored.size).toBe(1);
    expect(mounted.nodes().some(node => node.type === "aside" && String(node.props.children).includes("Retry sends the exact original body and key"))).toBe(true);

    const newerNumber = mounted.node("input", "number");
    if (!newerNumber) throw Error("ACTUAL_BANK_IMPORT_NEWER_FIELD_REQUIRED");
    mounted.setField(newerNumber, "ST-200");
    mounted.markInput();
    mounted.render();
    const retryAfterLostResponse = mounted.nodes().find(node => node.type === "button" && node.props.children === "Retry original bank import");
    if (!retryAfterLostResponse) throw Error("ACTUAL_BANK_IMPORT_RECOVERY_ACTION_REQUIRED");
    callBankHandler(retryAfterLostResponse.props.onClick);
    await mounted.flush();

    let nodes = mounted.render();
    expect(mounted.calls).toHaveLength(2);
    expect(mounted.calls.map(call => call.body)).toEqual([originalBody, originalBody]);
    expect(mounted.calls.map(call => call.key)).toEqual(["bank-key-1", "bank-key-1"]);
    expect(commands.pending()).toMatchObject({ request: submitted, key: "bank-key-1", uncertain: true });
    expect(mounted.stored.size).toBe(2);
    expect(mounted.node("input", "number")?.props.value).toBe("ST-200");
    expect(nodes.some(node => node.type === "aside" && String(node.props.children).includes("native Bank receipt"))).toBe(true);
    expect(mounted.router.push).not.toHaveBeenCalled();
    expect(mounted.router.refresh).not.toHaveBeenCalled();

    const retryMatchingReceipt = nodes.find(node => node.type === "button" && node.props.children === "Retry original bank import");
    if (!retryMatchingReceipt) throw Error("ACTUAL_BANK_IMPORT_RECOVERY_ACTION_REQUIRED");
    callBankHandler(retryMatchingReceipt.props.onClick);
    expect(mounted.calls).toHaveLength(3);
    expect(mounted.calls.map(call => call.body)).toEqual([originalBody, originalBody, originalBody]);
    expect(mounted.calls.map(call => call.key)).toEqual(["bank-key-1", "bank-key-1", "bank-key-1"]);

    const newerDescription = mounted.node("input", "description-0");
    if (!newerDescription) throw Error("ACTUAL_BANK_IMPORT_NEWER_ROW_FIELD_REQUIRED");
    mounted.setField(newerDescription, "A newer unsent note");
    mounted.markInput();
    mounted.render();
    resolveReceipt(Response.json({ data: { id: "statement-bank-key-1", status: "imported", imported: submitted.input.rows.length } }));
    await mounted.flush();

    nodes = mounted.render();
    expect(commands.pending()).toBeNull();
    expect(mounted.stored.size).toBe(0);
    expect(mounted.calls.map(call => call.body)).toEqual([originalBody, originalBody, originalBody]);
    expect(mounted.calls.map(call => call.key)).toEqual(["bank-key-1", "bank-key-1", "bank-key-1"]);
    expect(mounted.node("input", "number")?.props.value).toBe("ST-200");
    expect(mounted.node("input", "description-0")?.props.value).toBe("A newer unsent note");
    expect(nodes.some(node => node.type === "aside" && String(node.props.children).includes("Your newer fields remain here for review"))).toBe(true);
    expect(nodes.some(node => node.type === "form-drawer" && node.props.open === true)).toBe(true);
    expect(mounted.router.push).not.toHaveBeenCalled();
    expect(mounted.router.refresh).not.toHaveBeenCalled();
  } finally {
    vi.unstubAllGlobals();
  }
});
