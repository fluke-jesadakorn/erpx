"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { InlineNotice } from "@/components/ui/foundation";

import { ActionAnchor as UiActionAnchor } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { FinanceLegacyHeading } from "@/components/finance/finance-record-workspace";
import { RecordLink as Link } from "@/components/records/record-link";
import {workFetch} from "@/lib/work_projection/work-fetch";
import {useRefundCommand} from "./use-refund-command";
import {refundSourceCapabilities} from "@/lib/finance/refund-command-controls";
import {FormDisclosure} from "@/components/ui/form-disclosure";
import {RefundBankEvidence} from "./refund-bank-evidence";
import {useLayoutEffect,useRef,useState} from "react";
import {captureWorkCommandContext} from "@/lib/work_projection/command-context";
import type { refundWorkspace } from "@/server/modules/finance/public/refunds";
import { minorToDecimal } from "@/lib/finance/finance-money";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/files/attachment-panel";
import { Stack, Card, FormLayout } from "@/components/ui/foundation";
type Workspace = Awaited<ReturnType<typeof refundWorkspace>>;
export function RefundConsole({ base, data }: { base: string; data: Workspace }) {
  const row = data.owner;
  const sourceOnly=!!data.sourcePurchase && data.sourcePurchase.capability!=="finance.banking";
  const orderHref=typeof row.order_id==="string" && row.order_id
    ? canonicalUiHref(`${base}/${data.sourcePurchase?.capability==="finance.inventory-valuation" || row.direction==="incoming" ? "purchasing/orders" : "customers-sales/sales/documents/sales"}/${encodeURIComponent(row.order_id)}/overview`)
    : null;
  const invoiceHref=canonicalUiHref(`${base}/finance/invoices/${row.invoice_id}`);
  const returnHref=sourceOnly ? orderHref ?? invoiceHref : canonicalUiHref(`${base}/finance/refunds`);
  const {run,busy,error,setError,notice,draftScope,canSettle,canReadHistory}=useRefundCommand(base,refundSourceCapabilities(data.sourcePurchase),{ownerId:String(row.id),instructionIds:data.instructions.map(instruction=>String(instruction.id))});
  const
    [exported, setExported] = useState<{ content: string; sha256: string; owner: string; assertCurrent: () => void } | null>(null),
    [proofs, setProofs] = useState<Record<string, AttachmentPanelChange>>({});
  const money = (value: unknown) => `${row.currency} ${minorToDecimal(Number(value))}`;
  const [generations,setGenerations] = useState<Record<string,number>>({});
  const exportOwner = JSON.stringify([base,row.company_id,row.id]);
  const recoverGeneration = useRef(0),liveExportSource=useRef<{owner:string;canReadHistory:boolean}|null>({owner:exportOwner,canReadHistory});
  useLayoutEffect(()=>{liveExportSource.current={owner:exportOwner,canReadHistory};return()=>{liveExportSource.current=null;recoverGeneration.current++;};},[exportOwner,canReadHistory]);
  function captureExportSource(){
    const owner=exportOwner,assertWork=captureWorkCommandContext();
    return ()=>{assertWork();const live=liveExportSource.current;if(!live||live.owner!==owner||!live.canReadHistory)throw Object.assign(Error("Return to the original refund context before retrieving this export."),{code:"WORK_CONTEXT_CHANGED"});};
  }
  function resetEditor(key:string,proofId?:string) {setGenerations(current=>({...current,[key]:(current[key] ?? 0)+1}));if(proofId)setProofs(current=>Object.fromEntries(Object.entries(current).filter(([id])=>id!==proofId)));}
  async function send(path:string,input:unknown){
    if(busy)return;
    const assertCurrent=path==="refund-instructions/export"?captureExportSource():undefined;
    try{assertCurrent?.();const result=await run(path,input);if(assertCurrent&&result?.content&&result.sha256){assertCurrent();setExported({content:result.content,sha256:result.sha256,owner:exportOwner,assertCurrent});}}
    catch(cause){try{assertCurrent?.();setError(cause instanceof Error?cause.message:"Export unavailable.");}catch{/* The response belongs to an obsolete rendered source. */}}
  }
  async function recover(id: string) {
    if (!canReadHistory) return;
    const generation = ++recoverGeneration.current,assertCurrent=captureExportSource();
    try {
      assertCurrent();setError("");
      const response = await workFetch(`/api/v1${base}/finance/refund-instructions/${id}/export`);
      const body = await response.json();
      assertCurrent();
      if (!response.ok) throw new Error(body.error?.message ?? "Export unavailable.");
      if(typeof body.data?.content !== "string" || !body.data.content || typeof body.data?.sha256 !== "string" || !body.data.sha256)throw new Error("Export acknowledgement is missing.");
      assertCurrent();if(generation === recoverGeneration.current)setExported({content:body.data.content,sha256:body.data.sha256,owner:exportOwner,assertCurrent});
    } catch (cause) {
      try{assertCurrent();if(generation === recoverGeneration.current)setError(cause instanceof Error ? cause.message : "Export unavailable.");}catch{/* Do not publish an old source's error in the new context. */}
    }
  }
  const reserved = data.instructions
      .filter((i) => ["prepared", "exported", "unknown"].includes(String(i.status)))
      .reduce((total, i) => total + Number(i.amount_minor), 0),
    available = Number(row.outstanding_minor) - reserved;
  return (
    <Stack as="div" className="ui-stack">
      <FinanceLegacyHeading>
        <Link href={returnHref}>{sourceOnly ? "← Return to source" : "← Return refunds"}</Link>
        <h1>{String(row.return_number)} · Refund obligation</h1>
      </FinanceLegacyHeading>
      <Card as="section" appearance="standard" className="ui-panel">
        <p>
          {String(row.party_name)} ·{" "}
          {row.direction === "outgoing" ? "Pay customer" : "Receive from supplier"} ·{" "}
          {String(row.status)}
        </p>
        <p>
          Total {money(row.amount_minor)} · Remaining {money(row.outstanding_minor)} · Reserved{" "}
          {money(reserved)}
        </p>
        <Link href={invoiceHref}>Original invoice</Link>
        {orderHref && <> · <Link href={orderHref}>Return evidence</Link></>}
        <p>
          Created by {String(row.created_by)}
          {row.approved_by && <> · Approved by {String(row.approved_by)}</>}
        </p>
        {row.approved_by && (
          <p>
            Approved counterparty: {String(row.beneficiary_name)} · Account{" "}
            {String(row.beneficiary_account)}
            <br />
            {String(row.approval_reason)}
          </p>
        )}
      </Card>
      {notice && <p role="status">{notice}</p>}{error && (
        <InlineNotice tone="danger" className="error" role="alert">
          {error}
        </InlineNotice>
      )}
      {row.status === "pending" && data.canApprove && canSettle && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Independent approval</h2>
          <FormDisclosure appearance="standard" error={error} dismissDisabled={busy} drawerTitle="Review refund obligation" onDiscard={()=>resetEditor("approve")}><summary>Review refund obligation</summary><FormLayout key={generations.approve ?? 0} data-finance-draft={draftScope}
            className="ui-form-grid"
            onSubmit={(event) => {
              event.preventDefault();
              const form = new FormData(event.currentTarget);
              void send(`refunds/${row.id}/approve`, {
                expectedVersion: Number(row.version),
                beneficiaryName: form.get("name"),
                beneficiaryAccount: form.get("account"),
                reason: form.get("reason"),
              });
            }}
          >
            <Field appearance="standard">
              Counterparty name
              <TextInput appearance="standard"
                name="name"
                required
                minLength={2}
                maxLength={160}
                defaultValue={String(row.party_name)}
              />
            </Field>
            <Field appearance="standard">
              Verified counterparty bank account
              <TextInput appearance="standard" name="account" required minLength={3} maxLength={160} />
            </Field>
            <Field appearance="standard">
              Source and beneficiary review evidence
              <TextArea appearance="standard" name="reason" required minLength={3} maxLength={1000} />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={!canSettle || busy}>
              Approve refund obligation
            </Button>
          </FormLayout></FormDisclosure>
        </Card>
      )}
      {row.status === "approved" && data.canPay && canSettle && available > 0 && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Prepare partial or final instruction</h2>
          <FormDisclosure appearance="standard" error={error} dismissDisabled={busy} onDiscard={()=>resetEditor("prepare")} drawerTitle="Prepare refund instruction"><summary>Prepare partial or final instruction</summary>
          <FormLayout key={generations.prepare ?? 0} data-finance-draft={draftScope}
            className="ui-form-grid"
            onSubmit={(event) => {
              event.preventDefault();
              const form = new FormData(event.currentTarget);
              void send(`refunds/${row.id}/instructions`, {
                expectedVersion: Number(row.version),
                amount: form.get("amount"),
                plannedDate: form.get("date"),
                bankAccount: form.get("cash"),
                cashAccountId: form.get("cash"),
                ...(form.get("gain") ? { fxGainAccountId: form.get("gain") } : {}),
                ...(form.get("loss") ? { fxLossAccountId: form.get("loss") } : {}),
              });
            }}
          >
            <Field appearance="standard">
              Amount ({String(row.currency)})
              <TextInput appearance="standard"
                name="amount"
                type="number"
                required
                min="0.01"
                max={available / 100}
                step="0.01"
                defaultValue={minorToDecimal(available)}
              />
            </Field>
            <Field appearance="standard">
              Planned payment date
              <TextInput appearance="standard"
                name="date"
                type="date"
                required
                min={String(row.return_date)}
                defaultValue={new Date().toISOString().slice(0, 10)}
              />
            </Field>
            <Field appearance="standard">
              Bank GL account
              <Select appearance="standard" name="cash" required>
                <option value="">Select account</option>
                {data.accounts.map((a) => (
                  <option key={String(a.id)} value={String(a.id)}>
                    {String(a.code)} · {String(a.name)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Realized FX gain account
              <Select appearance="standard" name="gain">
                <option value="">Select when foreign cash may differ from the credit</option>
                {data.fxAccounts.filter((account) => account.category === "other_income").map((account) => (
                  <option key={String(account.id)} value={String(account.id)}>{String(account.code)} · {String(account.name)}</option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Realized FX loss account
              <Select appearance="standard" name="loss">
                <option value="">Select when foreign cash may differ from the credit</option>
                {data.fxAccounts.filter((account) => account.category === "other_expense").map((account) => (
                  <option key={String(account.id)} value={String(account.id)}>{String(account.code)} · {String(account.name)}</option>
                ))}
              </Select>
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={!canSettle || busy}>
              Reserve amount and prepare instruction
            </Button>
          </FormLayout></FormDisclosure>
        </Card>
      )}
      {data.canPay && canSettle && data.instructions.some((i) => i.status === "prepared") && (
        <Button appearance="standard"
          type="submit"
          variant="secondary"
          disabled={!canSettle || busy}
          onClick={() =>
            void send("refund-instructions/export", {
              instructions: data.instructions
                .filter((i) => i.status === "prepared")
                .map((i) => ({ id: String(i.id), expectedVersion: Number(i.version) })),
            })
          }
        >
          Export prepared instructions
        </Button>
      )}
      {exported && exported.owner===exportOwner && canReadHistory && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Manual bank instructions</h2>
          <p>
            This CSV is an instruction register, not a bank-specific upload format. The treasury
            officer must verify details in the bank system before paying.
          </p>
          <UiActionAnchor appearance="standard"
            variant="secondary"
            href={`data:text/csv;charset=utf-8,${encodeURIComponent(exported.content)}`}
            download="refund-instructions.csv"
            onClick={event=>{try{exported.assertCurrent();}catch{event.preventDefault();}}}
          >
            Download CSV
          </UiActionAnchor>
          <p>
            SHA-256 <code>{exported.sha256}</code>
          </p>
        </Card>
      )}
      <h2>Instructions and bank evidence</h2>
      {!data.instructions.length && <p>No instructions prepared yet.</p>}
      {data.instructions.map((i) => (
        <Card as="section" appearance="standard" className="ui-panel" key={String(i.id)}>
          <h3>
            {money(i.amount_minor)} · {String(i.status)}
          </h3>
          <p>
            {String(i.planned_date)} · Company bank {String(i.bank_account)}
          </p>
          {i.export_hash && (
            <p>
              Export SHA-256 <code>{String(i.export_hash)}</code>
              {data.canPay && canReadHistory && (
                <Button appearance="standard"
                  type="submit"
                  variant="secondary"
                  onClick={() => void recover(String(i.id))}
                >
                  Retrieve export
                </Button>
              )}
            </p>
          )}
          {i.evidence && <p>Evidence: {String(i.evidence)}</p>}
          {i.bank_reference && <p>Bank reference: {String(i.bank_reference)}</p>}
          {i.payment_id && (
            <Link href={canonicalUiHref(`${base}/finance/bank`)}>Matched bank payment · {String(i.payment_id)}</Link>
          )}
          {typeof row.company_id==="string" && <RefundBankEvidence source={{base,companyId:row.company_id,obligationId:String(row.id),instructionId:String(i.id),instructionVersion:Number(i.version),bankAccount:String(i.bank_account),currency:String(row.currency),amountMinor:Number(i.amount_minor),direction:row.direction==="outgoing"?"outgoing":"incoming",plannedDate:String(i.planned_date),functionalCurrency:data.bankEvidenceFunctionalCurrencies?.[String(i.id)]}} canImport={data.bankEvidenceImportable?.[String(i.id)]===true&&data.canPay&&row.status==="approved"&&["exported","unknown"].includes(String(i.status))&&!busy}/>}
          {["exported", "unknown"].includes(String(i.status)) && data.canPay && canSettle && (
            <FormDisclosure appearance="standard" error={error} dismissDisabled={busy} onDiscard={()=>resetEditor(`confirm:${i.id}`,String(i.id))} drawerTitle="Confirm refund bank result"><summary>Review bank result and proof</summary>
            <FormLayout key={generations[`confirm:${i.id}`] ?? 0} data-finance-draft={draftScope}
              className="ui-form-grid"
              onSubmit={(event) => {
                event.preventDefault();
                const form = new FormData(event.currentTarget),
                  bank = data.bank.find((b) => b.id === form.get("bank")),
                  proof = proofs[String(i.id)];
                if (!bank) return;
                void send(`refund-instructions/${i.id}/confirm`, {
                  expectedVersion: Number(i.version),
                  bankTransactionId: String(bank.id),
                  bankVersion: Number(bank.version),
                  periodId: form.get("period"),
                  bankReference: form.get("reference"),
                  evidence: form.get("evidence"),
                  attachmentSetIds:
                    proof?.setId && proof.readyCount > 0 && proof.bindable ? [proof.setId] : [],
                });
              }}
            >
              <Field appearance="standard">
                Imported bank transaction
                <Select appearance="standard" name="bank" required>
                  <option value="">Select matching bank evidence</option>
                  {data.bank
                    .filter(
                      (b) =>
                        b.bank_account === i.bank_account &&
                        Number(b.amount_minor) ===
                          (row.direction === "outgoing"
                            ? -Number(i.amount_minor)
                            : Number(i.amount_minor)) &&
                        String(b.transaction_date) >= String(i.planned_date),
                    )
                    .map((b) => (
                      <option key={String(b.id)} value={String(b.id)}>
                        {String(b.transaction_date)} · {String(b.external_id)} ·{" "}
                        {String(b.description)}
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
                targetType="finance_refund_obligation"
                targetId={String(row.id)}
                expectedTargetVersion={Number(row.version)}
                componentKind="action"
                componentRef={`refund-instruction:${String(i.id)}`}
                purposeKey="refund_proof"
                title="Refund bank proof"
                description="Attach the bank confirmation or refund payment proof for this exact instruction."
                visibility="Visible only to authorized Finance and Treasury reviewers."
                onChange={(change) =>
                  setProofs((current) => ({ ...current, [String(i.id)]: change }))
                }
              />
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={!canSettle || busy}>
                Confirm bank result and post refund
              </Button>
            </FormLayout></FormDisclosure>
          )}
          {canSettle && ((data.canPay && ["prepared", "exported"].includes(String(i.status))) ||
            (data.canReviewException &&
              i.prepared_by !== data.userId &&
              ["exported", "unknown"].includes(String(i.status)))) && (
            <FormDisclosure appearance="standard" error={error} dismissDisabled={busy} onDiscard={()=>resetEditor(`exception:${i.id}`)} drawerTitle="Review refund exception"><summary>Review refund exception</summary><FormLayout key={generations[`exception:${i.id}`] ?? 0} data-finance-draft={draftScope}
              className="ui-form-grid"
              onSubmit={(event) => {
                event.preventDefault();
                const form = new FormData(event.currentTarget);
                void send(`refund-instructions/${i.id}/exception`, {
                  expectedVersion: Number(i.version),
                  outcome: form.get("outcome"),
                  evidence: form.get("evidence"),
                });
              }}
            >
              <Field appearance="standard">
                Exception outcome
                <Select appearance="standard" name="outcome" required>
                  {i.status === "prepared" && data.canPay && (
                    <option value="cancelled">Cancel before export</option>
                  )}
                  {i.status === "exported" && data.canPay && (
                    <option value="unknown">Bank result uncertain — keep amount reserved</option>
                  )}
                  {data.canReviewException &&
                    i.prepared_by !== data.userId &&
                    ["exported", "unknown"].includes(String(i.status)) && (
                      <option value="failed">Bank confirmed failure — release reservation</option>
                    )}
                </Select>
              </Field>
              <Field appearance="standard">
                Evidence and reason
                <TextArea appearance="standard" name="evidence" required minLength={3} maxLength={1000} />
              </Field>
              <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={!canSettle || busy}>
                Record exception
              </Button>
            </FormLayout></FormDisclosure>
          )}
        </Card>
      ))}
    </Stack>
  );
}
