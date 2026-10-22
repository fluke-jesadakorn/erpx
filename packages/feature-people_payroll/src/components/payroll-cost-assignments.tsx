"use client";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Button, Field, TextInput } from "@erpx/ui/components/foundation";

import { useEffect, useRef, useState } from "react";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import { payrollErrorMessage, type PayrollApiError } from "@erpx/feature-people_payroll/lib/payroll-errors";
import { Card, FormLayout } from "@erpx/ui/components/foundation";
type Assignment = {
  id: string;
  effective_from: string;
  effective_to: string | null;
  branch_id: string | null;
  department_id: string | null;
  evidence_status: string;
  source: string;
  version: number;
  evidence_ref: string | null;
  verified_by: string | null;
  verified_at: string | null;
};
export function PayrollCostAssignmentReview({
  base,
  engagementId,
}: {
  base: string;
  engagementId: string;
  userId: string;
}) {
  const [assignments, setAssignments] = useState<Assignment[] | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const attempt = useRef<{
    body: string;
    key: string;
  } | null>(null);
  const router = useRouter();
  useEffect(() => {
    let active = true;
    void workFetch(
      `/api/v1${base}/people/payroll/cost-assignments/${encodeURIComponent(engagementId)}`,
      { cache: "no-store" },
    )
      .then(async (response) => {
        const result = (await response.json()) as {
          data?: Assignment[];
          error?: PayrollApiError;
        };
        if (!response.ok || !result.data)
          throw result.error ?? { message: "Could not load cost assignments." };
        if (active) setAssignments(result.data);
      })
      .catch((cause) => {
        if (active) setError(payrollErrorMessage(cause, "Could not load cost assignments."));
      });
    return () => {
      active = false;
    };
  }, [base, engagementId]);
  async function verify(assignment: Assignment, form: HTMLFormElement) {
    const evidenceRef = String(new FormData(form).get("evidenceRef") ?? "").trim();
    const body = JSON.stringify({ expectedVersion: assignment.version, evidenceRef });
    const path = `payroll/cost-assignments/${encodeURIComponent(assignment.id)}/verify`;
    if (attempt.current?.body !== path + body)
      attempt.current = { body: path + body, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(`/api/v1${base}/people/${path}`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
        body,
      });
      const result = (await response.json()) as {
        error?: PayrollApiError;
      };
      if (!response.ok) throw result.error ?? { message: "Could not verify this assignment." };
      attempt.current = null;
      const refresh = await workFetch(
        `/api/v1${base}/people/payroll/cost-assignments/${encodeURIComponent(engagementId)}`,
        { cache: "no-store" },
      );
      if (refresh.ok) {
        const updated = (await refresh.json()) as {
          data: Assignment[];
        };
        setAssignments(updated.data);
      }
      router.refresh();
    } catch (cause) {
      setError(payrollErrorMessage(cause, "Could not verify this assignment."));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Card as="section" appearance="standard" className="ui-panel" id="payroll-cost-assignments">
      <h2>{"Payroll cost assignments"}</h2>
      <p className="muted">
        {
          "Review the effective branch and department for each period. Verify imported legacy assignments with a real evidence reference."
        }
      </p>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {!assignments && !error && <p className="muted">{"Loading assignment history…"}</p>}
      {assignments?.length === 0 && <p className="muted">{"No cost assignments recorded."}</p>}
      {assignments?.map((assignment) => (
        <article className="ui-record-item" key={assignment.id}>
          <div className="row-main">
            <strong>
              {assignment.effective_from} – {assignment.effective_to ?? "present"}
            </strong>
            <span>
              {"Branch"}: {assignment.branch_id ?? "—"} · {"Department"}:{" "}
              {assignment.department_id ?? "—"}
            </span>
            <span>
              {"Evidence status"}: {assignment.evidence_status} · {"Source"}: {assignment.source}
            </span>
            {assignment.evidence_ref && (
              <span>
                {"Evidence reference"}: {assignment.evidence_ref}
              </span>
            )}
          </div>
          {assignment.evidence_status === "legacy_unverified" && (
            <FormLayout
              className="ui-form-grid"
              onSubmit={(event) => {
                event.preventDefault();
                void verify(assignment, event.currentTarget);
              }}
            >
              <Field appearance="standard">
                {"Evidence reference supporting unit and dates"}
                <TextInput appearance="standard" name="evidenceRef" minLength={8} maxLength={500} required />
              </Field>
              <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={busy}>
                {"Verify assignment"}
              </Button>
            </FormLayout>
          )}
        </article>
      ))}
    </Card>
  );
}
