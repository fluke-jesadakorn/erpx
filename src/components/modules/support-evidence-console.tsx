"use client";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/attachments/attachment-panel";
import {
  canMutateAttachments,
  useAttachmentFeatureMode,
} from "@/components/attachments/attachment-feature-context";
import { workFetch } from "@/lib/work-fetch";
import type { supportEvidence } from "@/server/support-evidence";

const emptyChange: AttachmentPanelChange = {
  setId: null,
  links: [],
  readyCount: 0,
  requiredCount: 0,
  bindable: true,
};

export function SupportEvidenceConsole({
  base,
  ticketId,
  version,
  data,
}: {
  base: string;
  ticketId: string;
  version: number;
  data: Awaited<ReturnType<typeof supportEvidence>>;
}) {
  const attachmentMode = useAttachmentFeatureMode(),
    router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [purpose, setPurpose] = useState<
      "internal_evidence" | "customer_shareable"
    >("internal_evidence"),
    [change, setChange] = useState<AttachmentPanelChange>(emptyChange),
    [reason, setReason] = useState(""),
    command = useRef<{ fingerprint: string; key: string } | null>(null),
    revokeKey = useRef<{ input: string; key: string } | null>(null),
    canMutateEvidence = canMutateAttachments(attachmentMode, data.canChange);
  const purposeLabel =
    purpose === "customer_shareable"
      ? "Customer-shareable document"
      : "Internal case evidence";
  async function bind() {
    if (
      !canMutateEvidence ||
      busy ||
      !change.setId ||
      !change.readyCount ||
      !change.bindable
    )
      return;
    const input = {
        id: ticketId,
        action: "bind-evidence",
        expectedVersion: version,
        attachmentSetIds: [change.setId],
        reason,
      },
      fingerprint = JSON.stringify(input);
    if (command.current?.fingerprint !== fingerprint)
      command.current = { fingerprint, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(`/api/v1${base}/support`, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": command.current.key,
          },
          body: fingerprint,
        }),
        result = await response.json();
      if (!response.ok) {
        if (response.status < 500) command.current = null;
        throw new Error(
          result.error?.message ?? "Unable to lock case evidence.",
        );
      }
      command.current = null;
      setReason("");
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry the evidence action.");
    } finally {
      setBusy(false);
    }
  }
  async function revoke(attachmentId: string, withdrawalReason: string) {
    if (!canMutateEvidence || busy) return;
    setBusy(true);
    setError("");
    const input = {
        id: ticketId,
        action: "revoke-evidence",
        expectedVersion: version,
        attachmentId,
        reason: withdrawalReason,
      },
      fingerprint = JSON.stringify(input);
    if (revokeKey.current?.input !== fingerprint)
      revokeKey.current = { input: fingerprint, key: crypto.randomUUID() };
    try {
      const response = await workFetch(`/api/v1${base}/support`, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": revokeKey.current.key,
          },
          body: fingerprint,
        }),
        result = await response.json();
      if (!response.ok)
        throw new Error(
          result.error?.message ?? "Unable to withdraw evidence.",
        );
      revokeKey.current = null;
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry the evidence action.");
    } finally {
      setBusy(false);
    }
  }
  if (attachmentMode === "disabled" && data.items.length === 0) return null;
  return (
    <section className="card content-stack">
      <h2>Support files</h2>
      <p>
        Internal evidence stays with authorized Support staff. A
        customer-shareable file is a separate reviewed purpose and the portal
        exposes only its sanitized preview, never the private original or
        internal metadata.
      </p>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {attachmentMode !== "disabled" && (
        <label>
          File audience to review
          <select
            value={purpose}
            onChange={(event) => {
              setPurpose(event.target.value as typeof purpose);
              setChange(emptyChange);
              setReason("");
            }}
          >
            <option value="internal_evidence">Internal Support only</option>
            <option value="customer_shareable">Customer-shareable files</option>
          </select>
        </label>
      )}
      {attachmentMode !== "disabled" && (
        <AttachmentPanel
          key={`${ticketId}:${version}:${purpose}`}
          base={base}
          targetType="support_ticket"
          targetId={ticketId}
          expectedTargetVersion={version}
          componentKind="action"
          componentRef={`support-evidence:${purpose}`}
          purposeKey={purpose}
          title={purposeLabel}
          description={
            purpose === "customer_shareable"
              ? "Add only an approved customer-safe image or PDF. The customer receives a sanitized preview after this action is bound."
              : "Add private photos or documents used to investigate this case."
          }
          visibility={
            purpose === "customer_shareable"
              ? "Support managers can bind this purpose; the matching authenticated customer can view only the sanitized preview."
              : "Only the current assigned Support colleague and eligible Support managers can view this purpose."
          }
          readOnly={
            !canMutateEvidence ||
            (purpose === "customer_shareable" && !data.canShare)
          }
          frozen={!canMutateEvidence}
          showHistory
          allowOriginalDownload={purpose !== "customer_shareable"}
          onChange={setChange}
        />
      )}
      {canMutateEvidence && (
        <div className="business-form">
          <label>
            Why this evidence belongs to the case
            <textarea
              value={reason}
              onChange={(event) => setReason(event.target.value)}
              minLength={3}
              maxLength={2000}
              required
            />
          </label>
          <button
            type="button"
            className="button"
            disabled={
              busy ||
              reason.trim().length < 3 ||
              !change.setId ||
              change.readyCount < 1 ||
              !change.bindable
            }
            onClick={() => void bind()}
          >
            {busy ? "Saving…" : `Bind ${purposeLabel.toLowerCase()}`}
          </button>
        </div>
      )}
      {data.items.length > 0 && (
        <details>
          <summary>Legacy case files</summary>
          <p>
            These files use the previous private evidence adapter and remain
            available during migration.
          </p>
          {data.items.map((f) => (
            <div className="content-stack" key={String(f.id)}>
              {f.revoked_at ? (
                <p>{String(f.original_name)} · Withdrawn</p>
              ) : (
                <a href={`/api/v1${base}/attachments/${f.id}/content`}>
                  {String(f.original_name)} · {String(f.bytes)} bytes
                </a>
              )}
              {canMutateEvidence && !f.revoked_at && !Number(f.legal_hold) && (
                <form
                  className="business-form"
                  onSubmit={(event) => {
                    event.preventDefault();
                    void revoke(
                      String(f.id),
                      String(new FormData(event.currentTarget).get("reason")),
                    );
                  }}
                >
                  <label>
                    Reason for withdrawing {String(f.original_name)}
                    <input
                      name="reason"
                      minLength={3}
                      maxLength={2000}
                      required
                    />
                  </label>
                  <button className="button secondary" disabled={busy}>
                    Withdraw legacy file
                  </button>
                </form>
              )}
            </div>
          ))}
        </details>
      )}
    </section>
  );
}
