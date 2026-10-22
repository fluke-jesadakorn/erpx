"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { FormDrawer } from "@erpx/ui/components/dialog";
import { Button, Field, Select, TextInput } from "@erpx/ui/components/foundation";

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import { createFinanceBankCommands, type FinanceBankDraft, type FinanceBankRequest, type FinanceBankScope } from "@erpx/feature-finance/lib/bank-command";
import { useFinanceActionAllowed } from "@erpx/feature-finance/components/purchase-context";
import { acknowledgeFinanceForm, captureFinanceForm, useFinanceDraftClose, useFinanceDraftCommandPending, useFinanceDraftScope } from "@erpx/feature-finance/components/finance-draft-boundary";
import { announceWorkChange } from "@erpx/feature-work_projection/lib/work-refresh";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@erpx/feature-files/components/attachment-panel";
import { Card, Stack, FormGrid, FormLayout } from "@erpx/ui/components/foundation";
type Row = Record<string, string | number | null>;
type BankCommandOutcome = { unchanged: boolean; completion: HTMLFormElement | null };
function useBankCommand(scope: FinanceBankScope) {
  const identity=`finance-bank:${JSON.stringify([scope.base,scope.ownerKey])}`,trackCommandPending=useFinanceDraftCommandPending();
  const trackPending=useCallback((token:object|string,completion:ReturnType<typeof captureFinanceForm>,pending:boolean)=>{
    trackCommandPending(token,completion,pending);
  },[trackCommandPending]);
  const [scopeStore] = useState(() => {
    let snapshot={scope,identity,trackPending};
    return {read:()=>snapshot,update:(next:typeof snapshot)=>{snapshot=next;}};
  });
  const trackedIdentity=useRef(identity),commands=useMemo(()=>createFinanceBankCommands(()=>{const current=scopeStore.read();return current.identity===identity?current.scope:null;}),[identity,scopeStore]);
  const [error, setError] = useState(""), [notice, setNotice] = useState(""), [busy, setBusy] = useState(false), [pendingState, setPendingState] = useState<{identity:string;request:FinanceBankRequest|null}>({identity,request:null});
  const pendingRequest=pendingState.identity===identity?pendingState.request:null;
  useLayoutEffect(()=>{
    scopeStore.update({scope,identity,trackPending});
    if(trackedIdentity.current===identity)return;
    trackedIdentity.current=identity;setPendingState({identity,request:null});setError("");setNotice("");setBusy(false);
  },[identity,scope,scopeStore,trackPending]);
  const refreshPending = useCallback((completion?: ReturnType<typeof captureFinanceForm>) => {
    if(scopeStore.read().identity!==identity)return;
    try {
      const request = commands.pending()?.request ?? null;
      const current=scopeStore.read();
      if(current.identity!==identity)return;
      setPendingState({identity,request});
      current.trackPending(identity, request ? completion ?? captureFinanceForm() : undefined, request !== null);
    }
    catch (cause) {
      const current=scopeStore.read();
      if(current.identity!==identity)return;
      current.trackPending(identity, completion ?? captureFinanceForm(), true);
      setError(cause instanceof Error ? cause.message : "Return to the original Bank owner to recover this request.");
    }
  }, [commands,identity,scopeStore]);
  const readDraft = useCallback(():FinanceBankDraft|null=>{
    if(scopeStore.read().identity!==identity)return null;
    try{return commands.readDraft();}
    catch(cause){setError(cause instanceof Error?cause.message:"Return to the original Bank owner to recover this draft.");return null;}
  },[commands,identity,scopeStore]);
  const saveDraft = useCallback((draft:FinanceBankDraft):boolean=>{
    if(scopeStore.read().identity!==identity)return false;
    try{commands.saveDraft(draft);return true;}
    catch(cause){setError(cause instanceof Error?cause.message:"The newer Bank draft could not be retained.");return false;}
  },[commands,identity,scopeStore]);
  const clearSavedDraft = useCallback(()=>{
    if(scopeStore.read().identity!==identity)return;
    try{commands.clearDraft();}
    catch(cause){setError(cause instanceof Error?cause.message:"The newer Bank draft could not be cleared.");}
  },[commands,identity,scopeStore]);
  useEffect(() => {
    let active = true;
    queueMicrotask(() => { if (active) refreshPending(); });
    return () => { active = false; };
  }, [refreshPending, scope.base, scope.ownerKey, scope.source, scope.canRecover]);
  async function complete(operation: Promise<Awaited<ReturnType<ReturnType<typeof createFinanceBankCommands>["submit"]>>>, completion?: ReturnType<typeof captureFinanceForm>) {
    const submittedIdentity=identity;
    const submittedScope=scopeStore.read();
    if(submittedScope.identity!==submittedIdentity)return null;
    submittedScope.trackPending(submittedIdentity, completion ?? captureFinanceForm(), true);
    setBusy(true); setError(""); setNotice("");
    try {
      const result = await operation, acknowledgement = result.acknowledge();
      if (!acknowledgement) throw Error("The original Bank acknowledgement was already consumed. Review the current Bank record.");
      if (acknowledgement.completion) acknowledgeFinanceForm(acknowledgement.completion);
      announceWorkChange();
      if(scopeStore.read().identity===submittedIdentity)setNotice(acknowledgement.unchanged ? "The original Bank request is confirmed." : "The original Bank request is confirmed. Your newer draft is retained for review.");
      return { unchanged: acknowledgement.unchanged, completion: acknowledgement.completion?.form ?? null } satisfies BankCommandOutcome;
    } catch (cause) {
      if(scopeStore.read().identity===submittedIdentity)setError(cause instanceof Error ? cause.message : "The original Bank request is not confirmed. Retry it safely.");
      return null;
    } finally { if(scopeStore.read().identity===submittedIdentity){setBusy(false);refreshPending(completion);} }
  }
  function send(request: FinanceBankRequest, form: HTMLFormElement) {
    setError("");
    const completion = { form, formEditVersion: form.getAttribute("data-erpx-edit-version") }, operation = commands.submit(request, form);
    // The command is durable before its first network await. Publish that fact
    // now so edits made while the first POST is still running are saved as a
    // separate draft, even before complete() reaches its finally block.
    refreshPending(completion);
    return complete(operation, completion);
  }
  function retry(form?: HTMLFormElement) {
    setError("");
    return complete(commands.retry(form), form ? { form, formEditVersion: form.getAttribute("data-erpx-edit-version") } : captureFinanceForm());
  }
  return { send, retry, error, notice, busy, pendingRequest, pending: pendingRequest !== null, refreshPending, readDraft, saveDraft, clearSavedDraft };
}
export function BankImport({
  base,
  accounts,
  initialOpen = false,
  returnHref,
}: {
  base: string;
  accounts: Row[];
  initialOpen?: boolean;
  returnHref?: string;
}) {
  const router = useRouter(), draftScope = useFinanceDraftScope(), canBank = useFinanceActionAllowed("settle-existing", "finance.banking"),
    ownerIdentity=JSON.stringify([base]),currentOwnerIdentity=useRef(ownerIdentity),lastOwnerIdentity=useRef(ownerIdentity),
    draftRevision = useRef(0), formRef = useRef<HTMLFormElement>(null), restoredRequest = useRef<string | null>(null), restoredDraft = useRef<string|null>(null);
  const [formOwner,setFormOwner]=useState(ownerIdentity),
    [fields, setFields] = useState({ bankAccount: "", number: "", date: "", opening: "", closing: "" }),
    [rows, setRows] = useState<{ externalId: string; transactionDate: string; description: string; amount: string }[]>([
      { externalId: "", transactionDate: "", description: "", amount: "0.00" },
    ]), [open, setOpen] = useState(initialOpen), [visited, setVisited] = useState(initialOpen), [confirmedWithNewerDraft, setConfirmedWithNewerDraft] = useState(false);
  if(formOwner!==ownerIdentity){
    setFormOwner(ownerIdentity);
    setFields({bankAccount:"",number:"",date:"",opening:"",closing:""});
    setRows([{externalId:"",transactionDate:"",description:"",amount:"0.00"}]);
    setConfirmedWithNewerDraft(false);
  }
  const requestFromDraft = (): FinanceBankRequest | null => {
    const account = accounts.find(item => String(item.id) === fields.bankAccount);
    if (!account) return null;
    return { kind: "import", input: {
      bankAccount: fields.bankAccount, statementNumber: fields.number, statementDate: fields.date,
      opening: fields.opening, closing: fields.closing, sourceHash: "server-computed",
      rows: rows.map(row => ({ ...row, currency: String(account.currency) })),
    } };
  };
  const command = useBankCommand({ base, ownerKey: "statement-import", source: JSON.stringify([accounts, canBank]), draft: () => draftRevision.current, draftRequest: requestFromDraft, canStart: canBank, canRecover: canBank });
  const pendingRequest=command.pendingRequest,readRecoveryDraft=command.readDraft;
  const importDraftFromForm = ():FinanceBankDraft|null => {
    const form=formRef.current;
    if(!form||!command.pending)return null;
    const values=new FormData(form),indices=Array.from(values.keys()).flatMap(name=>{const match=/^(?:externalId|transactionDate|description|amount)-(\d+)$/.exec(name);return match?[Number(match[1])]:[];});
    const rowCount=Math.max(0,...indices)+1;
    if(rowCount>500)return null;
    const value=(name:string)=>{const item=values.get(name);return typeof item==="string"?item:"";};
    return {kind:"import",fields:{bankAccount:value("bankAccount"),number:value("number"),date:value("date"),opening:value("opening"),closing:value("closing")},rows:Array.from({length:rowCount},(_,index)=>({externalId:value(`externalId-${index}`),transactionDate:value(`transactionDate-${index}`),description:value(`description-${index}`),amount:value(`amount-${index}`)}))};
  };
  const persistImportDraft = (draft?:FinanceBankDraft|null) => {
    const next=draft??importDraftFromForm();
    if(next?.kind==="import")command.saveDraft(next);
  };
  // Restore before the retry controls can receive input. A passive effect can
  // run after paint and make an untouched recovered request look like a new draft.
  useLayoutEffect(() => {
    currentOwnerIdentity.current=ownerIdentity;
    if(lastOwnerIdentity.current!==ownerIdentity){
      lastOwnerIdentity.current=ownerIdentity;
      draftRevision.current=0;restoredRequest.current=null;restoredDraft.current=null;
    }
    const draft=readRecoveryDraft();
    if(draft?.kind==="import"){
      const stamp=JSON.stringify(draft);
      if(restoredDraft.current===stamp)return;
      restoredDraft.current=stamp;
      draftRevision.current=1;
      const restoreOwner=ownerIdentity;
      queueMicrotask(()=>{if(currentOwnerIdentity.current===restoreOwner&&draftRevision.current===1){setFields(draft.fields);setRows(draft.rows);}});
      return;
    }
    const request = pendingRequest;
    if (request?.kind !== "import" || draftRevision.current > 0) return;
    const stamp = JSON.stringify(request);
    if (restoredRequest.current === stamp) return;
    restoredRequest.current = stamp;
    setFields({ bankAccount: request.input.bankAccount, number: request.input.statementNumber, date: request.input.statementDate, opening: request.input.opening, closing: request.input.closing });
    setRows(request.input.rows.map(row => ({ externalId: row.externalId, transactionDate: row.transactionDate, description: row.description ?? "", amount: row.amount })));
  }, [ownerIdentity,pendingRequest,readRecoveryDraft]);
  const clearDraft = () => {
    command.clearSavedDraft();
    setFields({ bankAccount: "", number: "", date: "", opening: "", closing: "" });
    setRows([{ externalId: "", transactionDate: "", description: "", amount: "0.00" }]);
    draftRevision.current = 0; restoredRequest.current=null; restoredDraft.current=null; setConfirmedWithNewerDraft(false);
  };
  const closeDraft = useFinanceDraftClose(() => { clearDraft(); setOpen(false); }, { root: () => formRef.current });
  const markDraft = () => { draftRevision.current += 1; persistImportDraft(); };
  const markStructure = (nextRows:typeof rows) => { markDraft(); formRef.current?.dispatchEvent(new Event("change", { bubbles: true })); persistImportDraft({kind:"import",fields,rows:nextRows}); };
  async function retryOriginal() {
    const outcome = await command.retry(formRef.current ?? undefined);
    if (!outcome) return;
    if (!outcome.unchanged) { setConfirmedWithNewerDraft(true); return; }
    if (initialOpen) router.push(returnHref ?? canonicalUiHref(`${base}/finance/bank`));
    else { clearDraft(); setOpen(false); router.refresh(); }
  }
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      {!initialOpen && (
        <Button appearance="standard" type="button" variant="primary" onClick={() => { if (open) closeDraft(); else { setVisited(true); setOpen(true); } }}>
          {open ? "Close bank statement import" : "Import bank statement"}
        </Button>
      )}
      {command.error && <InlineNotice tone="danger" role="alert">{command.error}</InlineNotice>}
      {command.notice && <InlineNotice tone="info" role="status">{command.notice}</InlineNotice>}
      {!open && command.pendingRequest?.kind === "import" && <Button appearance="standard" type="button" variant="secondary" disabled={command.busy || !canBank} onClick={() => void retryOriginal()}>Retry original bank import</Button>}
      {open && (
        <FormDrawer appearance="standard" inline={initialOpen} open={open} onClose={closeDraft} title="Import bank statement" dismissDisabled={command.busy} retainFormContents={visited}><Stack as="div" className="ui-stack">
          {command.pendingRequest?.kind === "import" && <InlineNotice tone="info" role="status">This statement request is not confirmed yet. Retry sends the exact original body and key. Any edits below remain separate.</InlineNotice>}
          {command.pendingRequest?.kind === "import" && <Button appearance="standard" type="button" variant="secondary" disabled={command.busy || !canBank} onClick={() => void retryOriginal()}>Retry original bank import</Button>}
          {confirmedWithNewerDraft && <InlineNotice tone="info" role="status">The earlier statement was imported. Your newer fields remain here for review; return to reconciliation before starting another import.</InlineNotice>}
          <form ref={formRef} data-finance-draft={draftScope} onInputCapture={markDraft} onChangeCapture={markDraft}
          onSubmit={async (e) => {
            e.preventDefault();
            const request = requestFromDraft();
            if (!request) return;
            const outcome = await command.send(request, e.currentTarget);
            if (!outcome) return;
            if (!outcome.unchanged) { setConfirmedWithNewerDraft(true); return; }
            if (initialOpen) router.push(returnHref ?? canonicalUiHref(`${base}/finance/bank`));
            else { clearDraft(); setOpen(false); router.refresh(); }
          }}
        >
          <FormGrid as="div" className="ui-form-grid">
            <Field appearance="standard">
              Bank ledger account
              <Select appearance="standard" name="bankAccount" required value={fields.bankAccount} onChange={event => setFields(value => ({ ...value, bankAccount: event.target.value }))}>
                <option value="">Select account</option>
                {accounts.map((a) => (
                  <option value={String(a.id)} key={String(a.id)}>
                    {String(a.code)} · {String(a.name)} · {String(a.currency)} (base{" "}
                    {String(a.base_currency)})
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Statement number
              <TextInput appearance="standard" name="number" required value={fields.number} onChange={event => setFields(value => ({ ...value, number: event.target.value }))} />
            </Field>
            <Field appearance="standard">
              Statement date
              <TextInput appearance="standard" name="date" type="date" value={fields.date} onChange={event => setFields(value => ({ ...value, date: event.target.value }))} required />
            </Field>
            <Field appearance="standard">
              Opening balance
              <TextInput appearance="standard" name="opening" type="number" step="0.01" value={fields.opening} onChange={event => setFields(value => ({ ...value, opening: event.target.value }))} required />
            </Field>
            <Field appearance="standard">
              Closing balance
              <TextInput appearance="standard" name="closing" type="number" step="0.01" value={fields.closing} onChange={event => setFields(value => ({ ...value, closing: event.target.value }))} required />
            </Field>
          </FormGrid>
          {rows.map((r, i) => (
            <FormGrid as="div" className="ui-form-grid" key={i}>
              {(
                [
                  ["externalId", "Bank reference", "text"],
                  ["transactionDate", "Transaction date", "date"],
                  ["description", "Description", "text"],
                  ["amount", "Signed amount", "number"],
                ] as const
              ).map(([key, label, type]) => (
                <Field appearance="standard" key={key}>
                  {label} {i + 1}
                  <TextInput appearance="standard"
                    name={`${key}-${i}`}
                    type={type}
                    step={type === "number" ? "0.01" : undefined}
                    required={key !== "description"}
                    value={r[key]}
                    onChange={(e) =>
                      setRows(rows.map((v, j) => (i === j ? { ...v, [key]: e.target.value } : v)))
                    }
                  />
                </Field>
              ))}
              {rows.length > 1 && (
                <Button appearance="standard"
                  variant="secondary"
                  type="button"
                  onClick={() => { const nextRows=rows.filter((_,j)=>j!==i);markStructure(nextRows);setRows(nextRows); }}
                >
                  Remove row {i + 1}
                </Button>
              )}
            </FormGrid>
          ))}
          <div className="ui-form-actions">
            <Button appearance="standard"
              variant="secondary"
              type="button"
              onClick={() => {
                const nextRows=[
                  ...rows,
                  { externalId: "", transactionDate: "", description: "", amount: "0.00" },
                ];markStructure(nextRows);setRows(nextRows);
              }}
            >
              Add statement row
            </Button>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={command.busy || command.pending || confirmedWithNewerDraft || !canBank}>
              Validate and import
            </Button>
          </div>
          <p className="muted">
            Receipts are positive; withdrawals are negative. For foreign accounts, the confirmed
            company manual rate for the statement date converts the closing balance to the ledger
            functional currency.
          </p>
        </form></Stack></FormDrawer>
      )}
    </Card>
  );
}
export function BankMatch({
  base,
  transaction,
  payments,
  canWrite,
  canApprove,
}: {
  base: string;
  transaction: Row;
  payments: Row[];
  canWrite: boolean;
  canApprove: boolean;
}) {
  const router = useRouter(), draftScope = useFinanceDraftScope(), canBank = useFinanceActionAllowed("settle-existing", "finance.banking") && canWrite && canApprove,
    formRef = useRef<HTMLFormElement>(null), ownerIdentity=JSON.stringify([base,String(transaction.id)]),
    [draft,setDraft] = useState(() => ({ ownerIdentity, files: null as AttachmentPanelChange | null, selectedPayment: "", proof: "", confirmedWithNewerDraft: false, revision: 0 })),
    lastOwnerIdentity=useRef(ownerIdentity),draftRevision=useRef({ownerIdentity,revision:0}),pendingDraft=useRef<{ownerIdentity:string;request:FinanceBankRequest|null}>({ownerIdentity,request:null}),restoredDraft=useRef<string|null>(null);
  const currentDraft=draft.ownerIdentity===ownerIdentity?draft:{ownerIdentity,files:null,selectedPayment:"",proof:"",confirmedWithNewerDraft:false,revision:0};
  if(draft.ownerIdentity!==ownerIdentity)setDraft(currentDraft);
  const requestFromDraft = (): FinanceBankRequest | null => {
    const recovered=pendingDraft.current.ownerIdentity===ownerIdentity?pendingDraft.current.request:null;
    if(draftRevision.current.ownerIdentity===ownerIdentity&&draftRevision.current.revision===0&&recovered?.kind==="reconcile"&&recovered.id===String(transaction.id))return recovered;
    return currentDraft.ownerIdentity===ownerIdentity&&currentDraft.selectedPayment && currentDraft.proof.trim() ? { kind: "reconcile", id: String(transaction.id), input: {
    expectedVersion: Number(transaction.version), matchedType: "payment", matchedId: currentDraft.selectedPayment, proof: currentDraft.proof,
    attachmentSetIds: currentDraft.files?.setId && currentDraft.files.readyCount > 0 && currentDraft.files.bindable ? [currentDraft.files.setId] : [],
    } } : null;
  };
  const command = useBankCommand({ base, ownerKey: `transaction:${String(transaction.id)}`, source: JSON.stringify([transaction, payments, canBank]), draft: () => draftRevision.current.ownerIdentity===ownerIdentity?draftRevision.current.revision:0, draftRequest: requestFromDraft, canStart: canBank && transaction.match_status !== "matched", canRecover: canBank });
  const pendingRequest=command.pendingRequest,readRecoveryDraft=command.readDraft;
  const updateDraft=(patch:Partial<typeof currentDraft>,edited=false)=>{
    if(edited){const previous=draftRevision.current;draftRevision.current=previous.ownerIdentity===ownerIdentity?{ownerIdentity,revision:previous.revision+1}:{ownerIdentity,revision:1};}
    setDraft(previous=>{
      const ownerDraft=previous.ownerIdentity===ownerIdentity?previous:{ownerIdentity,files:null,selectedPayment:"",proof:"",confirmedWithNewerDraft:false,revision:0},next={...ownerDraft,...patch,revision:ownerDraft.revision+(edited?1:0)};
      if(edited&&command.pending)command.saveDraft({kind:"reconcile",id:String(transaction.id),selectedPayment:next.selectedPayment,proof:next.proof});
      return next;
    });
  };
  useLayoutEffect(()=>{
    const saved=readRecoveryDraft();
    if(saved?.kind!=="reconcile"||saved.id!==String(transaction.id))return;
    const stamp=JSON.stringify(saved);
    if(restoredDraft.current===stamp)return;
    restoredDraft.current=stamp;
    draftRevision.current={ownerIdentity,revision:1};
    queueMicrotask(()=>{if(draftRevision.current.ownerIdentity!==ownerIdentity||draftRevision.current.revision!==1)return;setDraft(previous=>{const ownerDraft=previous.ownerIdentity===ownerIdentity?previous:{ownerIdentity,files:null,selectedPayment:"",proof:"",confirmedWithNewerDraft:false,revision:0};return {...ownerDraft,selectedPayment:saved.selectedPayment,proof:saved.proof,revision:1};});});
  },[ownerIdentity,transaction.id,pendingRequest,readRecoveryDraft]);
  useLayoutEffect(()=>{pendingDraft.current={ownerIdentity,request:pendingRequest};},[ownerIdentity,pendingRequest]);
  useLayoutEffect(()=>{
    if(lastOwnerIdentity.current===ownerIdentity)return;
    lastOwnerIdentity.current=ownerIdentity;
    draftRevision.current={ownerIdentity,revision:0};
    restoredDraft.current=null;
    if(formRef.current)window.dispatchEvent(new CustomEvent("erpx:record-saved",{detail:{root:formRef.current}}));
  },[ownerIdentity]);
  const markStructure = () => {
    const form = formRef.current;
    if (form) form.dispatchEvent(new Event("change", { bubbles: true }));
    else updateDraft({},true);
  };
  const pendingForTransaction = command.pendingRequest?.kind === "reconcile" && command.pendingRequest.id === String(transaction.id);
  const matched = transaction.match_status === "matched", hasRetainedDraft = pendingForTransaction || currentDraft.revision > 0 || currentDraft.confirmedWithNewerDraft;
  const restoredRequest=command.pendingRequest?.kind==="reconcile"&&command.pendingRequest.id===String(transaction.id)&&currentDraft.revision===0&&currentDraft.ownerIdentity===ownerIdentity?command.pendingRequest:null,
    selectedPayment=restoredRequest?.input.matchedId??currentDraft.selectedPayment,
    proof=restoredRequest?.input.proof??currentDraft.proof;
  const matches = payments.filter(
    (p) =>
      p.currency === transaction.currency &&
      p.bank_account === transaction.bank_account &&
      (p.kind === "receipt" ? Number(p.amount_minor) : -Number(p.amount_minor)) ===
        Number(transaction.amount_minor),
  );
  async function retryOriginal() {
    const outcome = await command.retry(formRef.current ?? undefined);
    if (!outcome) return;
    if (outcome.unchanged) router.refresh();
    else updateDraft({confirmedWithNewerDraft:true});
  }
  if (matched && !hasRetainedDraft) return <div>{command.error && <InlineNotice tone="danger" role="alert">{command.error}</InlineNotice>}<span className="muted">This bank movement is already matched.</span></div>;
  return (
    <div>
      {command.error && <InlineNotice tone="danger" role="alert">{command.error}</InlineNotice>}
      {command.notice && <InlineNotice tone="info" role="status">{command.notice}</InlineNotice>}
      {pendingForTransaction && <InlineNotice tone="info" role="status">{matched ? "This bank movement is already marked matched. The original request is not confirmed yet; retry resends its exact body and key, while edits below remain separate." : "The original match request is not confirmed yet. Retry resends the exact body and key; edits below remain separate."}</InlineNotice>}
      {pendingForTransaction && <Button appearance="standard" type="button" variant="secondary" disabled={command.busy || !canBank} onClick={() => void retryOriginal()}>Retry original bank match</Button>}
      {currentDraft.confirmedWithNewerDraft && <InlineNotice tone="info" role="status">The earlier match was saved. Your newer evidence remains here for review; close this review to load the current Bank movement.</InlineNotice>}
      {!matches.length && !hasRetainedDraft ? (
        <span className="muted">No posted payment matches this amount, bank and currency.</span>
      ) : (
        <FormLayout
          ref={formRef}
          data-finance-draft={draftScope}
          onChangeCapture={(event) => {
            if (event.target === event.currentTarget) updateDraft({},true);
            else if (event.target instanceof HTMLInputElement && event.target.type === "file" && event.target.files?.length) updateDraft({},true);
          }}
          className="ui-form-grid"
          onSubmit={async (e) => {
            e.preventDefault();
            const request = requestFromDraft();
            if (!request) return;
            const outcome = await command.send(request, e.currentTarget);
            if (!outcome) return;
            if (outcome.unchanged) router.refresh();
            else updateDraft({confirmedWithNewerDraft:true});
          }}
        >
          <Field appearance="standard">
            Match payment
            <Select appearance="standard" name="payment" required value={selectedPayment} onChange={event => updateDraft({selectedPayment:event.target.value},true)}>
              <option value="">Review and select</option>
              {hasRetainedDraft && selectedPayment && !matches.some(payment => String(payment.id) === selectedPayment) && <option value={selectedPayment}>Previously selected payment · unavailable after match</option>}
              {matches.map((p) => (
                <option value={String(p.id)} key={String(p.id)}>
                  {String(p.payment_number)} · {String(p.payment_date)} ·{" "}
                  {minorToDecimal(Number(p.amount_minor))}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Review evidence
            <TextInput appearance="standard" name="proof" required minLength={3} value={proof} onChange={event => updateDraft({proof:event.target.value},true)} />
          </Field>
          <div
            onClickCapture={event => {
              if (!(event.target instanceof Element)) return;
              const button = event.target.closest<HTMLButtonElement>("button");
              if (!button) return;
              const label = button.textContent?.trim();
              const action = button.getAttribute("aria-label");
              if (action?.startsWith("Retry upload for ") || label === "Record sanitization review" || label === "Withdraw file" || label === "Attach clean files to this record") markStructure();
            }}
            onDropCapture={event => { if (event.dataTransfer.files.length > 0) markStructure(); }}
          >
            <AttachmentPanel appearance="standard"
              key={ownerIdentity}
              base={base}
              targetType="finance_bank_transaction"
              targetId={String(transaction.id)}
              expectedTargetVersion={Number(transaction.version)}
              componentKind="action"
              componentRef="reconcile"
              purposeKey="reconciliation_workpaper"
              title="Reconciliation workpaper"
              description="Attach the bank matching workpaper or other evidence reviewed for this match."
              visibility="Visible only to authorized Company Finance reviewers."
              onChange={change => updateDraft({files:change})}
            />
          </div>
          <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={matched || command.busy || command.pending || currentDraft.confirmedWithNewerDraft || !canBank}>
            Confirm match
          </Button>
        </FormLayout>
      )}
    </div>
  );
}
