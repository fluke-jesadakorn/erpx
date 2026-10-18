"use client";
import { useFinanceDraftScope } from "./finance-draft-boundary";
import { useState } from "react";
import type { BusinessContractInput } from "@/server/modules/finance/public/rules/finance-contract-policy";
import { financeCsvRows } from "@/lib/finance/finance-csv";
import { FormDisclosure } from "@/components/ui/form-disclosure";
import {
  Button,
  Field,
  FormLayout,
  InlineNotice,
  TextArea,
  TextInput,
  Select,
} from "@/components/ui/foundation";
type Row = Record<string, string | number | null>;
export function FinanceBillingRevision({
  bill,
  accounts,
  busy,
  onSave,
  posted=false,
  periods=[],
  commandError,
}: {
  bill: Row;
  accounts: Row[];
  busy: boolean;
  onSave: (input: unknown) => Promise<void>;
  posted?:boolean;
  periods?:Row[];
  commandError?:string;
}) {
  const draftScope=useFinanceDraftScope();
  const [error, setError] = useState(""),
    content = JSON.parse(
      String(bill.effective_contract_content_json),
    ) as BusinessContractInput,
    prior = JSON.parse(String(bill.input_json)) as {
      month?: string;
      invoiceDate: string;
      dueDate: string;
      usageUnits: number;
      usageEvidence: string;
    };
  return (
    <FormDisclosure error={commandError || error}
      appearance="standard"
      drawerTitle={posted?"Prepare reviewed credit and replacement":"Correct an unposted bill or usage reading"}
    >
      <summary>{posted?"Prepare credit and replacement":"Prepare corrected billing edition"}</summary>
      <FormLayout data-finance-draft={draftScope}
        className="ui-form-grid"
        onSubmit={async (e) => {
          e.preventDefault();
          setError("");
          const form = new FormData(e.currentTarget),
            text = (name: string) => String(form.get(name) ?? "");
          try {
            const flat = text("flat").trim(),
              tiers = text("tiers").trim(),
              allocation = text("allocation").trim();
            if (flat && tiers)
              throw new Error(
                "Choose one replacement price: flat or graduated tiers.",
              );
            const pricing = tiers
              ? {
                  kind: "tier",
                  tiers: financeCsvRows(tiers).map((t) => ({
                    upTo: t.upTo === "unlimited" ? null : Number(t.upTo),
                    unitPrice: t.unitPrice,
                  })),
                }
              : flat
                ? { kind: "flat", amount: flat }
                : undefined;
            const obligations = allocation
              ? financeCsvRows(allocation).map((r) => ({
                  key: r.key,
                  name: r.name,
                  standalonePrice: r.standalonePrice,
                  revenueAccountId: String(
                    accounts.find(
                      (a) =>
                        a.chart_id === content.chartId &&
                        a.code === r.revenueAccountCode,
                    )?.id ?? "",
                  ),
                  mode: r.mode,
                  milestones: r.milestones
                    ? r.milestones.split("|").map((m) => {
                        const [date, weight] = m.split(":");
                        return { date, weight: Number(weight) };
                      })
                    : [],
                }))
              : undefined;
            await onSave({
              ...(posted?{number:text("creditNumber"),periodId:text("periodId"),entryDate:text("invoiceDate")} : {}),
              expectedVersion: Number(bill.version),
              invoiceExpectedVersion: bill.invoice_id
                ? Number(bill.current_invoice_version)
                : undefined,
              billing: {
                month: prior.month,
                invoiceDate: text("invoiceDate"),
                dueDate: text("dueDate"),
                usageUnits:
                  content.cycle === "usage" ? Number(text("usageUnits")) : 0,
                usageEvidence:
                  content.cycle === "usage" ? text("usageEvidence") : "",
              },
              pricing,
              obligations,
              reason: text("reason"),
            });
          } catch (e) {
            setError(
              e instanceof Error ? e.message : "Review the correction input.",
            );
          }
        }}
      >
        <p>
          Retain {String(bill.service_from)} to {String(bill.service_to)}.
          Review a new invoice or zero-charge reading; the previous edition
          stays in history.
        </p>
        {posted&&<><p>The review credits this entire invoice and reverses its posted recognition in the selected open period. The replacement is posted in the same transaction. Previously collected cash becomes a refund obligation for separate beneficiary review and bank confirmation.</p><Field appearance="standard">Credit note number<TextInput appearance="standard" name="creditNumber" required maxLength={100}/></Field><Field appearance="standard">Open correction period<Select appearance="standard" name="periodId" required>{periods.map(p=><option key={String(p.id)} value={String(p.id)}>{String(p.name)} · {String(p.start_date)} to {String(p.end_date)}</option>)}</Select></Field></>}
        {error && (
          <InlineNotice tone="danger" role="alert">
            {error}
          </InlineNotice>
        )}
        <Field appearance="standard">
          Corrected invoice date
          <TextInput
            appearance="standard"
            type="date"
            name="invoiceDate"
            required
            min={prior.invoiceDate}
            defaultValue={prior.invoiceDate}
          />
        </Field>
        <Field appearance="standard">
          Due date
          <TextInput
            appearance="standard"
            type="date"
            name="dueDate"
            required
            defaultValue={prior.dueDate}
          />
        </Field>
        {content.cycle === "usage" && (
          <>
            <Field appearance="standard">
              Corrected usage units
              <TextInput
                appearance="standard"
                type="number"
                min={0}
                step={1}
                name="usageUnits"
                required
                defaultValue={prior.usageUnits}
              />
            </Field>
            <Field appearance="standard">
              Verified meter or usage evidence
              <TextArea
                appearance="standard"
                name="usageEvidence"
                required
                defaultValue={prior.usageEvidence}
              />
            </Field>
          </>
        )}
        <details>
          <summary>Change pricing or revenue allocation</summary>
          <Field appearance="standard">
            Replacement flat price (leave blank to keep the current price)
            <TextInput
              appearance="standard"
              name="flat"
              inputMode="decimal"
              placeholder={
                content.pricing.kind === "flat"
                  ? content.pricing.amount
                  : "Keep graduated tiers"
              }
            />
          </Field>
          {content.cycle === "usage" && (
            <Field appearance="standard">
              Replacement graduated tiers CSV
              <TextArea
                appearance="standard"
                name="tiers"
                placeholder={"upTo,unitPrice\n10,2.00\nunlimited,1.00"}
              />
            </Field>
          )}
          <Field appearance="standard">
            Replacement standalone price allocation CSV
            <TextArea
              appearance="standard"
              name="allocation"
              placeholder={
                "key,name,standalonePrice,revenueAccountCode,mode,milestones\nservice,Monthly support,100.00,4100,monthly,"
              }
            />
          </Field>
        </details>
        <Field appearance="standard">
          Correction reason
          <TextArea
            appearance="standard"
            name="reason"
            required
            minLength={3}
            maxLength={2000}
          />
        </Field>
        <Button
          appearance="standard"
          type="submit"
          clientManaged
          disabled={busy}
        >
          Save corrected edition for review
        </Button>
      </FormLayout>
    </FormDisclosure>
  );
}
