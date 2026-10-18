"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Dialog } from "@/components/ui/dialog";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useState } from "react";
import { useRouter } from "@/lib/shared/destination-router";
import { useFinanceCommand } from "./use-finance-command";
import { useFinanceDraftClose } from "./finance-draft-boundary";
import { minorToDecimal } from "@/lib/finance/finance-money";
import { vatTreatmentLabels } from "@/lib/finance/finance-vat-labels";
import { useHydrated } from "@/components/ui/use-hydrated";
import type { TaxRow } from "@/server/modules/finance/public/finance-tax-register";
import { FormLayout, Card, Stack } from "@/components/ui/foundation";

function useTaxCommand(base:string){return useFinanceCommand(base,"finance.tax");}
export function VatEntryPreparation({ base, candidates }: { base: string; candidates: TaxRow[] }) {
  const { send, pending, error, draftScope, canSettle } = useTaxCommand(base),
    [selected, setSelected] = useState(String(candidates[0]?.id ?? ""));
  const source = candidates.find((c) => c.id === selected);
  return (
    <FormDisclosure appearance="standard" error={error} drawerTitle="Prepare VAT entry"><summary>Prepare VAT entry</summary>
    <FormLayout data-finance-draft={draftScope}
      className="ui-panel ui-form-grid"
      onSubmit={async (e) => {
        e.preventDefault();
        if (!source) return;
        const element=e.currentTarget,editVersion=element.getAttribute("data-erpx-edit-version"),form = new FormData(element);
        const result = await send("tax-entries", {
          sourceType: source.source_type ?? "invoice",
          sourceId: source.id,
          expectedSourceVersion: Number(source.version),
          taxPointDate: form.get("taxPointDate"),
          claimMonth: form.get("claimMonth"),
          documentReference: form.get("documentReference"),
          treatment: form.get("treatment"),
          deductibleBps: Number(form.get("deductibleBps")),
          legalBasis: form.get("legalBasis"),
          taxPointEvidence: form.get("taxPointEvidence"),
          documentEvidence: form.get("documentEvidence"),
        });
        if (result&&element.getAttribute("data-erpx-edit-version")===editVersion) setSelected("");
      }}
    >
      <h2>Prepare VAT entry from source</h2>
      <p className="muted">
        Use this when the whole document has one tax point. Amounts come from the source and GL. A
        different reviewer confirms the tax point and input VAT entitlement. Advance or partial
        receipts require a supported tax-point adapter.
      </p>
      <Field appearance="standard">
        Source document
        <Select appearance="standard" value={selected} onChange={(e) => setSelected(e.target.value)} required>
          <option value="">Select document</option>
          {candidates.map((c) => (
            <option key={String(c.id)} value={String(c.id)}>
              {String(c.invoice_number ?? c.document_number)} · {String(c.party_name)} · VAT{" "}
              {minorToDecimal(Number(c.tax_minor))}
            </option>
          ))}
        </Select>
      </Field>
      {source && (
        <div className="muted">
          Document date {String(source.invoice_date ?? source.document_date)} · Accounting date{" "}
          {String(source.accounting_date)} · Taxable {minorToDecimal(Number(source.subtotal_minor))}{" "}
          · VAT {minorToDecimal(Number(source.tax_minor))} THB
        </div>
      )}
      <Field appearance="standard">
        Tax document reference in evidence
        <TextInput appearance="standard" name="documentReference" required maxLength={200} />
      </Field>
      <Field appearance="standard">
        Tax-point date
        <TextInput appearance="standard" name="taxPointDate" type="date" required />
      </Field>
      <Field appearance="standard">
        Reporting / claim month
        <TextInput appearance="standard" name="claimMonth" type="month" required />
      </Field>
      <Field appearance="standard">
        VAT treatment
        <Select appearance="standard" name="treatment" defaultValue="standard">
          <option value="standard">Standard</option>
          <option value="zero">Zero rated</option>
          <option value="exempt">Exempt</option>
          <option value="disallowed">Disallowed input VAT</option>
          <option value="allocated">Allocated input VAT</option>
        </Select>
      </Field>
      <Field appearance="standard">
        Input VAT entitlement (10,000 = 100%)
        <TextInput appearance="standard"
          name="deductibleBps"
          type="number"
          min={0}
          max={10000}
          step={1}
          defaultValue={10000}
          required
        />
      </Field>
      <Field appearance="standard">
        Applicable tax rules and legal sources
        <TextArea appearance="standard" name="legalBasis" required minLength={10} maxLength={4000} />
      </Field>
      <Field appearance="standard">
        Tax-point facts and evidence
        <TextArea appearance="standard" name="taxPointEvidence" required minLength={10} maxLength={4000} />
      </Field>
      <Field appearance="standard">
        Supplier document / tax invoice evidence
        <TextArea appearance="standard" name="documentEvidence" required minLength={10} maxLength={4000} />
      </Field>
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={!canSettle || pending || !source}>
        {pending ? "Preparing…" : "Save draft for review"}
      </Button>
    </FormLayout></FormDisclosure>
  );
}
export function VatEntryReview({ base, entry }: { base: string; entry: TaxRow }) {
  const { send, pending, error, draftScope, canSettle } = useTaxCommand(base),
    [reason, setReason] = useState("");
  return (
    <FormLayout data-finance-draft={draftScope} onSubmit={event=>event.preventDefault()}>
      <Field appearance="standard">
        Review reason {String(entry.document_reference)}
        <TextArea appearance="standard"
          value={reason}
          onChange={(e) => setReason(e.target.value)}
          minLength={10}
          maxLength={4000}
        />
      </Field>
      <div className="ui-form-actions">
        <Button appearance="standard"
          type="button"
          variant="primary"
          disabled={!canSettle || pending || reason.trim().length < 10}
          onClick={() =>
            void send(`tax-entries/${entry.id}/approve`, {
              expectedVersion: Number(entry.version),
              reason,
            })
          }
        >
          Approve VAT entry
        </Button>
        <Button appearance="standard"
          type="button"
          variant="secondary"
          disabled={!canSettle || pending || reason.trim().length < 10}
          onClick={() =>
            void send(`tax-entries/${entry.id}/reject`, {
              expectedVersion: Number(entry.version),
              reason,
            })
          }
        >
          Return with reason
        </Button>
      </div>
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
    </FormLayout>
  );
}
export function VatCorrectionPreparation({
  base,
  entry,
  currentSourceVersion,
}: {
  base: string;
  entry: TaxRow;
  currentSourceVersion: number;
}) {
  const { send, pending, error, draftScope, canSettle } = useTaxCommand(base),
    router = useRouter(),
    facts = JSON.parse(String(entry.evidence_json)).facts;
  const ready = useHydrated(),
    [expanded, setExpanded] = useState(false),
    [treatment, setTreatment] = useState(String(entry.treatment)),
    [percent, setPercent] = useState(Number(entry.deductible_bps) / 100);
  const close=useFinanceDraftClose(()=>setExpanded(false));
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <Button appearance="standard"
        variant="secondary"
        type="button"
        aria-expanded={expanded}
        disabled={!canSettle || !ready || pending}
        onClick={() => setExpanded(!expanded)}
      >
        Prepare correction to approved entry
      </Button>
      {expanded && (
        <Dialog appearance="standard" open={expanded} onClose={close} title="Prepare VAT correction" presentation="form-drawer" dismissDisabled={pending}><Stack as="div" className="ui-stack">{error && <InlineNotice tone="danger" role="alert" className="error">{error}</InlineNotice>}<FormLayout data-finance-draft={draftScope}
          className="ui-form-grid"
          onSubmit={async (event) => {
            event.preventDefault();
            const f = new FormData(event.currentTarget);
            const result = await send(`tax-entries/${entry.id}/corrections`, {
              expectedEntryVersion: Number(entry.version),
              expectedSourceVersion: currentSourceVersion,
              documentReference: f.get("documentReference"),
              taxPointDate: f.get("taxPointDate"),
              claimMonth: f.get("claimMonth"),
              treatment,
              deductibleBps: Math.round(percent * 100),
              legalBasis: f.get("legalBasis"),
              taxPointEvidence: f.get("taxPointEvidence"),
              documentEvidence: f.get("documentEvidence"),
              reason: f.get("reason"),
            });
            if (result) router.push(canonicalUiHref(`${base}/finance/tax/corrections/${result.id}`));
          }}
        >
          <p className="muted">
            Original entries and exports are retained. New entries and linked return adjustments
            require independent review as one correction set. Taxable and VAT amounts remain tied to
            the GL source.
          </p>
          <p>
            Taxable {minorToDecimal(Number(entry.taxable_minor))} · VAT{" "}
            {minorToDecimal(Number(entry.tax_minor))} THB
          </p>
          <Field appearance="standard">
            Tax document reference in evidence
            <TextInput appearance="standard"
              name="documentReference"
              defaultValue={String(entry.document_reference)}
              required
              maxLength={200}
            />
          </Field>
          <Field appearance="standard">
            Tax-point date
            <TextInput appearance="standard"
              name="taxPointDate"
              type="date"
              defaultValue={String(entry.tax_point_date)}
              required
            />
          </Field>
          <Field appearance="standard">
            Reporting / claim month
            <TextInput appearance="standard"
              name="claimMonth"
              type="month"
              defaultValue={String(entry.claim_month)}
              required
            />
          </Field>
          <Field appearance="standard">
            VAT treatment
            <Select appearance="standard"
              value={treatment}
              onChange={(e) => {
                setTreatment(e.target.value);
                if (e.target.value !== "allocated")
                  setPercent(e.target.value === "disallowed" ? 0 : 100);
              }}
            >
              {Object.entries(vatTreatmentLabels)
                .filter(
                  ([key]) =>
                    entry.direction === "input" || !["disallowed", "allocated"].includes(key),
                )
                .map(([key, label]) => (
                  <option key={key} value={key}>
                    {label}
                  </option>
                ))}
            </Select>
          </Field>
          {entry.direction === "input" && (
            <Field appearance="standard">
              Input VAT entitlement (%)
              <TextInput appearance="standard"
                type="number"
                min={0}
                max={100}
                step="0.01"
                value={percent}
                onChange={(e) => setPercent(Number(e.target.value))}
                required
              />
            </Field>
          )}
          <Field appearance="standard">
            Applicable tax rules and legal sources
            <TextArea appearance="standard"
              name="legalBasis"
              defaultValue={facts.legalBasis}
              required
              minLength={10}
              maxLength={4000}
            />
          </Field>
          <Field appearance="standard">
            Tax-point facts and evidence
            <TextArea appearance="standard"
              name="taxPointEvidence"
              defaultValue={facts.taxPointEvidence}
              required
              minLength={10}
              maxLength={4000}
            />
          </Field>
          <Field appearance="standard">
            Supplier document / tax invoice evidence
            <TextArea appearance="standard"
              name="documentEvidence"
              defaultValue={facts.documentEvidence}
              required
              minLength={10}
              maxLength={4000}
            />
          </Field>
          <Field appearance="standard">
            Correction reason
            <TextArea appearance="standard" name="reason" required minLength={10} maxLength={4000} />
          </Field>
          {error && (
            <InlineNotice tone="danger" className="error" role="alert">
              {error}
            </InlineNotice>
          )}
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={!canSettle || pending}>
            {pending ? "Preparing…" : "Prepare correction set for review"}
          </Button>
        </FormLayout></Stack></Dialog>
      )}
    </Card>
  );
}
export function VatCorrectionReview({ base, correction }: { base: string; correction: TaxRow }) {
  const { send, pending, error, draftScope, canSettle } = useTaxCommand(base),
    [reason, setReason] = useState("");
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>Review correction set</h2>
      <p>
        Approval applies all replacements together and retains the originals. Return and prepare
        again if a source has changed.
      </p>
      <FormLayout data-finance-draft={draftScope} onSubmit={event=>event.preventDefault()}>
      <Field appearance="standard">
        Correction-set review evidence and reason
        <TextArea appearance="standard"
          value={reason}
          onChange={(e) => setReason(e.target.value)}
          minLength={10}
          maxLength={4000}
        />
      </Field>
      <div className="ui-form-actions">
        <Button appearance="standard"
          type="button"
          variant="primary"
          disabled={!canSettle || pending || reason.trim().length < 10}
          onClick={() =>
            void send(`vat-corrections/${correction.id}/approve`, {
              expectedVersion: Number(correction.version),
              reason,
            })
          }
        >
          Approve complete correction set
        </Button>
        <Button appearance="standard"
          type="button"
          variant="secondary"
          disabled={!canSettle || pending || reason.trim().length < 10}
          onClick={() =>
            void send(`vat-corrections/${correction.id}/reject`, {
              expectedVersion: Number(correction.version),
              reason,
            })
          }
        >
          Return complete correction set
        </Button>
      </div>
      </FormLayout>
      {error && (
        <InlineNotice tone="danger" className="error" role="alert">
          {error}
        </InlineNotice>
      )}
    </Card>
  );
}
export function VatPackagePreparation({
  base,
  chartId,
  filingMonth,
  packages,
  ready,
}: {
  base: string;
  chartId: string;
  filingMonth: string;
  packages: TaxRow[];
  ready: boolean;
}) {
  const { send, pending, error, draftScope, canSettle } = useTaxCommand(base);
  return (
    <FormDisclosure appearance="standard" error={error} drawerTitle="Prepare VAT workpaper package"><summary>Prepare VAT workpaper package</summary>
    <FormLayout data-finance-draft={draftScope}
      className="ui-panel ui-form-grid"
      onSubmit={async (e) => {
        e.preventDefault();
        const f = new FormData(e.currentTarget);
        await send("vat-packages", {
          chartId,
          filingMonth,
          filingKind: f.get("filingKind"),
          originalPackageId: f.get("originalPackageId") || undefined,
          reason: f.get("reason"),
        });
      }}
    >
      <h2>Prepare VAT workpaper (ภ.พ.30)</h2>
      <p className="muted">
        {filingMonth} · Use approved entries reconciled to the GL. A different reviewer approves the
        workpaper for manual filing. These amounts exclude carried credits and form adjustments.
      </p>
      <Field appearance="standard">
        Filing kind
        <Select appearance="standard" name="filingKind">
          <option value="original">Original</option>
          <option value="additional">Additional</option>
        </Select>
      </Field>
      <Field appearance="standard">
        Filed original package (for additional filing)
        <Select appearance="standard" name="originalPackageId">
          <option value="">Select for an additional filing</option>
          {packages
            .filter(
              (p) => p.filing_kind === "original" && ["filed", "paid"].includes(String(p.status)),
            )
            .map((p) => (
              <option key={String(p.id)} value={String(p.id)}>
                {String(p.filing_reference)} · {String(p.filing_month)}
              </option>
            ))}
        </Select>
      </Field>
      <Field appearance="standard">
        Preparation evidence and reason
        <TextArea appearance="standard" name="reason" minLength={10} maxLength={4000} required />
      </Field>
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={!canSettle || pending || !ready}>
        {pending ? "Preparing…" : "Prepare package for review"}
      </Button>
    </FormLayout></FormDisclosure>
  );
}
export function VatPackageControls({
  base,
  package: p,
  canApprove,
  userId,
  currentReady = true,
}: {
  base: string;
  package: TaxRow;
  canApprove: boolean;
  userId: string;
  currentReady?: boolean;
}) {
  const { send, pending, error, draftScope, canSettle } = useTaxCommand(base),
    [reason, setReason] = useState("");
  async function exportFile() {
    const result = await send(`vat-packages/${p.id}/export`, {
      expectedVersion: Number(p.version),
      reason,
    });
    if (result) {
      const url = URL.createObjectURL(
          new Blob([result.content], { type: "text/csv;charset=utf-8" }),
        ),
        link = document.createElement("a");
      link.href = url;
      link.download = result.filename;
      link.click();
      URL.revokeObjectURL(url);
    }
  }
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>VAT workpaper next steps</h2>
      <FormLayout data-finance-draft={draftScope} onSubmit={event=>event.preventDefault()}>
      <Field appearance="standard">
        Reason / evidence
        <TextArea appearance="standard"
          value={reason}
          onChange={(e) => setReason(e.target.value)}
          minLength={10}
          maxLength={4000}
        />
      </Field>
      <div className="ui-form-actions">
        {p.status === "prepared" && canApprove && p.prepared_by !== userId && (
          <Button appearance="standard"
            type="button"
            variant="primary"
            disabled={!canSettle || pending || !currentReady || reason.trim().length < 10}
            onClick={() =>
              void send(`vat-packages/${p.id}/approve`, {
                expectedVersion: Number(p.version),
                reason,
              })
            }
          >
            Approve workpaper package
          </Button>
        )}
        {p.status === "approved_for_export" && (
          <Button appearance="standard"
            type="button"
            variant="primary"
            disabled={!canSettle || pending || !currentReady || reason.trim().length < 10}
            onClick={() => void exportFile()}
          >
            Export workpaper CSV
          </Button>
        )}
        {["prepared", "approved_for_export", "exported"].includes(String(p.status)) &&
          (p.prepared_by === userId || canApprove) && (
            <Button appearance="standard"
              type="button"
              variant="secondary"
              disabled={!canSettle || pending || reason.trim().length < 10}
              onClick={() =>
                void send(`vat-packages/${p.id}/supersede`, {
                  expectedVersion: Number(p.version),
                  reason,
                })
              }
            >
              Retain package and prepare replacement
            </Button>
          )}
      </div>
      </FormLayout>
      {p.status === "exported" && (
        <FormLayout data-finance-draft={draftScope}
          className="ui-form-grid"
          onSubmit={async (e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            await send(`vat-packages/${p.id}/file`, {
              expectedVersion: Number(p.version),
              reason,
              filingReference: f.get("filingReference"),
              filedDate: f.get("filedDate"),
              receiptEvidence: f.get("receiptEvidence"),
            });
          }}
        >
          <Field appearance="standard">Filing reason / evidence<TextArea appearance="standard" value={reason} onChange={event=>setReason(event.target.value)} required minLength={10} maxLength={4000}/></Field>
          <p>
            After the responsible person files through the official channel, record the receipt
            against the original export.
          </p>
          <Field appearance="standard">
            Filing receipt reference
            <TextInput appearance="standard" name="filingReference" required maxLength={200} />
          </Field>
          <Field appearance="standard">
            Filed date
            <TextInput appearance="standard" name="filedDate" type="date" required />
          </Field>
          <Field appearance="standard">
            Filing receipt evidence location
            <TextArea appearance="standard" name="receiptEvidence" required minLength={10} maxLength={4000} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={!canSettle || pending || reason.trim().length < 10}>
            Record external filing evidence
          </Button>
        </FormLayout>
      )}
      {error && (
        <InlineNotice tone="danger" className="error" role="alert">
          {error}
        </InlineNotice>
      )}
    </Card>
  );
}
