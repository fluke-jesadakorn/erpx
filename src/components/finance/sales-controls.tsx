"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { hasUnsavedWork } from "@/lib/work_projection/context-switch";
import { salesInvoiceDrawerHref } from "@/lib/finance/invoice-drawer-list-search";
import { useCallback,useLayoutEffect,useRef, useState } from "react";
import { useRouter } from "@/lib/shared/destination-router";
import { FormDrawer } from "@/components/ui/dialog";
import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, FormGrid, InlineNotice, Select, TextArea, TextInput } from "@/components/ui/foundation";
import {createFinanceSalesCommands,createFinanceSalesInvoiceCommands,type FinanceInvoiceDraft,type FinanceSalesCommands} from "@/lib/finance/sales-command";
import {captureFinanceForm,markFinanceFormEdited,useFinanceDraftClose,useFinanceDraftCommandPending,useFinanceDraftScope} from "./finance-draft-boundary";
import { businessDate } from "@/server/modules/pos/public/rules/pos";
import { calculateInvoiceLines } from "@/server/modules/finance/public/rules/finance-invoice-lines";
import { minorToDecimal } from "@/lib/finance/finance-money";
import type { SalesDefaults } from "@/server/modules/finance/public/sales";
import type { FinanceRow } from "@/server/modules/finance/public/finance";
import styles from "./sales-workspace.module.css";
function useSalesCommand(base: string, invoiceForm?: {current:HTMLFormElement|null}, invoiceDestination?:(id:unknown)=>string) {
  const router=useRouter(),trackCommandPending=useFinanceDraftCommandPending(),source=useRef<{base:string}|null>({base}),commands=useRef<FinanceSalesCommands|null>(null),destinationRef=useRef<((data:Record<string,unknown>)=>string)|undefined>(undefined),pendingToken=useRef({}),pendingCompletion=useRef<ReturnType<typeof captureFinanceForm>|undefined>(undefined),busy=useRef(false),invoiceRecovery=!!invoiceForm;
  commands.current??=invoiceRecovery?createFinanceSalesInvoiceCommands(()=>source.current):createFinanceSalesCommands(()=>source.current);
  const [pending,setPending]=useState(false),[error,setError]=useState(""),[unresolved,setUnresolved]=useState(false),[saved,setSaved]=useState(false);
  useLayoutEffect(()=>{
    source.current={base};
    if(invoiceRecovery)try{
      const recovery=commands.current?.restoreInvoice?.();
      if(recovery?.pending){destinationRef.current=data=>invoiceDestination?invoiceDestination(data.id):canonicalUiHref(`${base}/finance/invoices/${data.id}/overview`);
        // sessionStorage is an external source; hydrate its command lock before the user can submit.
        // eslint-disable-next-line react-hooks/set-state-in-effect
        setUnresolved(true);
        setError("The original invoice result needs confirmation.");
        const form=invoiceForm.current;trackCommandPending(pendingToken.current,form?{form,formEditVersion:form.getAttribute("data-erpx-edit-version")}:undefined,true);
      }
    }catch(cause){
      // Keep the saved command quarantined and surface its ownership/storage error.
      setUnresolved(true);
      setError(cause instanceof Error?cause.message:"The original invoice request could not be restored safely.");
    }
    return()=>{source.current=null;};
  },[base,invoiceDestination,invoiceForm,invoiceRecovery,trackCommandPending]);
  async function send(path:string,input:unknown,destination?:(data:Record<string,unknown>)=>string,retryOriginal=false){
    if(busy.current)return false;busy.current=true;setPending(true);setError("");setSaved(false);
    let completion:ReturnType<typeof captureFinanceForm>|undefined;
    try{
      if(!commands.current!.pending()){destinationRef.current=destination;pendingCompletion.current=captureFinanceForm();}
      completion=pendingCompletion.current;
      const operation=retryOriginal?commands.current!.retry():commands.current!.submit(path,input,completion);
      if(commands.current!.pending())trackCommandPending(pendingToken.current,completion,true);
      const outcome=await operation,unchanged=outcome.unchanged();
      if(!outcome.acknowledge())throw Error("Confirm the original Finance request again.");
      trackCommandPending(pendingToken.current,completion,false);pendingCompletion.current=undefined;
      setUnresolved(false);
      if(unchanged){if(destinationRef.current)router.push(destinationRef.current(outcome.data));router.refresh();}else setSaved(true);
      destinationRef.current=undefined;return true;
    }catch(cause){let stillPending=false,pendingReadable=true;try{stillPending=commands.current!.pending();}catch{stillPending=true;pendingReadable=false;}if(stillPending&&pendingReadable)trackCommandPending(pendingToken.current,completion,true);else{trackCommandPending(pendingToken.current,completion,false);if(!stillPending){pendingCompletion.current=undefined;destinationRef.current=undefined;}}setUnresolved(stillPending);setError(cause instanceof Error?cause.message:"Unable to confirm the Finance command.");return false;}
    finally{busy.current=false;setPending(false);}
  }
  const retryOriginal=()=>send("",null,destinationRef.current,true),restoreInvoice=useCallback(()=>commands.current?.restoreInvoice?.()??null,[]),readInvoiceDraft=useCallback(()=>commands.current?.readInvoiceDraft?.()??null,[]),saveInvoiceDraft=useCallback((draft:FinanceInvoiceDraft)=>commands.current?.saveInvoiceDraft?.(draft),[]),clearInvoiceDraft=useCallback(()=>commands.current?.clearInvoiceDraft?.(),[]);
  return {send,pending,error,unresolved,saved,setError,retryOriginal,restoreInvoice,readInvoiceDraft,saveInvoiceDraft,clearInvoiceDraft};
}
export function IssueCustomerReceipt({ base, paymentId }: { base: string; paymentId: string }) {
  const { send, pending, error,unresolved,saved,retryOriginal } = useSalesCommand(base);
  return <><Button appearance="standard" type="button" disabled={pending} onClick={() => void send(`receipts/${paymentId}/issue`, {}, data => canonicalUiHref(`${base}/finance/sales/receipts/${data.id}`))}>Issue receipt</Button>{error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}{unresolved&&<Button appearance="standard" type="button" disabled={pending} onClick={()=>void retryOriginal()}>Retry original confirmation</Button>}{saved&&<InlineNotice role="status">The original request was saved. Your newer edits remain.</InlineNotice>}</>;
}
export function PrintReceipt() { return <Button appearance="standard" type="button" onClick={() => window.print()}>Print / Save as PDF</Button>; }
export function SalesInvoiceEditor({ base, parties, defaults, inline = false }: { base: string; parties: FinanceRow[]; defaults: SalesDefaults[]; inline?: boolean }) {
  const form=useRef<HTMLFormElement|null>(null),invoiceDestination=useCallback((id:unknown)=>salesInvoiceDrawerHref(base,id,window.location.href,inline),[base,inline]),{send,pending,error,unresolved,saved,setError,retryOriginal,restoreInvoice,readInvoiceDraft,saveInvoiceDraft,clearInvoiceDraft}=useSalesCommand(base,form,invoiceDestination),router=useRouter(),scope=useFinanceDraftScope();
  const [open, setOpen] = useState(true), [currency, setCurrency] = useState(defaults[0]?.currency ?? "THB");
  const [lines, setLines] = useState([{ description: "", quantity: "1", unitPrice: "", taxBps: 700 }]);
  const lineStructureEdited=useRef(false),closeEditor=useFinanceDraftClose(()=>{
    try{clearInvoiceDraft();}catch{return;}
    setOpen(false);
    // Another marked form may share this Sales page. Closing this drawer must
    // keep that form mounted, including after discarding only this invoice.
    if(inline || hasUnsavedWork())return;
    const target=new URL(window.location.href);
    if(!target.searchParams.getAll("new").includes("invoice"))return;
    target.searchParams.delete("new","invoice");
    router.replace(target.pathname+target.search+target.hash,{scroll:false});
  },{root:()=>form.current});
  function restoreDraftFields(formElement:HTMLFormElement,draft:FinanceInvoiceDraft){
    for(const [name,value]of [["partyId",draft.partyId],["invoiceDate",draft.invoiceDate],["dueDate",draft.dueDate],["completionNote",draft.completionNote]] as const){const field=formElement.elements.namedItem(name);if(field instanceof HTMLInputElement||field instanceof HTMLSelectElement||field instanceof HTMLTextAreaElement)field.value=value;}
    const confirmation=formElement.elements.namedItem("completionConfirmed");if(confirmation&&"checked"in confirmation)(confirmation as HTMLInputElement).checked=draft.completionConfirmed;
    setCurrency(draft.currency);setLines(draft.lines.map(line=>({...line})));
  }
  const invoiceFormDraft=useCallback((formElement:HTMLFormElement):FinanceInvoiceDraft=>{
    const values=new FormData(formElement),text=(name:string)=>{const value=values.get(name);return typeof value==="string"?value:"";};
    const lineGroups=Array.from(formElement.querySelectorAll<HTMLElement>("[data-sales-invoice-line]"));
    return{partyId:text("partyId"),invoiceDate:text("invoiceDate"),dueDate:text("dueDate"),currency:text("currency")||currency,completionNote:text("completionNote"),completionConfirmed:values.get("completionConfirmed")==="on",lines:lineGroups.map((_,index)=>{const rate=Number(text(`service-${index}-tax`));return{description:text(`service-${index}-description`),quantity:text(`service-${index}-quantity`),unitPrice:text(`service-${index}-unitPrice`),taxBps:Number.isFinite(rate)?Math.round(rate*100):0};})};
  },[currency]);
  const invoiceRecoveryPending=useCallback(()=>{try{return !!restoreInvoice()?.pending;}catch{return true;}},[restoreInvoice]);
  const commandsHaveInvoiceWork=useCallback(()=>!!readInvoiceDraft()||invoiceRecoveryPending(),[readInvoiceDraft,invoiceRecoveryPending]);
  const persistNewerDraft=useCallback(()=>{
    const formElement=form.current;if(!formElement)return;
    try{if(commandsHaveInvoiceWork())saveInvoiceDraft(invoiceFormDraft(formElement));}
    catch(cause){setError(cause instanceof Error?cause.message:"The newer invoice draft could not be retained safely.");}
  },[commandsHaveInvoiceWork,invoiceFormDraft,saveInvoiceDraft,setError]);
  useLayoutEffect(()=>{
    try{
      const recovery=restoreInvoice();if(!recovery)return;
      const savedDraft=recovery.draft??(recovery.pending&&recovery.input?{...recovery.input}:undefined);
      if(savedDraft&&form.current){restoreDraftFields(form.current,savedDraft);if(recovery.draft){markFinanceFormEdited(form.current);}}
    }catch(cause){setError(cause instanceof Error?cause.message:"The saved invoice draft could not be restored safely.");}
  },[restoreInvoice,setError]);
  useLayoutEffect(()=>{if(lineStructureEdited.current){lineStructureEdited.current=false;persistNewerDraft();}},[lines,persistNewerDraft]);
  const mapping = defaults.find(row => row.currency === currency);
  let total: ReturnType<typeof calculateInvoiceLines> | null = null;
  try { total = calculateInvoiceLines(lines.map(line => ({ ...line, accountId: "preview" }))); } catch { /* Incomplete drafts have no valid total yet. */ }
  return <FormDrawer appearance="standard" inline={inline} open={open} onClose={closeEditor} title="Invoice completed services" dismissDisabled={pending||unresolved}>
    <p>For services already completed. For goods or a quotation, use a sales order to retain delivery evidence.</p>
    {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}{unresolved&&<Button appearance="standard" type="button" disabled={pending} onClick={()=>void retryOriginal()}>Retry original confirmation</Button>}{saved&&<InlineNotice role="status">The original request was saved. Your newer edits remain.</InlineNotice>}
    <form ref={form} data-finance-draft={scope} className={styles.form} onInputCapture={persistNewerDraft} onChangeCapture={persistNewerDraft} onSubmit={event => { event.preventDefault(); const values = new FormData(event.currentTarget); void send("invoices", { partyId: values.get("partyId"), invoiceDate: values.get("invoiceDate"), dueDate: values.get("dueDate"), currency, completionConfirmed: values.get("completionConfirmed") === "on", completionNote: values.get("completionNote"), lines }, data => salesInvoiceDrawerHref(base,data.id,window.location.href,inline)); }}>
      <FormGrid><Field appearance="standard">Customer<Select appearance="standard" name="partyId" required><option value="">Select customer</option>{parties.filter(row => row.kind === "customer" && row.status === "active").map(row => <option key={String(row.id)} value={String(row.id)}>{String(row.name)}</option>)}</Select></Field>
      <Field appearance="standard">Currency<Select appearance="standard" name="currency" value={currency} onChange={event => setCurrency(event.target.value)}>{defaults.map(row => <option key={row.id} value={row.currency}>{row.currency}</option>)}</Select></Field>
      <Field appearance="standard">Invoice date<TextInput appearance="standard" name="invoiceDate" type="date" required defaultValue={businessDate()} /></Field><Field appearance="standard">Due date<TextInput appearance="standard" name="dueDate" type="date" required defaultValue={businessDate()} /></Field></FormGrid>
      {lines.map((line, index) => <fieldset key={index} className={styles.line} data-sales-invoice-line><legend>Service {index + 1}</legend><Field appearance="standard">Description<TextInput appearance="standard" name={`service-${index}-description`} required maxLength={500} value={line.description} onChange={event => setLines(lines.map((row, i) => i === index ? { ...row, description: event.target.value } : row))} /></Field><FormGrid>
        <Field appearance="standard">Quantity<TextInput appearance="standard" name={`service-${index}-quantity`} type="number" min="0.001" step="0.001" required value={line.quantity} onChange={event => setLines(lines.map((row, i) => i === index ? { ...row, quantity: event.target.value } : row))} /></Field>
        <Field appearance="standard">Unit price<TextInput appearance="standard" name={`service-${index}-unitPrice`} type="number" min="0.01" step="0.01" required value={line.unitPrice} onChange={event => setLines(lines.map((row, i) => i === index ? { ...row, unitPrice: event.target.value } : row))} /></Field>
        <Field appearance="standard">VAT (%)<TextInput appearance="standard" name={`service-${index}-tax`} type="number" min="0" max="100" step="0.01" required value={line.taxBps / 100} onChange={event => setLines(lines.map((row, i) => i === index ? { ...row, taxBps: Math.round(Number(event.target.value) * 100) } : row))} /></Field></FormGrid>
        {lines.length > 1 && <Button appearance="standard" type="button" variant="secondary" onClick={event => { markFinanceFormEdited(event.currentTarget.closest<HTMLFormElement>("form[data-finance-draft]"));lineStructureEdited.current=true;setLines(current => current.filter((_, i) => i !== index)); }}>Remove service</Button>}
      </fieldset>)}
      <Button appearance="standard" type="button" variant="secondary" disabled={lines.length >= 100} onClick={event => { markFinanceFormEdited(event.currentTarget.closest<HTMLFormElement>("form[data-finance-draft]"));lineStructureEdited.current=true;setLines(current => [...current, { description: "", quantity: "1", unitPrice: "", taxBps: 700 }]); }}>Add service</Button>
      <div className={styles.total} aria-live="polite">Subtotal {currency} {total ? minorToDecimal(total.subtotalMinor) : "—"}<br />VAT {currency} {total ? minorToDecimal(total.taxMinor) : "—"}<br /><strong>Total {currency} {total ? minorToDecimal(total.totalMinor) : "—"}</strong></div>
      <Field appearance="standard">Completion evidence<TextArea appearance="standard" name="completionNote" required minLength={3} maxLength={2000} placeholder="What was completed, when, and who accepted it?" /></Field>
      <label><input type="checkbox" name="completionConfirmed" required /> These services are completed and ready to invoice.</label>
      <p className="muted">The document number and accounts are filled from approved Company settings. Finance reviews the posting before approval.</p>
      {!mapping && <InlineNotice tone="danger">Approve sales defaults for this currency before invoicing.</InlineNotice>}
      {mapping && !mapping.tax_account_id && lines.some(line => line.taxBps > 0) && <InlineNotice tone="warning">Output VAT is missing from these sales defaults. <a href={canonicalUiHref(`${base}/finance/sales/settings`)}>Configure and approve a VAT account</a> before preparing a taxable invoice.</InlineNotice>}
      <Button appearance="standard" clientManaged type="submit" disabled={pending || unresolved || !mapping || !total || !!lines.find(line => line.taxBps > 0 && !mapping?.tax_account_id)}>{pending ? "Saving…" : "Prepare invoice"}</Button>
    </form>
  </FormDrawer>;
}
export function SalesDefaultsControls({ base, editions, charts, accounts, canPrepare, canReview, userId }: { base: string; editions: SalesDefaults[]; charts: FinanceRow[]; accounts: FinanceRow[]; canPrepare: boolean; canReview: boolean; userId: string }) {
  const { send, pending, error,unresolved,saved,retryOriginal } = useSalesCommand(base),scope=useFinanceDraftScope(), [chartId, setChartId] = useState(String(charts[0]?.id ?? ""));
  const chart = charts.find(row => row.id === chartId), currency = String(chart?.base_currency ?? "THB");
  const eligible = accounts.filter(row => row.chart_id === chartId && row.currency === currency && row.status === "active");
  const picker = (name: string, label: string, rows: FinanceRow[], required = true) => <Field appearance="standard">{label}<Select appearance="standard" name={name} required={required}><option value="">{required ? "Select account" : "No VAT account"}</option>{rows.map(row => <option key={String(row.id)} value={String(row.id)}>{String(row.code)} · {String(row.name)}</option>)}</Select></Field>;
  return <div className={styles.form}>{error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}{unresolved&&<Button appearance="standard" type="button" disabled={pending} onClick={()=>void retryOriginal()}>Retry original confirmation</Button>}{saved&&<InlineNotice role="status">The original request was saved. Your newer edits remain.</InlineNotice>}
    {canPrepare && <FormDisclosure appearance="standard" drawerTitle="Configure sales defaults" dismissDisabled={pending||unresolved} error={error}><summary>Configure sales accounts</summary><form data-finance-draft={scope} className={styles.form} onSubmit={event => { event.preventDefault(); const values = new FormData(event.currentTarget); void send("defaults", { chartId, currency, receivableAccountId: values.get("receivableAccountId"), revenueAccountId: values.get("revenueAccountId"), ...(values.get("taxAccountId") ? { taxAccountId: values.get("taxAccountId") } : {}), reason: values.get("reason") }); }}>
      <Field appearance="standard">Ledger<Select appearance="standard" value={chartId} required onChange={event => setChartId(event.target.value)}>{charts.filter(row => row.status === "active").map(row => <option key={String(row.id)} value={String(row.id)}>{String(row.name)} · {String(row.base_currency)}</option>)}</Select></Field>
      {picker("receivableAccountId", "Trade receivables", eligible.filter(row => row.control_kind === "receivable"))}{picker("revenueAccountId", "Sales revenue", eligible.filter(row => row.category === "revenue"))}{picker("taxAccountId", "Output VAT", eligible.filter(row => row.category === "liability" && row.control_kind === "none"), false)}
      <Field appearance="standard">Reason<TextArea appearance="standard" name="reason" minLength={10} required /></Field><Button appearance="standard" clientManaged type="submit" disabled={pending}>Prepare for independent review</Button>
    </form></FormDisclosure>}
    {editions.map(row => <article className={styles.line} key={row.id}><strong>{row.currency} · {row.status}</strong><p>{row.reason}</p><p className="muted">Receivables: {String(accounts.find(a => a.id === row.receivable_account_id)?.name ?? row.receivable_account_id)} · Revenue: {String(accounts.find(a => a.id === row.revenue_account_id)?.name ?? row.revenue_account_id)} · VAT: {String(accounts.find(a => a.id === row.tax_account_id)?.name ?? "Not configured")}</p>
      {row.status === "draft" && canReview && row.created_by !== userId && <FormDisclosure appearance="standard" drawerTitle="Review sales defaults" error={error} dismissDisabled={pending||unresolved}><summary>Review defaults</summary><form data-finance-draft={scope} className={styles.form} onSubmit={event => { event.preventDefault(); const values = new FormData(event.currentTarget); void send(`defaults/${row.id}/decide`, { expectedVersion: Number(row.version), action: values.get("action"), reason: values.get("reason") }); }}><Field appearance="standard">Decision<Select appearance="standard" name="action"><option value="approve">Approve</option><option value="reject">Reject</option></Select></Field><Field appearance="standard">Review reason<TextArea appearance="standard" name="reason" minLength={10} required /></Field><Button appearance="standard" clientManaged type="submit" disabled={pending}>Record review</Button></form></FormDisclosure>}
    </article>)}
  </div>;
}
