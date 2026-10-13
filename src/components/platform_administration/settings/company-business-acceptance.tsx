"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, TextArea } from "@/components/ui/foundation";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work_projection/work-fetch";
import { Card, FormLayout } from "@/components/ui/foundation";

type Check = { id: string; label: string; ready: boolean; owner: string; detail: string };
type Acceptance = {
  companyId: string;
  status: "draft" | "submitted" | "rejected" | "accepted" | "revoked";
  version: number;
  checks: Check[];
  blockers: string[];
  canSubmit: boolean;
  canReview: boolean;
  canRevoke: boolean;
  requestedBy: string | null;
  reviewedBy: string | null;
  reason: string | null;
  reviewReason: string | null;
  updatedAt: number | null;
  events: {
    id: string;
    action: string;
    actor_id: string;
    version: number;
    reason: string;
    created_at: number;
  }[];
};

export function CompanyBusinessAcceptance({
  base,
  initial,
}: {
  base: string;
  initial: Acceptance;
}) {
  const router = useRouter();
  const [data, setData] = useState(initial),
    [reason, setReason] = useState(""),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  async function act(action: "submit" | "accept" | "reject" | "revoke") {
    setBusy(true);
    setError("");
    const path = action === "submit" ? "submit" : action === "revoke" ? "revoke" : "review";
    const body =
      action === "submit"
        ? { reason }
        : action === "revoke"
          ? { expectedVersion: data.version, reason }
          : { expectedVersion: data.version, decision: action, reason };
    try {
      const response = await workFetch(`/api/v1${base}/people/organization/acceptance/${path}`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
        body: JSON.stringify(body),
      });
      const result = await response.json();
      if (!response.ok)
        throw new Error(result.error?.message ?? "Unable to update Company acceptance.");
      setReason("");
      setData((current) => ({
        ...current,
        status: result.data.status,
        version: result.data.version,
      }));
      router.refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Unable to update Company acceptance.");
    } finally {
      setBusy(false);
    }
  }
  const status =
    data.status === "accepted"
      ? "Accepted"
      : data.status === "submitted"
        ? "Awaiting independent review"
        : data.status === "rejected"
          ? "Returned for correction"
          : data.status === "revoked"
            ? "Revoked"
            : "Not submitted";
  return (
    <Card as="section" appearance="standard" className="ui-panel" id="company-setup-acceptance" aria-label="Company setup acceptance">
      <header className="ui-section-header">
        <div>
          <p className="eyebrow">Company control</p>
          <h2>Company setup acceptance</h2>
        </div>
        <span
          className={`badge ${data.status === "accepted" ? "success" : data.status === "submitted" ? "warning" : ""}`}
        >
          {status}
        </span>
      </header>
      <p>
        Accepting setup confirms the Company structure and staff placement. It requires an
        independent Company administrator and does not approve transactions or certify a production
        release.
      </p>
      <ul>
        {data.checks.map((check) => (
          <li key={check.id}>
            <strong>
              {check.ready ? "✓" : "○"} {check.label}
            </strong>{" "}
            · {check.owner}
            <p className="muted">{check.detail}</p>
          </li>
        ))}
      </ul>
      {data.reviewReason && (
        <InlineNotice tone="info" role="status">
          <strong>Review note:</strong> {data.reviewReason}
        </InlineNotice>
      )}
      {data.reason && <p className="muted">Submitted reason: {data.reason}</p>}
      {(data.canSubmit || data.canReview || data.canRevoke) && (
        <FormLayout
          className="ui-form-grid"
          onSubmit={(event) => {
            event.preventDefault();
            void act(data.canReview ? "accept" : data.canRevoke ? "revoke" : "submit");
          }}
        >
          <Field appearance="standard">
            {data.canReview
              ? "Review reason"
              : data.canRevoke
                ? "Revocation reason"
                : "Acceptance reason"}
            <TextArea appearance="standard"
              required
              minLength={3}
              maxLength={2000}
              value={reason}
              onChange={(event) => setReason(event.target.value)}
              placeholder="Record the evidence reviewed and the business decision."
            />
          </Field>
          <div className="button-row">
            {data.canSubmit && (
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || reason.trim().length < 3}>
                Submit for independent review
              </Button>
            )}
            {data.canReview && (
              <>
                <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || reason.trim().length < 3}>
                  Accept Company setup
                </Button>
                <Button appearance="standard"
                  variant="secondary"
                  type="button"
                  disabled={busy || reason.trim().length < 3}
                  onClick={() => void act("reject")}
                >
                  Return for correction
                </Button>
              </>
            )}
            {data.canRevoke && (
              <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={busy || reason.trim().length < 3}>
                Revoke acceptance
              </Button>
            )}
          </div>
        </FormLayout>
      )}
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      {!!data.events.length && (
        <details>
          <summary>Immutable acceptance history</summary>
          <ol>
            {data.events.map((event) => (
              <li key={event.id}>
                v{event.version} · {event.action} · {event.reason}
              </li>
            ))}
          </ol>
        </details>
      )}
    </Card>
  );
}
