"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, TextArea } from "@/components/ui/foundation";

import { useState } from "react";
import { useRouter } from "next/navigation";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/attachments/attachment-panel";

type SharedFile = { linkId: string; label: string; previewAvailable: true };
type Props = {
  holdingSlug: string;
  bindingId: string;
  bindingVersion: number;
  actions?: Record<string, unknown>[];
  sharedFiles?: SharedFile[];
};
export function CustomerPortalActions({
  holdingSlug,
  bindingId,
  bindingVersion,
  actions = [],
  sharedFiles = [],
}: Props) {
  const router = useRouter(),
    [note, setNote] = useState(""),
    [busy, setBusy] = useState<string | null>(null),
    [error, setError] = useState(""),
    [saved, setSaved] = useState(""),
    [evidence, setEvidence] = useState<AttachmentPanelChange | null>(null);
  const acknowledged = actions.some((action) => action.action === "acknowledge_delivery"),
    supportRequested = actions.some((action) => action.action === "request_support");
  const activeEvidence =
      evidence?.links.filter((link) => !["withdrawn", "expired"].includes(link.linkState)) ?? [],
    evidencePending = activeEvidence.length > 0 && !evidence?.bindable;
  async function submit(action: "acknowledge_delivery" | "request_support") {
    if (busy) return;
    setBusy(action);
    setError("");
    setSaved("");
    try {
      const response = await fetch(`/api/customer-portal/${encodeURIComponent(holdingSlug)}`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
        body: JSON.stringify({
          bindingId,
          expectedBindingVersion: bindingVersion,
          action,
          note: action === "request_support" ? note : "",
          attachmentSetIds:
            action === "request_support" &&
            evidence?.setId &&
            evidence.readyCount > 0 &&
            evidence.bindable
              ? [evidence.setId]
              : [],
        }),
      });
      const body = await response.json().catch(() => ({}));
      if (!response.ok)
        throw new Error(body.error?.message ?? "The customer action could not be recorded.");
      setSaved(
        action === "request_support"
          ? "Support request sent to the company."
          : "Delivery acknowledgement recorded.",
      );
      setNote("");
      router.refresh();
    } catch (error) {
      setError(
        error instanceof Error ? error.message : "The customer action could not be recorded.",
      );
    } finally {
      setBusy(null);
    }
  }
  return (
    <div className="customer-portal-actions" aria-label="Customer actions">
      <h3>Next action</h3>
      <Field appearance="standard">
        Message to the company (optional for support)
        <TextArea appearance="standard"
          value={note}
          onChange={(event) => setNote(event.target.value)}
          maxLength={500}
          rows={3}
          placeholder="Describe what you need help with"
        />
      </Field>
      <AttachmentPanel appearance="standard"
        base={`/api/customer-portal/${encodeURIComponent(holdingSlug)}/attachments`}
        targetType="customer_access_binding"
        targetId={bindingId}
        expectedTargetVersion={bindingVersion}
        componentKind="action"
        componentRef="request_support"
        purposeKey="customer_support_evidence"
        title="Support request photos and documents"
        description="Optionally attach photos or documents that explain this support request. Every file is scanned before it can be submitted."
        visibility="Only your verified account and authorized company staff can access this evidence."
        readOnly={supportRequested}
        showHistory
        allowOriginalDownload={false}
        onChange={setEvidence}
      />
      {sharedFiles.length > 0 && (
        <section aria-label="Files shared by the company">
          <h4>Files shared by the company</h4>
          {sharedFiles.map((file) => (
            <a
              key={file.linkId}
              className="ui-record-item"
              href={`/api/customer-portal/${encodeURIComponent(holdingSlug)}/attachments/shared/${encodeURIComponent(file.linkId)}/content`}
              target="_blank"
              rel="noreferrer"
            >
              {file.label}
            </a>
          ))}
        </section>
      )}
      <div className="actions">
        <Button appearance="standard"
          variant="primary"
          type="button"
          disabled={Boolean(busy) || acknowledged}
          onClick={() => void submit("acknowledge_delivery")}
        >
          {acknowledged
            ? "Delivery acknowledged"
            : busy === "acknowledge_delivery"
              ? "Recording…"
              : "Acknowledge delivery"}
        </Button>
        <Button appearance="standard"
          variant="secondary"
          type="button"
          disabled={Boolean(busy) || supportRequested || evidencePending}
          onClick={() => void submit("request_support")}
        >
          {supportRequested
            ? "Support requested"
            : busy === "request_support"
              ? "Sending…"
              : evidencePending
                ? "Wait for file scan"
                : "Request support"}
        </Button>
      </div>
      {saved && <InlineNotice tone="info" role="status">{saved}</InlineNotice>}
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
    </div>
  );
}
