"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { InlineNotice } from "@/components/ui/foundation";

import { ActionAnchor as UiActionAnchor } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { FinanceLegacyHeading } from "@/components/finance/finance-record-workspace";
import { RecordLink as Link } from "@/components/records/record-link";
import styles from "@/components/procurement/supplier-batch.module.css";
import { useState } from "react";
import {useRouter} from "@/lib/shared/destination-router";
import {captureWorkCommandContext} from "@/lib/work_projection/command-context";
import {useFinanceActionAllowed} from "@/components/finance/purchase-context";
import {useSupplierBatchCommand} from "./use-supplier-batch-command";
import {type SupplierBatchReceipt,type SupplierBatchOwner} from "@/lib/procurement/supplier-batch-owner-command";
import {FormDisclosure} from "@/components/ui/form-disclosure";
import {FeatureWidget} from "@/components/shared/features/feature-widget";
import { minorToDecimal } from "@/lib/finance/finance-money";
import type { supplierBatchWorkspace } from "@/server/modules/procurement/public/supplier-payments";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/files/attachment-panel";
import { Card, FormLayout, FormGrid } from "@/components/ui/foundation";
type Workspace = Awaited<ReturnType<typeof supplierBatchWorkspace>>;
export function SupplierBatchConsole({base,data:currentData,owner,invoiceId,onLockChange,onCloseRetained}:{base:string;data:Workspace|null;owner?:SupplierBatchOwner;invoiceId?:string;onLockChange?:(locked:boolean)=>void;onCloseRetained?:()=>void}){
  const bankingSettle=useFinanceActionAllowed("settle-existing","finance.banking"),bankingHistory=useFinanceActionAllowed("history","finance.banking"),canSettle=bankingSettle||currentData?.sourcePurchase?.settleExisting===true,canHistory=bankingHistory||currentData?.sourcePurchase?.history===true;
  const [draftForms]=useState(()=>{const rows=new Set<WeakRef<HTMLFormElement>>();let revision=0;return{edited(){revision++;},revision(){return revision;},connected(){return [...rows].flatMap(row=>{const form=row.deref();return form?.isConnected?[form]:[];});},remember(form:HTMLFormElement){if(![...rows].some(row=>row.deref()===form))rows.add(new WeakRef(form));},forget(form:HTMLFormElement|undefined){for(const row of rows)if(row.deref()===form)rows.delete(row);},discard(){for(const row of rows){const form=row.deref();if(form?.isConnected){form.reset();}}rows.clear();}};});
  const {send:run,retry,retryAttempt,uncertain,retainedElsewhere,busy,error,notice,setError,draftScope,recoverExport,captureCurrent,discardAndRefresh}=useSupplierBatchCommand({base,owner,data:currentData,invoiceId,canSettle,canHistory,draft:()=>({revision:draftForms.revision(),forms:draftForms.connected()})},onLockChange);
  const ownerKey=JSON.stringify([base,owner??null,invoiceId??null]);
  const [retained,setRetained]=useState<{owner:string;data:Workspace}|null>(null),[generation,setGeneration]=useState(0);
  const view=retained?.owner===ownerKey?retained.data:currentData;
  const [selected,setSelected]=useState<string[]>(invoiceId&&currentData?.invoices.some(row=>row.id===invoiceId)?[invoiceId]:[]),[query,setQuery]=useState(''),[exportRecord,setExportRecord]=useState<{owner:string;batchId:string;content:string;sha256:string;assertCurrent:()=>void}|null>(null),[proofs,setProofs]=useState<Record<string,AttachmentPanelChange>>({}),[created,setCreated]=useState<{owner:string;id:string}|null>(null);
  if(!view)return retainedElsewhere?<InlineNotice role="status">An original supplier request is retained for its original Company and role. Return there to recover it.</InlineNotice>:null;
  const data=view,b=data.batch,createdBatch=created?.owner===ownerKey?created.id:null,canPay=!!b&&data.canPay&&data.userId!==b.created_by&&data.userId!==b.approved_by;
  const exportAllowed=()=>{if(!exportRecord||exportRecord.owner!==ownerKey||!canHistory||!currentData?.canPay||currentData.userId===currentData.batch?.created_by||currentData.userId===currentData.batch?.approved_by||currentData.batch?.id!==exportRecord.batchId)return false;try{exportRecord.assertCurrent();return true;}catch{return false;}};
  const exported=exportAllowed()?exportRecord:null;
  function remember(form?:HTMLFormElement){if(form)draftForms.remember(form);if(!retained&&!retainedElsewhere)setRetained({owner:ownerKey,data});}
  function discardDraft(){if(busy||retryAttempt||retainedElsewhere)return;discardAndRefresh(()=>{draftForms.discard();setRetained(null);setGeneration(value=>value+1);setSelected([]);setProofs({});setError('');});}
  function saved(result:SupplierBatchReceipt,unchanged:boolean){
    if(typeof result.content==='string'&&typeof result.sha256==='string'&&b)setExportRecord({owner:ownerKey,batchId:String(b.id),content:result.content,sha256:result.sha256,assertCurrent:captureCurrent()});
    if(!b)setCreated({owner:ownerKey,id:result.id});
    if(unchanged){setRetained(null);setSelected([]);setProofs({});}
  }
  async function send(path:string,input:unknown,form?:HTMLFormElement){if(busy||createdBatch&&!b)return;remember(form);const original=form?new WeakRef(form):undefined;await run(path,input,form,(result,unchanged)=>{if(unchanged){draftForms.forget(original?.deref());}saved(result,unchanged);});}
  function proofChanged(id:string,change:AttachmentPanelChange){if(JSON.stringify(proofs[id])===JSON.stringify(change))return;const previous=proofs[id];setProofs(current=>({...current,[id]:change}));if(change.setId||previous?.setId){remember();draftForms.edited();}}
  async function recover(){if(!b)return;await recoverExport(value=>setExportRecord({owner:ownerKey,batchId:String(b.id),...value,assertCurrent:captureCurrent()}));}
  const recovery=retryAttempt?<><InlineNotice role="status">The original action and key are retained. Retry never submits newer fields. Current native Treasury, evidence and source rules still apply.</InlineNotice><Button appearance="standard" type="button" disabled={busy} onClick={()=>void retry()}>Retry submitted payment action</Button></>:null;
  return (
    <div key={ownerKey+':'+generation} className={b ? `content-stack ${styles.console}` : styles.console} data-supplier-owner-pending={uncertain?'true':undefined} onInputCapture={event=>{const form=event.target instanceof Element?event.target.closest<HTMLFormElement>('form[data-finance-draft]'):null;if(form){remember(form);draftForms.edited();}}} onChangeCapture={event=>{const form=event.target instanceof Element?event.target.closest<HTMLFormElement>('form[data-finance-draft]'):null;if(form){remember(form);draftForms.edited();}}}>
      {createdBatch&&!b&&<p>Saved batch: <Link href={canonicalUiHref(`${base}/finance/supplier-batches/${createdBatch}`)}>Open the retained batch</Link>. Newer form edits remain here. Review that saved batch before preparing another instruction.</p>}
      {b && (
        <>
          <FinanceLegacyHeading>
            {!data.sourcePurchase&&<Link href={canonicalUiHref(`${base}/finance/supplier-batches`)}>← Supplier payment batches</Link>}
            <h1>Supplier payment batch</h1>
          </FinanceLegacyHeading>
          <Card as="section" appearance="standard" className="ui-panel">
            <p>
              {String(b.planned_date)} · {String(b.currency)} · {String(b.status)}
            </p>
            <p>
              Prepared by {String(b.created_by)}
              {b.approved_by && <> · Approved by {String(b.approved_by)}</>}
            </p>
            {b.approval_reason && <p>Approval evidence: {String(b.approval_reason)}</p>}
          </Card>
        </>
      )}
      {notice&&<InlineNotice tone="info" role="status">{notice}</InlineNotice>}
      {recovery}{retainedElsewhere&&<InlineNotice role="status">An original request belongs to the previous Company or work role. Return there before another supplier action.</InlineNotice>}
      {!onCloseRetained&&retained&&retained.owner===ownerKey&&!retryAttempt&&!retainedElsewhere&&<Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={()=>{if(window.confirm('Discard the unsubmitted supplier fields and review the current source?'))discardDraft();}}>Discard draft and review current source</Button>}
      {onCloseRetained&&<Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={onCloseRetained}>Close and retain draft</Button>}
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      {!b && data.canCreate && (canSettle||!!createdBatch||!!retryAttempt) && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Prepare supplier instructions</h2>
          <p>
            Select up to 50 posted invoices in one currency and chart. Review bank details before
            approval. Available balances exclude other pending instructions.
          </p>
          <FormLayout data-finance-draft={draftScope}
            className="ui-form-grid"
            onSubmit={(event) => {
              event.preventDefault();
              const form = new FormData(event.currentTarget);
              void send("supplier-batches", {
                bankAccountId: form.get("bank"),
                plannedDate: form.get("date"),
                items: data.invoices
                  .filter((i) => selected.includes(String(i.id)))
                  .map((i) => ({
                    invoiceId: String(i.id),
                    expectedVersion: Number(i.version),
                    amount: form.get(`amount-${i.id}`),
                    beneficiaryName: form.get(`name-${i.id}`),
                    beneficiaryAccount: form.get(`account-${i.id}`),
                  })),
              },event.currentTarget);
            }}
          >
            <Field appearance="standard">
              Company bank ledger
              <Select appearance="standard" name="bank" required>
                <option value="">Select bank ledger</option>
                {data.accounts.map((a) => (
                  <option key={String(a.id)} value={String(a.id)}>
                    {String(a.code)} · {String(a.name)} · {String(a.currency)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Planned payment date
              <TextInput appearance="standard" name="date" type="date" required />
            </Field>
            <Field appearance="standard">
              Find supplier invoices
              <TextInput appearance="standard"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Invoice number or supplier"
              />
            </Field>
            <p className="muted">{selected.length} of 50 selected</p>
            {data.invoices
              .filter(
                (i) =>
                  Number(i.available_minor) > 0 &&
                  (selected.includes(String(i.id)) ||
                    `${i.invoice_number} ${i.party_name}`
                      .toLowerCase()
                      .includes(query.trim().toLowerCase())),
              )
              .map((i) => (
                <fieldset key={String(i.id)}>
                  <legend>
                    <Field appearance="standard" orientation="inline">
                      <TextInput appearance="standard"
                        type="checkbox"
                        disabled={selected.length >= 50 && !selected.includes(String(i.id))}
                        checked={selected.includes(String(i.id))}
                        onChange={(event) =>
                          setSelected((values) =>
                            event.target.checked
                              ? [...values, String(i.id)]
                              : values.filter((id) => id !== i.id),
                          )
                        }
                      />{" "}
                      {String(i.invoice_number)} · {String(i.party_name)} · Available{" "}
                      {String(i.currency)} {minorToDecimal(Number(i.available_minor))}
                    </Field>
                  </legend>
                  <FormGrid as="div" className="ui-form-grid" hidden={!selected.includes(String(i.id))}>
                    <Field appearance="standard">
                      Amount
                      <TextInput appearance="standard"
                        disabled={!selected.includes(String(i.id))}
                        name={`amount-${i.id}`}
                        type="number"
                        step="0.01"
                        min="0.01"
                        max={Number(i.available_minor) / 100}
                        defaultValue={minorToDecimal(Number(i.available_minor))}
                        required
                      />
                    </Field>
                    <Field appearance="standard">
                      Verified beneficiary name
                      <TextInput appearance="standard"
                        disabled={!selected.includes(String(i.id))}
                        name={`name-${i.id}`}
                        defaultValue={String(i.party_name)}
                        required
                        minLength={2}
                        maxLength={160}
                      />
                    </Field>
                    <Field appearance="standard">
                      Verified beneficiary account
                      <TextInput appearance="standard"
                        disabled={!selected.includes(String(i.id))}
                        name={`account-${i.id}`}
                        required
                        minLength={3}
                        maxLength={160}
                      />
                    </Field>
                  </FormGrid>
                </fieldset>
              ))}
            {!data.invoices.length && (
              <p>No eligible posted supplier invoices. Create and post the source invoice first.</p>
            )}
            {data.invoices.length === 200 && (
              <p>Showing the first 200 outstanding invoices by due date.</p>
            )}
            <Button appearance="standard"
              clientManaged type="submit"
              variant="primary"
              disabled={!canSettle || busy || !!createdBatch || !selected.length || selected.length > 50}
            >
              Reserve selected amounts
            </Button>
          </FormLayout>
        </Card>
      )}
      {b && (canSettle||busy||!!retryAttempt) &&
        ["draft", "approved"].includes(String(b.status)) &&
        ((b.status === "draft" && data.canApprove && b.created_by !== data.userId) ||
          (data.canCreate && (b.created_by === data.userId || data.canApprove))) && (
          <Card as="section" appearance="standard" className="ui-panel">
            <h2>Review the batch</h2>
            <FormDisclosure appearance="standard" error={error} drawerTitle="Review supplier batch" retainFormContents={!!retained||uncertain} dismissDisabled={busy} discardLabel={uncertain?"Close and retain request":undefined} ><summary>Review decision and evidence</summary>{recovery}<FormLayout data-finance-draft={draftScope}
              className="ui-form-grid"
              onSubmit={(event) => {
                event.preventDefault();
                const form = new FormData(event.currentTarget);
                void send(`supplier-batches/${b.id}/${form.get("decision")}`, {
                  expectedVersion: Number(b.version),
                  reason: form.get("reason"),
                },event.currentTarget);
              }}
            >
              <Field appearance="standard">
                Decision
                <Select appearance="standard" name="decision">
                  {b.status === "draft" && data.canApprove && b.created_by !== data.userId && (
                    <option value="approve">Approve sources, amounts and beneficiaries</option>
                  )}
                  {data.canCreate && (b.created_by === data.userId || data.canApprove) && (
                    <option value="cancel">Cancel and release reservations</option>
                  )}
                </Select>
              </Field>
              <Field appearance="standard">
                Review evidence or cancellation reason
                <TextArea appearance="standard" name="reason" minLength={3} maxLength={1000} required />
              </Field>
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={!canSettle || busy}>
                Record decision
              </Button>
            </FormLayout></FormDisclosure>
          </Card>
        )}
      {b && b.status === "approved" && canPay && canSettle && (
        <Button appearance="standard"
          type="submit"
          variant="primary"
          disabled={!canSettle || busy}
          onClick={() =>
            void send(`supplier-batches/${b.id}/export`, { expectedVersion: Number(b.version) })
          }
        >
          Export approved instructions
        </Button>
      )}
      {b?.export_hash && (
        <Card as="section" appearance="standard" className="ui-panel">
          <p>
            Export SHA-256 <code>{String(b.export_hash)}</code>
          </p>
          {canPay && canHistory && (
            <Button appearance="standard" type="submit" variant="secondary" onClick={() => void recover()}>
              Retrieve original export
            </Button>
          )}
        </Card>
      )}
      {exported && (
        <Card as="section" appearance="standard" className="ui-panel">
          <p>
            This CSV is a manual instruction register. Verify the beneficiary in the bank system
            before paying. Export does not post cash or settle invoices.
          </p>
          <UiActionAnchor appearance="standard"
            variant="secondary"
            download="supplier-instructions.csv"
            onClick={event=>{if(!exportAllowed()){event.preventDefault();setError("Review the current independent Treasury source and retrieve the original export again.");}}} href={`data:text/csv;charset=utf-8,${encodeURIComponent(exported.content)}`}
          >
            Download CSV
          </UiActionAnchor>
          <p>
            SHA-256 <code>{exported.sha256}</code>
          </p>
        </Card>
      )}
      {b &&
        data.items.map((i) => (
          <Card as="section" appearance="standard" className="ui-panel" key={String(i.id)}>
            <h2>
              <Link href={canonicalUiHref(`${base}/finance/invoices/${i.invoice_id}`)}>
                {String(i.invoice_number)}
              </Link>{" "}
              · {String(i.currency)} {minorToDecimal(Number(i.amount_minor))}
            </h2>
            <p>
              {String(i.beneficiary_name)} · Account {String(i.beneficiary_account)} ·{" "}
              {String(i.status)}
            </p>
            {i.status === "failed" && (
              <p>
                Review the source invoice balance and{" "}
                <Link href={canonicalUiHref(`${base}/finance/invoices/${i.invoice_id}`)}>prepare a new instruction from its source invoice</Link> if
                it is still unpaid.
              </p>
            )}
            {i.evidence && <p>Evidence: {String(i.evidence)}</p>}
            {i.bank_reference && <p>Bank reference: {String(i.bank_reference)}</p>}
            {i.payment_id && (
              <Link href={canonicalUiHref(`${base}/finance/payments/${i.payment_id}`)}>Payment and allocation</Link>
            )}
            {["exported", "unknown"].includes(String(i.status)) && canPay && (canSettle||busy||!!retryAttempt) && (
              <FormDisclosure appearance="standard" error={error} dismissDisabled={busy} retainFormContents={!!retained||uncertain} discardLabel={uncertain?"Close and retain request":undefined}  drawerTitle="Confirm supplier bank debit"><summary>Review bank debit and proof</summary>{recovery}
              <FormLayout data-finance-draft={draftScope}
                className="ui-form-grid"
                onSubmit={(event) => {
                  event.preventDefault();
                  const form = new FormData(event.currentTarget),
                    bank = data.bank.find((t) => t.id === form.get("bank")),
                    proof = proofs[String(i.id)];
                  if (!bank) return;
                  void send(`supplier-instructions/${i.id}/confirm`, {
                    expectedVersion: Number(i.version),
                    bankTransactionId: String(bank.id),
                    bankVersion: Number(bank.version),
                    periodId: form.get("period"),
                    bankReference: form.get("reference"),
                    evidence: form.get("evidence"),
                    attachmentSetIds:
                      proof?.setId && proof.readyCount > 0 && proof.bindable ? [proof.setId] : [],
                  },event.currentTarget);
                }}
              >
                <Field appearance="standard">
                  Matching bank debit
                  <Select appearance="standard" name="bank" required>
                    <option value="">Select imported bank evidence</option>
                    {data.bank
                      .filter(
                        (t) =>
                          Number(t.amount_minor) === -Number(i.amount_minor) &&
                          String(t.transaction_date) >= String(b.planned_date),
                      )
                      .map((t) => (
                        <option key={String(t.id)} value={String(t.id)}>
                          {String(t.transaction_date)} · {String(t.external_id)} ·{" "}
                          {String(t.description)}
                        </option>
                      ))}
                  </Select>
                </Field>
                <Field appearance="standard">
                  Open posting period
                  <Select appearance="standard" name="period" required>
                    <option value="">Select period</option>
                    {data.periods.map((p) => (
                      <option key={String(p.id)} value={String(p.id)}>
                        {String(p.name)}
                      </option>
                    ))}
                  </Select>
                </Field>
                <Field appearance="standard">
                  Actual bank reference
                  <TextInput appearance="standard" name="reference" required minLength={2} maxLength={160} />
                </Field>
                <Field appearance="standard">
                  Verification evidence
                  <TextArea appearance="standard" name="evidence" required minLength={3} maxLength={1000} />
                </Field>
                <AttachmentPanel appearance="standard"
                  base={base}
                  targetType="finance_supplier_batch"
                  targetId={String(b.id)}
                  expectedTargetVersion={Number(b.version)}
                  componentKind="action"
                  componentRef={`supplier-instruction:${String(i.id)}`}
                  purposeKey="payment_proof"
                  title="Supplier payment proof"
                  description="Attach the bank confirmation for this exact supplier instruction."
                  visibility="Visible only to authorized Company Finance and Treasury reviewers."
                  onChange={change=>proofChanged(String(i.id),change)}
                />
                <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={!canSettle || busy}>
                  Confirm bank debit and settle invoice
                </Button>
              </FormLayout></FormDisclosure>
            )}
            {["exported", "unknown"].includes(String(i.status)) &&
              (canSettle||busy||!!retryAttempt) && (canPay || (data.canApprove && data.userId !== b.exported_by)) && (
                <FormDisclosure appearance="standard" error={error} drawerTitle="Review supplier bank exception" retainFormContents={!!retained||uncertain} dismissDisabled={busy} discardLabel={uncertain?"Close and retain request":undefined} ><summary>Record bank exception</summary>{recovery}<FormLayout data-finance-draft={draftScope}
                  className="ui-form-grid"
                  onSubmit={(event) => {
                    event.preventDefault();
                    const form = new FormData(event.currentTarget);
                    void send(`supplier-instructions/${i.id}/result`, {
                      expectedVersion: Number(i.version),
                      outcome: form.get("outcome"),
                      evidence: form.get("evidence"),
                    },event.currentTarget);
                  }}
                >
                  <Field appearance="standard">
                    Bank exception
                    <Select appearance="standard" name="outcome">
                      {canPay && canHistory && (
                        <option value="unknown">Result uncertain — retain reservation</option>
                      )}
                      {data.canApprove && data.userId !== b.exported_by && (
                        <option value="failed">
                          Bank confirmed no payment — release reservation
                        </option>
                      )}
                    </Select>
                  </Field>
                  <Field appearance="standard">
                    Evidence
                    <TextArea appearance="standard" name="evidence" required minLength={3} maxLength={1000} />
                  </Field>
                  <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={!canSettle || busy}>
                    Record bank exception
                  </Button>
                </FormLayout></FormDisclosure>
              )}
          </Card>
        ))}
    </div>
  );
}

/** The list and explicit new view share the same typed owner editor. Explicit
 * discard remounts only this local draft; it never cancels a saved batch. */
export function SupplierBatchPreparation({base,data,owner,invoiceId,feature}:{base:string;data:Workspace|null;owner?:SupplierBatchOwner;invoiceId?:string;feature?:{id:string;title:string}}){
 const router=useRouter(),[revision,setRevision]=useState(0),[locked,setLocked]=useState(false),[open,setOpen]=useState(false),[visited,setVisited]=useState(false);
 if(!data&&!visited)return null;
 const content=<><FormDisclosure appearance="standard" open={open} onToggle={event=>setOpen(event.currentTarget.open)} retainFormContents={visited||locked} drawerTitle="Prepare supplier payment batch" discardLabel={locked?"Close and retain request":undefined} onDiscard={locked?undefined:()=>{setRevision(value=>value+1);setOpen(false);try{const guard=captureWorkCommandContext(),node=document.querySelector<HTMLElement>("[data-work-revision]");guard();if(owner&&node?.dataset.workUser===owner.userId&&node.dataset.workCompany===owner.companyId&&node.dataset.workAssignment===owner.assignmentId)router.refresh();}catch{/* Explicit known-draft discard does not authorize a different Work refresh. */}}}><summary onClick={()=>setVisited(true)}>Prepare batch</summary><SupplierBatchConsole key={revision} base={base} data={data} owner={owner} invoiceId={invoiceId} onLockChange={setLocked} onCloseRetained={()=>setOpen(false)}/></FormDisclosure>{visited&&!open&&<Button appearance="standard" type="button" onClick={()=>setOpen(true)}>Resume supplier batch draft</Button>}</>;
 return feature?<FeatureWidget appearance="standard" id={feature.id} title={feature.title}>{content}</FeatureWidget>:content;
}
