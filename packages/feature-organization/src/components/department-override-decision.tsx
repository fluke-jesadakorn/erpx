"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Button, Field, TextArea } from "@erpx/ui/components/foundation";

import { useRef, useState } from "react";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import { Card } from "@erpx/ui/components/foundation";

type Detail = {
  kind: "expense" | "leave" | "overtime" | "request";
  id: string;
  version: number;
  step: number;
  originalApprover: string;
};
export function DepartmentOverrideDecision({
  base,
  detail,
  returnTo = "",
}: {
  base: string;
  detail: Detail;
  returnTo?: string;
}) {
  const router = useRouter(),
    [reason, setReason] = useState(""),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const attempt = useRef<{ body: string; key: string } | null>(null);
  async function decide(outcome: "approve" | "reject") {
    const body = JSON.stringify({
      kind: detail.kind,
      id: detail.id,
      expectedVersion: detail.version,
      outcome,
      reason: reason.trim(),
    });
    if (attempt.current?.body !== body) attempt.current = { body, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(`/api/v1${base}/authority/decisions`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
        body,
      });
      const result = await response.json();
      if (!response.ok) throw new Error(result.error?.message ?? "Decision could not be recorded.");
      attempt.current = null;
      window.dispatchEvent(new Event("erpx:work-changed"));
      router.push(canonicalUiHref(`${base}/work${returnTo ? `?${returnTo}` : ""}`));
      router.refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Decision could not be recorded.");
    } finally {
      setBusy(false);
    }
  }
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>Record decision</h2>
      <Field appearance="standard">
        Override reason{" "}
        <TextArea appearance="standard"
          value={reason}
          onChange={(event) => setReason(event.target.value)}
          required
          minLength={3}
          maxLength={2048}
        />
      </Field>
      <div className="ui-form-actions">
        <Button appearance="standard"
          type="submit"
          variant="primary"
          disabled={busy || reason.trim().length < 3}
          onClick={() => void decide("approve")}
        >
          Approve current step
        </Button>
        <Button appearance="standard"
          type="submit"
          variant="secondary"
          disabled={busy || reason.trim().length < 3}
          onClick={() => void decide("reject")}
        >
          Reject current step
        </Button>
      </div>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
    </Card>
  );
}
