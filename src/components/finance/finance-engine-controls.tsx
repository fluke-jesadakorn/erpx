"use client";
import { useFinanceActionAllowed } from "./purchase-context";
import { useFinanceDraftScope, captureFinanceForm, acknowledgeFinanceForm } from "./finance-draft-boundary";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextInput } from "@/components/ui/foundation";

import { useState } from "react";
import { useRouter } from "@/lib/shared/destination-router";
import { financeCommandFetch as workFetch } from "@/lib/finance/finance-command-fetch";
import { Card, FormGrid, FormLayout } from "@/components/ui/foundation";

type Row = Record<string, string | number | null>;
const fieldSets = {
  allocation: [
    ["total", "Total amount", "100.00"],
    ["weightA", "Weight A", "1"],
    ["weightB", "Weight B", "1"],
  ],
  cash_flow: [
    ["openingCash", "Opening cash", "0"],
    ["closingCash", "Closing cash", "0"],
    ["profit", "Net profit", "0"],
    ["nonCashAdjustments", "Non-cash adjustments", "0"],
    ["workingCapitalChange", "Working-capital change", "0"],
    ["investing", "Investing cash flow", "0"],
    ["financing", "Financing cash flow", "0"],
    ["fxEffect", "Foreign-exchange effect", "0"],
  ],
  consolidation: [
    ["consideration", "Transferred consideration", "0"],
    ["fairValueNetAssets", "Fair-value net assets", "0"],
    ["nciFairValue", "NCI fair value at acquisition", "0"],
    ["parentBps", "Parent ownership (bps)", "8000"],
    ["openingNci", "Opening NCI", "0"],
    ["subsidiaryProfit", "Subsidiary profit", "0"],
    ["subsidiaryOci", "Subsidiary OCI", "0"],
    ["subsidiaryDividends", "Subsidiary dividends", "0"],
    ["associateOpening", "Opening associate investment", "0"],
    ["associateBps", "Associate ownership (bps)", "3000"],
    ["associateProfit", "Associate profit", "0"],
  ],
  thai_statutory: [
    ["outputVat", "Output VAT", "0"],
    ["inputVat", "Input VAT", "0"],
    ["disallowedInputVat", "Disallowed input VAT", "0"],
    ["priorCredit", "Prior credit", "0"],
    ["pnd3Withholding", "P.N.D.3 withholding", "0"],
    ["pnd53Withholding", "P.N.D.53 withholding", "0"],
    ["pp36Vat", "P.P.36 VAT", "0"],
    ["pnd54Withholding", "P.N.D.54 withholding", "0"],
  ],
  manufacturing_cost: [
    ["openingQuantityMilli", "Opening material quantity (milli-units)", "1000"],
    ["openingValue", "Opening material value", "0"],
    ["receiptQuantityMilli", "Receipt quantity (milli-units)", "0"],
    ["receiptValue", "Receipt value", "0"],
    ["issueQuantityMilli", "Issue quantity (milli-units)", "0"],
    ["labor", "Labor", "0"],
    ["overhead", "Allocated overhead", "0"],
    ["abnormalScrapCost", "Abnormal scrap", "0"],
    ["outputQuantityMilli", "Output quantity (milli-units)", "1000"],
  ],
  reconciliation: [
    ["bankAmount", "Bank amount", "0"],
    ["allocationA", "Allocation A", "0"],
    ["allocationB", "Allocation B", "0"],
  ],
} as const;
type Kind = keyof typeof fieldSets;
const labels: Record<Kind, string> = {
  allocation: "Allocation",
  cash_flow: "Cash flow",
  consolidation: "Group reporting / NCI / Goodwill",
  thai_statutory: "Thai statutory",
  manufacturing_cost: "Manufacturing cost",
  reconciliation: "Reconciliation",
};
function payload(kind: Kind, form: FormData) {
  const value = (key: string) => String(form.get(key) ?? "0");
  if (kind === "allocation")
    return {
      total: value("total"),
      weights: [
        { key: "A", weight: value("weightA") },
        { key: "B", weight: value("weightB") },
      ],
    };
  if (kind === "reconciliation")
    return {
      bankAmount: value("bankAmount"),
      allocations: [
        { reference: "A", amount: value("allocationA") },
        { reference: "B", amount: value("allocationB") },
      ],
    };
  if (kind === "manufacturing_cost")
    return {
      openingQuantityMilli: value("openingQuantityMilli"),
      openingValue: value("openingValue"),
      receipts:
        value("receiptQuantityMilli") === "0"
          ? []
          : [{ quantityMilli: value("receiptQuantityMilli"), value: value("receiptValue") }],
      issues:
        value("issueQuantityMilli") === "0" ? [] : [{ quantityMilli: value("issueQuantityMilli") }],
      labor: value("labor"),
      overhead: value("overhead"),
      abnormalScrapCost: value("abnormalScrapCost"),
      outputQuantityMilli: value("outputQuantityMilli"),
    };
  const result = Object.fromEntries(
    fieldSets[kind].map(([key]) => [key, key.endsWith("Bps") ? Number(value(key)) : value(key)]),
  );
  if (kind === "consolidation")
    return {
      ...result,
      previousInterest: "0",
      nciOwnershipChange: "0",
      associateOci: "0",
      associateDividends: "0",
      associateImpairment: "0",
    };
  return result;
}
export function CalculationRunForm({ base, periods }: { base: string; periods: Row[] }) {
  const draftScope=useFinanceDraftScope();
  const [hasAttempt,setHasAttempt]=useState(false);
  const allowed=useFinanceActionAllowed("new-work","finance.advanced");
  const router = useRouter(),
    [kind, setKind] = useState<Kind>("cash_flow"),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>Prepare a calculation run</h2>
      <p className="muted">
        The deterministic engine stores a source hash before another person reviews the result.
      </p>
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      <FormLayout data-finance-draft={draftScope}
        onSubmit={async (event) => {
          event.preventDefault();
          const completion=captureFinanceForm();
          setBusy(true);
          setError("");
          const form = new FormData(event.currentTarget);
          try {
            const url=`${base.replace(canonicalUiHref("/holdings/"), "/api/v1/holdings/")}/finance/calculation-runs`,
              requestBody=JSON.stringify({kind,currency:String(form.get("currency")),currencyScale:Number(form.get("currencyScale")),periodId:String(form.get("periodId"))||null,policyVersion:1,input:payload(kind,form)});
            if (!allowed && !workFetch.hasAttempt(url,requestBody)) throw new Error("This calculation action is unavailable for the current package. Your draft is retained.");
            setHasAttempt(true);
            const response = await workFetch(
                url,
                {
                  method: "POST",
                  headers: {
                    "Content-Type": "application/json",
                    "Idempotency-Key": crypto.randomUUID(),
                  },
                  body: requestBody,
                },
              ),
              body = (await response.json()) as { data?: unknown; error?: { message: string } };
            if (!response.ok)
              throw new Error(body.error?.message ?? "Unable to create calculation run");
            if (body.data === undefined) throw new Error("Calculation acknowledgement is missing. Retry the same action.");
            acknowledgeFinanceForm(completion);
            setHasAttempt(false);
            router.refresh();
          } catch (reason) {
            setError(reason instanceof Error ? reason.message : "Unable to create calculation run");
          } finally {
            setBusy(false);
          }
        }}
      >
        <FormGrid as="div" className="ui-form-grid">
          <Field appearance="standard">
            Type
            <Select appearance="standard" value={kind} onChange={(event) => setKind(event.target.value as Kind)}>
              {Object.entries(labels).map(([value, label]) => (
                <option value={value} key={value}>
                  {label}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Period
            <Select appearance="standard" name="periodId">
              <option value="">No period</option>
              {periods.map((period) => (
                <option value={String(period.id)} key={String(period.id)}>
                  {String(period.name)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Currency
            <TextInput appearance="standard" name="currency" defaultValue="THB" maxLength={3} required />
          </Field>
          <Field appearance="standard">
            Decimals
            <Select appearance="standard" name="currencyScale" defaultValue="2">
              <option value="0">0</option>
              <option value="2">2</option>
              <option value="3">3</option>
            </Select>
          </Field>
        </FormGrid>
        <FormGrid as="div" className="ui-form-grid">
          {fieldSets[kind].map(([key, label, initial]) => (
            <Field appearance="standard" key={key}>
              {label}
              <TextInput appearance="standard" name={key} inputMode="decimal" defaultValue={initial} required />
            </Field>
          ))}
        </FormGrid>
        <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || (!allowed && !hasAttempt)}>
          {busy ? "Calculating…" : "Calculate and save draft"}
        </Button>
      </FormLayout>
    </Card>
  );
}

export function CalculationDecision({ base, run }: { base: string; run: Row }) {
  const draftScope=useFinanceDraftScope();
  const [hasAttempt,setHasAttempt]=useState(false);
  const allowed=useFinanceActionAllowed("settle-existing","finance.advanced");
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const action =
    run.status === "prepared" ? "review" : run.status === "reviewed" ? "approve" : null;
  if (!action) return null;
  return (
    <FormLayout data-finance-draft={draftScope}
      onSubmit={async (event) => {
        event.preventDefault();
        const completion=captureFinanceForm();
        setBusy(true);
        setError("");
        try {
          const form = new FormData(event.currentTarget),
            url=`${base.replace(canonicalUiHref("/holdings/"), "/api/v1/holdings/")}/finance/calculation-runs/${run.id}/${action}`,
            requestBody=JSON.stringify({expectedVersion:Number(run.version),reason:String(form.get("reason"))});
          if (!allowed && !workFetch.hasAttempt(url,requestBody)) throw new Error("This calculation action is unavailable for the current package. Your draft is retained.");
          setHasAttempt(true);
          const response = await workFetch(
              url,
              {
                method: "POST",
                headers: {
                  "Content-Type": "application/json",
                  "Idempotency-Key": crypto.randomUUID(),
                },
                body: requestBody,
              },
            ),
            body = (await response.json()) as { data?: unknown; error?: { message: string } };
          if (!response.ok) throw new Error(body.error?.message ?? "Unable to save the decision");
          if (body.data === undefined) throw new Error("Calculation acknowledgement is missing. Retry the same action.");
          acknowledgeFinanceForm(completion);
          setHasAttempt(false);
          router.refresh();
        } catch (reason) {
          setError(reason instanceof Error ? reason.message : "Unable to save the decision");
        } finally {
          setBusy(false);
        }
      }}
    >
      <Field appearance="standard">
        Review evidence
        <TextInput appearance="standard" name="reason" required minLength={3} />
      </Field>
      <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={busy || (!allowed && !hasAttempt)}>
        {action === "review" ? "Confirm review" : "Approve calculation"}
      </Button>
      {error && (
        <span className="error" role="alert">
          {error}
        </span>
      )}
    </FormLayout>
  );
}
