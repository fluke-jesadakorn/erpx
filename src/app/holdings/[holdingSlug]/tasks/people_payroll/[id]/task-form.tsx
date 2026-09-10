"use client";
import { useHydrated } from "@/components/ui/use-hydrated";
import { useRouter } from "next/navigation";
import { useRef, useState, useTransition } from "react";
import { ArrowRight, CheckCircle2, FileUp } from "lucide-react";
import { Dialog } from "@/components/ui/dialog";
import styles from "@/components/workplace/pages.module.css";
type Request = {
  id: string;
  start_date: string;
  days: number;
  handover_note: string;
  state: string;
  version: number;
};
type Submission = {
  startDate: string;
  days: number;
  handoverNote: string;
  expectedVersion: number;
};
export function LeaveTaskForm({
  workspaceBase,
  request,
  canApprove,
  canSubmit = true,
  blockedReason,
}: {
  workspaceBase: string;
  request: Request;
  canApprove: boolean;
  canSubmit?: boolean;
  blockedReason?: string | null;
}) {
  const router = useRouter();
  const [pending, setPending] = useState(false);
  const [refreshing, startRefresh] = useTransition();
  const ready = useHydrated();
  const working = pending || refreshing || !ready;
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [upload, setUpload] = useState("");
  const [draftValues, setDraftValues] = useState({
    startDate: request.start_date,
    days: String(request.days),
    handoverNote: request.handover_note,
  });
  const [review, setReview] = useState<
    Submission | { note: string; expectedVersion: number } | null
  >(null);
  const formRef = useRef<HTMLFormElement>(null);
  const attempt = useRef<{ signature: string; key: string } | null>(null);
  const base = `/api/v1${workspaceBase}`;
  async function confirm() {
    if (!review) return;
    setPending(true);
    setError("");
    const approval = "note" in review;
    const url = `${base}/leave-requests/${request.id}${approval ? "/approve" : ""}`;
    const body = JSON.stringify(review);
    const signature = url + body;
    if (attempt.current?.signature !== signature)
      attempt.current = { signature, key: crypto.randomUUID() };
    try {
      const response = await fetch(url, {
        method: approval ? "POST" : "PATCH",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": attempt.current.key,
        },
        body,
      });
      const data = (await response.json()) as { error?: { message?: string } };
      if (!response.ok) {
        if (response.status === 409)
          setError(
            "This request changed or your role is no longer eligible. Refresh the record before reviewing it again.",
          );
        else
          setError(
            data.error?.message ??
              "We couldn’t save this request. Your details are still here.",
          );
        return;
      }
      setMessage(
        approval
          ? "Approval recorded. The current stage is shown above."
          : "Request submitted. It’s ready for an independent review.",
      );
      setReview(null);
      attempt.current = null;
      startRefresh(() => router.refresh());
    } catch {
      setError(
        "The connection was interrupted. Retry to check and finish the same action. Your details are still here.",
      );
    } finally {
      setPending(false);
    }
  }
  async function uploadEvidence(file: File) {
    setPending(true);
    setError("");
    setUpload(`Uploading ${file.name}…`);
    try {
      const intentResponse = await fetch(`${base}/uploads`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          ownerType: "leave_request",
          ownerId: request.id,
          purpose: "evidence",
          filename: file.name,
          mediaType: file.type,
          bytes: file.size,
        }),
      });
      const intent = (await intentResponse.json()) as {
        data?: { uploadUrl: string };
        error?: { message?: string };
      };
      if (!intentResponse.ok || !intent.data)
        throw new Error(intent.error?.message ?? "The upload could not start.");
      const response = await fetch(intent.data.uploadUrl, {
        method: "PUT",
        headers: { "Content-Type": file.type },
        body: file,
      });
      const result = (await response.json()) as {
        error?: { message?: string };
      };
      if (!response.ok)
        throw new Error(result.error?.message ?? "The upload failed.");
      setUpload(`${file.name} · Uploaded and verified`);
      startRefresh(() => router.refresh());
    } catch (cause) {
      setUpload("Upload incomplete. Choose the file again to retry.");
      setError(
        cause instanceof Error ? cause.message : "The upload was interrupted.",
      );
    } finally {
      setPending(false);
    }
  }
  const approval = request.state === "pending_approval" && canApprove;
  const draft = ["draft", "rejected"].includes(request.state) && canSubmit;
  return (
    <>
      {message && (
        <div
          className="notice success-notice"
          role="status"
          style={{ marginTop: 18 }}
        >
          {message}
        </div>
      )}
      {draft || approval ? (
        <form
          ref={formRef}
          className="form card"
          onSubmit={(event) => {
            event.preventDefault();
            const form = new FormData(event.currentTarget);
            setError("");
            setReview(
              draft
                ? {
                    startDate: String(form.get("startDate")),
                    days: Number(form.get("days")),
                    handoverNote: String(form.get("handoverNote")),
                    expectedVersion: request.version,
                  }
                : {
                    note: String(form.get("note")),
                    expectedVersion: request.version,
                  },
            );
          }}
        >
          <header>
            <h2>{draft ? "Prepare your request" : "Review this request"}</h2>
            <p className="small">
              {draft
                ? "Add the dates, your handover plan, and any supporting evidence."
                : "Check the dates and handover above, then add your decision note."}
            </p>
          </header>
          {draft ? (
            <>
              <div className="field-grid">
                <label className="field">
                  First day
                  <input
                    disabled={working}
                    name="startDate"
                    type="date"
                    value={draftValues.startDate}
                    onChange={(event) =>
                      setDraftValues({
                        ...draftValues,
                        startDate: event.target.value,
                      })
                    }
                    required
                  />
                </label>
                <label className="field">
                  Leave days
                  <input
                    disabled={working}
                    name="days"
                    type="number"
                    min="1"
                    max="365"
                    value={draftValues.days}
                    onChange={(event) =>
                      setDraftValues({
                        ...draftValues,
                        days: event.target.value,
                      })
                    }
                    required
                  />
                </label>
              </div>
              <label className="field">
                Handover note
                <textarea
                  disabled={working}
                  name="handoverNote"
                  value={draftValues.handoverNote}
                  onChange={(event) =>
                    setDraftValues({
                      ...draftValues,
                      handoverNote: event.target.value,
                    })
                  }
                  maxLength={2048}
                />
              </label>
              <label className="field">
                <span>
                  <FileUp size={14} /> Supporting evidence
                </span>
                <small>
                  JPEG, PNG, WebP, PDF or CSV · Up to 10 MiB per file
                </small>
                <input
                  type="file"
                  aria-label="Supporting evidence"
                  disabled={working}
                  accept="image/jpeg,image/png,image/webp,application/pdf,text/csv"
                  onChange={(event) => {
                    const file = event.currentTarget.files?.[0];
                    event.currentTarget.value = "";
                    if (file) void uploadEvidence(file);
                  }}
                />
              </label>
              {upload && (
                <div className={styles.uploadState} role="status">
                  {upload}
                </div>
              )}
            </>
          ) : (
            <label className="field">
              Decision note
              <textarea
                disabled={working}
                name="note"
                maxLength={2048}
                placeholder="Add context for the requester (optional)…"
              />
            </label>
          )}
          {error && !review && (
            <div className="error" role="alert">
              {error}
            </div>
          )}
          <div className="form-actions" data-task-actions>
            <span>
              {draft
                ? "You’ll review the details before submitting."
                : "Your decision will be recorded with this request."}
            </span>
            {draft && (
              <button
                type="button"
                className="button secondary"
                disabled={working}
                onClick={async () => {
                  setPending(true);
                  setError("");
                  try {
                    const body = JSON.stringify({
                      ...draftValues,
                      days: Number(draftValues.days),
                      expectedVersion: request.version,
                      saveOnly: true,
                    });
                    const signature = "save" + body;
                    if (attempt.current?.signature !== signature)
                      attempt.current = { signature, key: crypto.randomUUID() };
                    const r = await fetch(
                      `${base}/leave-requests/${request.id}`,
                      {
                        method: "PATCH",
                        headers: {
                          "Content-Type": "application/json",
                          "Idempotency-Key": attempt.current.key,
                        },
                        body,
                      },
                    );
                    const data = (await r.json()) as {
                      error?: { message: string };
                    };
                    if (!r.ok)
                      throw new Error(
                        data.error?.message ?? "Unable to save draft.",
                      );
                    attempt.current = null;
                    startRefresh(() => router.refresh());
                  } catch (e) {
                    setError(
                      e instanceof Error ? e.message : "Unable to save draft.",
                    );
                  } finally {
                    setPending(false);
                  }
                }}
              >
                Save draft
              </button>
            )}
            <button
              className="button"
              disabled={working || (draft && !!blockedReason)}
            >
              {pending
                ? "Working…"
                : draft
                  ? "Review request"
                  : "Review approval"}
              <ArrowRight size={14} />
            </button>
          </div>
        </form>
      ) : (
        <div className="notice" style={{ marginTop: 20 }}>
          <CheckCircle2
            size={15}
            style={{ verticalAlign: "middle", marginRight: 8 }}
          />
          {request.state === "pending_approval"
            ? "Waiting for the named approver of the current stage."
            : request.state === "draft"
              ? "Only the requester can submit this draft."
              : `This request is ${request.state}.`}
        </div>
      )}
      <Dialog
        open={!!review}
        title={draft ? "Submit leave request" : "Approve leave"}
        onClose={() => {
          if (!working) setReview(null);
        }}
      >
        <p>
          Confirm these details before completing your{" "}
          {draft ? "request" : "decision"}.
        </p>
        <div className={styles.reviewSummary}>
          <div>
            <small>First day</small>
            {review && "startDate" in review
              ? review.startDate
              : request.start_date}
          </div>
          <div>
            <small>Duration</small>
            {review && "days" in review ? review.days : request.days} days
          </div>
          <div>
            <small>{draft ? "Handover" : "Decision note"}</small>
            <span style={{ whiteSpace: "pre-wrap" }}>
              {review && "handoverNote" in review
                ? review.handoverNote || "No handover note"
                : review && "note" in review
                  ? review.note || "No additional note"
                  : ""}
            </span>
          </div>
        </div>
        {error && (
          <div className="error" role="alert" style={{ marginBottom: 16 }}>
            {error}{" "}
            <button
              className="button secondary"
              onClick={() => {
                setReview(null);
                startRefresh(() => router.refresh());
              }}
              disabled={working}
            >
              Refresh record
            </button>
          </div>
        )}
        <div className={styles.reviewActions}>
          <button
            className="button secondary"
            disabled={working}
            onClick={() => setReview(null)}
          >
            Back to details
          </button>
          <button
            className="button"
            disabled={working}
            onClick={() => void confirm()}
          >
            {pending
              ? "Saving…"
              : draft
                ? "Submit leave request"
                : "Approve leave"}
          </button>
        </div>
      </Dialog>
    </>
  );
}
