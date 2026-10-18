"use client";
import { useFinanceActionAllowed } from "./purchase-context";
import { useFinanceDraftScope, captureFinanceForm, acknowledgeFinanceForm } from "./finance-draft-boundary";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import { useRouter } from "@/lib/shared/destination-router";
import { workFetch } from "@/lib/work_projection/work-fetch";
import type { AccountingProfile } from "@/server/modules/finance/public/accounting-profiles";
import type { FinanceChart } from "@/components/finance/finance-chart-controls";
import { Card, FormLayout } from "@/components/ui/foundation";
import { FormDisclosure } from "@/components/ui/form-disclosure";

export function AccountingProfileControls({
  base,
  charts,
  profiles,
  canPrepare,
  canApprove,
  userId,
}: {
  base: string;
  charts: FinanceChart[];
  profiles: AccountingProfile[];
  canPrepare: boolean;
  canApprove: boolean;
  userId: string;
}) {
  const allowedNew = useFinanceActionAllowed("new-work"), allowedSettle = useFinanceActionAllowed("settle-existing");
  const draftScope = useFinanceDraftScope(), [hasAttempt,setHasAttempt] = useState(false);
  const router = useRouter(),
    [pending, setPending] = useState(false),
    [error, setError] = useState("");
  const attempt = useRef<{ fingerprint: string; key: string } | null>(null);
  async function send(path: string, input: unknown) {
    const fingerprint = path + JSON.stringify(input);
    if (!(path === "accounting-profiles" ? allowedNew : allowedSettle) && attempt.current?.fingerprint !== fingerprint) {setError("This Finance action is unavailable. Your draft is retained.");return;}
    if (attempt.current?.fingerprint !== fingerprint)
      attempt.current = { fingerprint, key: crypto.randomUUID() };
    setHasAttempt(true);const completion=captureFinanceForm();
    setPending(true);
    setError("");
    try {
      const response = await workFetch(
        `${base.replace(canonicalUiHref("/holdings/"), "/api/v1/holdings/")}/finance/${path}`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
          body: JSON.stringify(input),
        },
      );
      const result = await response.json();
      if (!response.ok || !result.data || typeof result.data.id !== "string" || !result.data.id) throw new Error(result.error?.message ?? "Could not save.");
      acknowledgeFinanceForm(completion);attempt.current = null;setHasAttempt(false);
      router.refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Could not save.");
    } finally {
      setPending(false);
    }
  }
  return (
    <Card as="section" appearance="standard" className="ui-panel" aria-label="Thai accounting profile">
      <h2>Statutory chart and Company accounting policy</h2>
      <p className="muted">
        Record NPAEs eligibility and inventory/depreciation policy. A different reviewer approves
        the evidence. Then validate opening balances, tax and live workflows.
      </p>
      {error && (
        <InlineNotice tone="danger" className="error" role="alert">
          {error}
        </InlineNotice>
      )}
      {profiles.map((profile) => (
        <Card as="article" appearance="standard" className="ui-panel" key={String(profile.id)}>
          <h3>
            {String(profile.chart_name)} · {String(profile.status)}
          </h3>
          <p>
            {String(profile.standard)} · Effective {String(profile.effective_date)} · Fiscal year
            start month {String(profile.fiscal_start_month)}
          </p>
          <p>Eligibility: {String(profile.eligibility_evidence)}</p>
          <p>Policy: {String(profile.policy_evidence)}</p>
          {profile.status === "draft" && canApprove && profile.prepared_by !== userId && (
            <FormLayout data-finance-draft={draftScope}
              className="ui-form-grid"
              onSubmit={(e) => {
                e.preventDefault();
                const f = new FormData(e.currentTarget);
                void send(`accounting-profiles/${profile.id}/${f.get("decision")}`, {
                  expectedVersion: Number(profile.version),
                  reason: f.get("reason"),
                });
              }}
            >
              <Field appearance="standard">
                Review decision
                <Select appearance="standard" name="decision">
                  <option value="approve">Approve</option>
                  <option value="reject">Return for preparation</option>
                </Select>
              </Field>
              <Field appearance="standard">
                Review evidence and reason
                <TextInput appearance="standard" name="reason" required minLength={10} maxLength={4000} />
              </Field>
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={pending || (!allowedSettle && !hasAttempt)}>
                Record review decision
              </Button>
            </FormLayout>
          )}
          {profile.review_reason && <p>Review decision: {String(profile.review_reason)}</p>}
        </Card>
      ))}
      {canPrepare && !profiles.some((p) => p.status === "approved") && (
        <FormDisclosure appearance="standard" drawerTitle="Prepare Company accounting policy">
          <summary>Prepare Company accounting policy</summary>
        <FormLayout data-finance-draft={draftScope}
          className="ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            void send("accounting-profiles", {
              statutoryChartId: f.get("statutoryChartId"),
              standard: "TFRS for NPAEs",
              fiscalStartMonth: Number(f.get("fiscalStartMonth")),
              effectiveDate: f.get("effectiveDate"),
              eligibilityEvidence: f.get("eligibilityEvidence"),
              policyEvidence: f.get("policyEvidence"),
              taxBasis: "current_tax",
              inventoryPolicy: "moving_average",
              depreciationPolicy: "straight_line",
            });
          }}
        >
          <Field appearance="standard">
            Statutory chart (THB)
            <Select appearance="standard" name="statutoryChartId" required defaultValue="">
              <option value="">Select chart</option>
              {charts
                .filter((c) => c.base_currency === "THB")
                .map((chart) => (
                  <option key={String(chart.id)} value={String(chart.id)}>
                    {String(chart.name)}
                  </option>
                ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Effective date
            <TextInput appearance="standard" name="effectiveDate" type="date" required />
          </Field>
          <Field appearance="standard">
            Fiscal year start month
            <TextInput appearance="standard"
              name="fiscalStartMonth"
              type="number"
              min={1}
              max={12}
              defaultValue={1}
              required
            />
          </Field>
          <Field appearance="standard">
            Evidence of Company NPAEs eligibility
            <TextArea appearance="standard" name="eligibilityEvidence" required minLength={10} maxLength={4000} />
          </Field>
          <Field appearance="standard">
            Evidence of current-tax, moving-average and straight-line policies
            <TextArea appearance="standard" name="policyEvidence" required minLength={10} maxLength={4000} />
          </Field>
          <Button appearance="standard"
            clientManaged type="submit"
            variant="primary"
            disabled={pending || (!allowedNew && !hasAttempt) || !charts.some((c) => c.base_currency === "THB")}
          >
            Prepare accounting profile for review
          </Button>
        </FormLayout>
        </FormDisclosure>
      )}
    </Card>
  );
}
