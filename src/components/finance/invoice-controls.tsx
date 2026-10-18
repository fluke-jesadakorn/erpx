"use client";
import { useFinanceDraftScope, useFinanceDraftClose, captureFinanceForm, acknowledgeFinanceForm } from "./finance-draft-boundary";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { InlineNotice } from "@/components/ui/foundation";

import {FormDisclosure} from "@/components/ui/form-disclosure";
import { FormDrawer } from "@/components/ui/dialog";
import { Button, Field, Select, TextInput } from "@/components/ui/foundation";

import { workFetch } from "@/lib/work_projection/work-fetch";
import { useRef, useState } from "react";
import { useRouter } from "@/lib/shared/destination-router";
import { minorToDecimal } from "@/lib/finance/finance-money";
import { currencyScale, resolveRate } from "@/lib/finance/company-money";
import { useCurrency } from "@/components/finance/money/currency-provider";
import { AttachmentPanel,type AttachmentPanelChange } from "@/components/files/attachment-panel";
import { calculateInvoiceLines, type InvoiceLineInput } from "@/server/modules/finance/public/rules/finance-invoice-lines";
import type { PersistedInvoiceLine } from "@/server/modules/finance/public/finance-invoice-lines";
import { Card, Stack, FormLayout } from "@/components/ui/foundation";
import { useFinanceActionAllowed } from "./purchase-context";
import {financeDocumentCommandAllowed, financeDocumentCommandContext, financeDocumentFormUnchanged, type FinanceDocumentPostControl} from "@/lib/finance/document-post-control";
import {announceWorkChange} from "@/lib/work_projection/work-refresh";
type Row = Record<string, string | number | null>;
type Options = { charts: Row[]; accounts: Row[]; parties: Row[]; periods: Row[] };
function useInvoiceCommand(base: string, purpose: "new-work" | "settle-existing", post?: FinanceDocumentPostControl) {
  const allowed = useFinanceActionAllowed(purpose);
  const router = useRouter(),
    attempt = useRef<{ value: string; key: string } | null>(null);
  const [error, setError] = useState(""),
    [busy, setBusy] = useState(false);
  const [hasAttempt, setHasAttempt] = useState(false);
  const [notice,setNotice] = useState("");
  const pendingRef = useRef(false);
  async function send(path: string, input: unknown) {
    if (pendingRef.current) return false;
    const body = JSON.stringify(input), identity = JSON.stringify([financeDocumentCommandContext(),path,body]);
    if (!financeDocumentCommandAllowed(allowed,path,post) && attempt.current?.value !== identity) {
      setError("This Finance action is unavailable. Your draft is retained.");
      return false;
    }
    if (attempt.current?.value !== identity)
      { attempt.current = { value: identity, key: crypto.randomUUID() }; setHasAttempt(true); }
    pendingRef.current = true; setNotice(""); setError("");
    const completion = captureFinanceForm();
    setBusy(true);
    try {
      const r = await workFetch(`/api/v1${base}/finance/invoices${path}`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
        body,
      }, completion, "caller");
      const v = (await r.json()) as { data: { id: string }; error?: { message: string } };
      if (!r.ok || !v.data || typeof v.data.id !== "string" || !v.data.id) throw new Error(v.error?.message ?? "Could not save.");
      acknowledgeFinanceForm(completion); announceWorkChange();
      attempt.current = null;
      setHasAttempt(false);
      if (!financeDocumentFormUnchanged(completion)) {setNotice("Submitted change saved. Your newer draft is retained."); return false;}
      router.push(canonicalUiHref(`${base}/finance/invoices/${v.data.id}`));
      router.refresh();
      return true;
    } catch (e) {
      setError(e instanceof Error ? e.message : "Connection interrupted. Retry.");
      return false;
    } finally {
      pendingRef.current = false; setBusy(false);
    }
  }
  return { send, error, busy, notice, canSend: allowed || hasAttempt, canPostSend: (post ? financeDocumentCommandAllowed(allowed,post.path,post) : allowed) || hasAttempt };
}
export function InvoiceEditor({
  base,
  options,
  invoice,
  invoiceLines = [],
  initialOpen = false,
  initialKind = "receivable",
  initialChartId,
}: {
  base: string;
  options: Options;
  invoice?: Row;
  invoiceLines?: PersistedInvoiceLine[];
  initialOpen?: boolean;
  initialKind?: "receivable" | "payable";
  initialChartId?: string;
}) {
  const draftScope = useFinanceDraftScope();
  const selected = useCurrency(),
    { send, error, busy, canSend } = useInvoiceCommand(base, "new-work");
  const [open, setOpen] = useState(initialOpen),
    [kind, setKind] = useState(String(invoice?.kind ?? initialKind)),
    [chartId, setChartId] = useState(
      String(
        options.accounts.find((a) => a.id === invoice?.debit_account_id)?.chart_id ??
          initialChartId ??
          options.charts.find((c) => c.name === "Operations ledger")?.id ??
          options.charts[0]?.id ??
          "",
      ),
    ),
    [currencyOverride, setCurrency] = useState<string | null>(null),
    [invoiceDate, setInvoiceDate] = useState(
      String(invoice?.invoice_date ?? new Date().toISOString().slice(0, 10)),
    );
  const closeEditor = useFinanceDraftClose(() => setOpen(false));
  const currency = currencyOverride ?? String(invoice?.currency ?? selected.displayCurrency);
  const [multiLine,setMultiLine]=useState(invoice?.line_mode==="native_lines"),
    [lines,setLines]=useState<InvoiceLineInput[]>(invoiceLines.filter(line=>line.origin_kind==="native_line").map(line=>({description:line.description,quantity:String(Number(line.quantity_millis)/1000),unitPrice:minorToDecimal(Number(line.unit_price_minor)),taxBps:Number(line.tax_bps),accountId:line.account_id,taxAccountId:line.tax_account_id??undefined})));
  let calculated:ReturnType<typeof calculateInvoiceLines>|null=null,lineError="";
  if(multiLine){try{calculated=calculateInvoiceLines(lines);}catch(cause){lineError=cause instanceof Error?cause.message:"Review invoice lines.";}}
  function updateLine(index:number,field:keyof InvoiceLineInput,value:string|number){setLines(current=>current.map((line,position)=>position===index?{...line,[field]:value}:line));}
  const accounts = options.accounts.filter(
      (a) => a.chart_id === chartId && a.currency === currency,
    ),
    chartCurrency = String(options.charts.find((c) => c.id === chartId)?.base_currency ?? ""),
    date = invoiceDate;
  let scaleReady = false;
  try {
    scaleReady = currencyScale(currency, selected.scales[currency]) === 2;
  } catch {}
  const rateReady =
      Boolean(
        resolveRate(selected.rates, currency, date) &&
        resolveRate(selected.rates, chartCurrency, date),
      ),
    partyReady = options.parties.some(
      (p) => p.currency === currency && p.kind === (kind === "payable" ? "supplier" : "customer"),
    ),
    canSave = scaleReady && rateReady && accounts.length >= 2 && partyReady && (!multiLine||(lines.length>0&&Boolean(calculated)&&!lineError));
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      {!initialOpen && (
        <Button appearance="standard" type="button" variant="primary" disabled={!canSend} onClick={() => open ? closeEditor() : setOpen(true)}>
          {invoice ? "Edit invoice draft" : "Create standalone invoice"}
        </Button>
      )}
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {open && (
        <FormDrawer appearance="standard" inline={initialOpen} open={open} onClose={closeEditor} title="Invoice details" dismissDisabled={busy}><Stack as="div" className="ui-stack">{error && <InlineNotice tone="danger" role="alert" className="error">{error}</InlineNotice>}<FormLayout data-finance-draft={draftScope}
          className="ui-form-grid"
          onSubmit={async (e) => {
            e.preventDefault();
            if (!canSave) return;
            const f = new FormData(e.currentTarget);
            if (
              await send(invoice ? `/${invoice.id}` : "", {
                kind,
                partyId: f.get("partyId"),
                invoiceNumber: f.get("number"),
                invoiceDate: f.get("date"),
                dueDate: f.get("dueDate"),
                currency,
                subtotal: multiLine?calculated!.subtotal:f.get("subtotal"),
                tax: multiLine?calculated!.tax:f.get("tax"),
                debitAccountId: f.get("debit"),
                creditAccountId: f.get("credit"),
                taxAccountId: f.get("taxAccount") || undefined,
                ...(invoice ? { expectedVersion: invoice.version } : {}),
                ...(multiLine?{lines:lines.map(line=>({...line,taxAccountId:line.taxAccountId||undefined}))}:{}),
              })
            )
              setOpen(false);
          }}
        >
          <Field appearance="standard">
            Invoice kind
            <Select appearance="standard" value={kind} onChange={(e) => setKind(e.target.value)}>
              <option value="receivable">Customer invoice</option>
              <option value="payable">Supplier bill</option>
            </Select>
          </Field>
          <Field appearance="standard">
            Ledger
            <Select appearance="standard" value={chartId} onChange={(e) => setChartId(e.target.value)}>
              {options.charts.map((c) => (
                <option key={String(c.id)} value={String(c.id)}>
                  {String(c.name)} · base {String(c.base_currency)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Transaction currency
            <Select appearance="standard" value={currency} onChange={(e) => setCurrency(e.target.value)}>
              {selected.currencies.map((item) => (
                <option key={item.code} value={item.code}>
                  {item.code} · {item.name}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Customer or supplier
            <Select appearance="standard" name="partyId" required defaultValue={String(invoice?.party_id ?? "")}>
              <option value="">Select party</option>
              {options.parties
                .filter(
                  (p) =>
                    p.currency === currency &&
                    p.kind === (kind === "payable" ? "supplier" : "customer"),
                )
                .map((p) => (
                  <option key={String(p.id)} value={String(p.id)}>
                    {String(p.name)}
                  </option>
                ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Invoice number
            <TextInput appearance="standard"
              name="number"
              required
              maxLength={100}
              defaultValue={String(invoice?.invoice_number ?? "")}
            />
          </Field>
          <Field appearance="standard">
            Invoice date
            <TextInput appearance="standard"
              name="date"
              type="date"
              required
              value={invoiceDate}
              onChange={(e) => setInvoiceDate(e.target.value)}
            />
          </Field>
          <Field appearance="standard">
            Due date
            <TextInput appearance="standard"
              name="dueDate"
              type="date"
              required
              defaultValue={String(invoice?.due_date ?? "")}
            />
          </Field>
          <Field appearance="standard">
            Subtotal ({currency})
            <TextInput appearance="standard"
              key={`subtotal:${multiLine}`}
              name="subtotal"
              type="number"
              min="0.01"
              step="0.01"
              required
              {...(multiLine?{readOnly:true,value:calculated?.subtotal??"0.00"}:{defaultValue:minorToDecimal(Number(invoice?.subtotal_minor ?? 0))})}
            />
          </Field>
          <Field appearance="standard">
            Tax amount
            <TextInput appearance="standard"
              key={`tax:${multiLine}`}
              name="tax"
              type="number"
              min="0"
              step="0.01"
              required
              {...(multiLine?{readOnly:true,value:calculated?.tax??"0.00"}:{defaultValue:minorToDecimal(Number(invoice?.tax_minor ?? 0))})}
            />
          </Field>
          {(
            [
              ["debit", "Debit account", invoice?.debit_account_id],
              ["credit", "Credit account", invoice?.credit_account_id],
              ["taxAccount", "Tax account", invoice?.tax_account_id],
            ] as const
          ).map(([name, label, value]) => (
            <Field appearance="standard" key={name}>
              {label}
              <Select appearance="standard"
                name={name}
                required={name !== "taxAccount"}
                defaultValue={String(value ?? "")}
              >
                <option value="">Select account</option>
                {accounts.map((a) => (
                  <option value={String(a.id)} key={String(a.id)}>
                    {String(a.code)} · {String(a.name)}
                  </option>
                ))}
              </Select>
            </Field>
          ))}
          <label><input type="checkbox" checked={multiLine} disabled={invoice?.line_mode==="native_lines"} onChange={e=>{setMultiLine(e.target.checked);if(e.target.checked&&!lines.length)setLines([{description:"",quantity:"1",unitPrice:"0.01",taxBps:0,accountId:""}]);}}/> Itemized document</label>
          {multiLine&&<Stack as="section" aria-label="Invoice line items" className="ui-stack">
            {lines.map((line,index)=><fieldset key={index} className="ui-form-grid"><legend>Line {index+1}</legend>
              <Field appearance="standard">Description<TextInput appearance="standard" required value={line.description} onChange={e=>updateLine(index,"description",e.target.value)}/></Field>
              <Field appearance="standard">Quantity<TextInput appearance="standard" type="number" required min="0.001" step="0.001" value={line.quantity} onChange={e=>updateLine(index,"quantity",e.target.value)}/></Field>
              <Field appearance="standard">Unit price ({currency})<TextInput appearance="standard" type="number" required min="0.01" step="0.01" value={line.unitPrice} onChange={e=>updateLine(index,"unitPrice",e.target.value)}/></Field>
              <Field appearance="standard">VAT percentage<TextInput appearance="standard" type="number" required min="0" max="100" step="0.01" value={line.taxBps/100} onChange={e=>updateLine(index,"taxBps",Math.round(Number(e.target.value)*100))}/></Field>
              <Field appearance="standard">Line account<Select appearance="standard" required value={line.accountId} onChange={e=>updateLine(index,"accountId",e.target.value)}><option value="">Select account</option>{accounts.map(account=><option key={String(account.id)} value={String(account.id)}>{String(account.code)} · {String(account.name)}</option>)}</Select></Field>
              <Field appearance="standard">Line VAT account<Select appearance="standard" value={line.taxAccountId??""} onChange={e=>updateLine(index,"taxAccountId",e.target.value)}><option value="">Use document tax account</option>{accounts.map(account=><option key={String(account.id)} value={String(account.id)}>{String(account.code)} · {String(account.name)}</option>)}</Select></Field>
              {calculated?.lines[index]&&<p>Net {minorToDecimal(calculated.lines[index].subtotalMinor)} · VAT {minorToDecimal(calculated.lines[index].taxMinor)}</p>}
              <Button appearance="standard" type="button" disabled={lines.length===1} onClick={()=>setLines(current=>current.filter((_line,position)=>position!==index))}>Remove line {index+1}</Button>
            </fieldset>)}
            <Button appearance="standard" type="button" disabled={lines.length>=100} onClick={()=>setLines(current=>[...current,{description:"",quantity:"1",unitPrice:"0.01",taxBps:0,accountId:""}])}>Add invoice line</Button>
            {lineError&&<InlineNotice tone="danger" role="alert">{lineError}</InlineNotice>}
          </Stack>}
          {!scaleReady && (
            <InlineNotice tone="danger" role="alert">
              This invoice workflow currently records two decimal places. Choose a supported
              currency while precision support is being completed.
            </InlineNotice>
          )}
          {!rateReady && (
            <InlineNotice tone="danger" role="alert">
              Confirm the manual USD rates for {currency} and {chartCurrency} before creating this
              invoice.
            </InlineNotice>
          )}
          {accounts.length < 2 && (
            <InlineNotice tone="danger" role="alert">
              Configure active debit and credit accounts in {currency} on this ledger.
            </InlineNotice>
          )}
          {!partyReady && (
            <InlineNotice tone="danger" role="alert">
              Configure a {currency} {kind === "payable" ? "supplier" : "customer"} first.
            </InlineNotice>
          )}
          <Button appearance="standard" type="submit" clientManaged variant="primary" disabled={busy || !canSave || !canSend}>
            Save invoice draft
          </Button>
        </FormLayout></Stack></FormDrawer>
      )}
    </Card>
  );
}
export function InvoiceActions({
  base,
  invoice,
  periods,
  canPost,
  sourceCanPost,
  canVoid = true,
  fxAccounts = [],
}: {
  base: string;
  invoice: Row;
  periods: Row[];
  canPost: boolean;
  sourceCanPost?: boolean;
  canVoid?: boolean;
  fxAccounts?: Row[];
}) {
  const draftScope = useFinanceDraftScope();
  const { send, error, busy, notice, canSend, canPostSend } = useInvoiceCommand(base, "settle-existing", {path:`/${invoice.id}/post`,canPost:sourceCanPost});
  const [reviewEvidence,setReviewEvidence]=useState<AttachmentPanelChange|null>(null);
  const [generation,setGeneration]=useState(0);
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      {notice && <p role="status">{notice}</p>}{error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {canPost && canPostSend && (
        <FormDisclosure appearance="standard" drawerTitle="Review invoice posting" error={error || undefined} dismissDisabled={busy} onDiscard={()=>{setReviewEvidence(null);setGeneration(value=>value+1);}}><summary>Review invoice posting</summary><FormLayout key={generation} data-finance-draft={draftScope}
          className="ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            void send(`/${invoice.id}/post`, {
              expectedVersion: invoice.version,
              periodId: f.get("period"),
              entryDate: f.get("date"),
              ...(f.get('fxGainAccountId')?{fxGainAccountId:f.get('fxGainAccountId')}:{}),
              ...(f.get('fxLossAccountId')?{fxLossAccountId:f.get('fxLossAccountId')}:{}),
              ...(invoice.kind==="payable"?{attachmentSetIds:reviewEvidence?.setId&&reviewEvidence.readyCount>0&&reviewEvidence.bindable?[reviewEvidence.setId]:[]}:{}),
            });
          }}
        >
          <Field appearance="standard">
            Posting period
            <Select appearance="standard" name="period" required>
              <option value="">Select open period</option>
              {periods.map((p) => (
                <option key={String(p.id)} value={String(p.id)}>
                  {String(p.name)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Posting date
            <TextInput appearance="standard"
              name="date"
              type="date"
              required
              defaultValue={String(invoice.invoice_date)}
            />
          </Field>
          <p className="muted">
            The approved company manual rate for the posting date will be used when the transaction
            currency differs from the ledger.
          </p>
          {invoice.source_type==='commerce_fulfillment'&&invoice.kind==='payable'&&fxAccounts.length>0&&<>
            <p className="muted">Review the goods FX accounts. Original receipt cost stays in inventory; any invoice-date carrying difference posts separately.</p>
            <Field appearance="standard">Goods FX gain account<Select appearance="standard" name="fxGainAccountId" required defaultValue=""><option value="">Choose reviewed gain account</option>{fxAccounts.filter(account=>account.category==='other_income').map(account=><option key={String(account.id)} value={String(account.id)}>{String(account.code)} · {String(account.name)}</option>)}</Select></Field>
            <Field appearance="standard">Goods FX loss account<Select appearance="standard" name="fxLossAccountId" required defaultValue=""><option value="">Choose reviewed loss account</option>{fxAccounts.filter(account=>account.category==='other_expense').map(account=><option key={String(account.id)} value={String(account.id)}>{String(account.code)} · {String(account.name)}</option>)}</Select></Field>
          </>}
          {invoice.kind==="payable"&&<AttachmentPanel appearance="standard" base={base} targetType="supplier_bill" targetId={String(invoice.id)} expectedTargetVersion={Number(invoice.version)} componentKind="action" componentRef="invoice-review" purposeKey="bill_match_support" title="Supplier bill review evidence" description="Attach the matching or correction evidence reviewed for this posting." visibility="Visible only to authorized Finance colleagues in this Company." onChange={setReviewEvidence}/>}
          <Button appearance="standard" type="submit" clientManaged variant="primary" disabled={busy||!canPostSend||Boolean(reviewEvidence?.links.length&&!reviewEvidence.bindable)}>
            Approve and post invoice
          </Button>
        </FormLayout></FormDisclosure>
      )}
      {canVoid && invoice.source_type === "standalone" && (
        <>
          <Button appearance="standard"
            type="button"
            variant="secondary"
            disabled={busy || !canSend}
            onClick={() => send(`/${invoice.id}/void`, { expectedVersion: invoice.version })}
          >
            Void draft
          </Button>
          <p className="muted">
            Voided drafts retain their audit history. Posted invoices require a credit-note
            workflow.
          </p>
        </>
      )}
    </Card>
  );
}
