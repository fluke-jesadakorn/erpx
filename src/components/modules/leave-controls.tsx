"use client";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
export function LeaveControls({
  base,
  recordId,
  version,
  canReject = false,
  canCancel = false,
  initialOpen = false,
}: {
  base: string;
  recordId?: string;
  version?: number;
  canReject?: boolean;
  canCancel?: boolean;
  initialOpen?: boolean;
}) {
  const router = useRouter(),
    attempt = useRef<{ body: string; key: string } | null>(null);
  const [open, setOpen] = useState(initialOpen),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  async function submit(form: FormData, action?: string) {
    const body = JSON.stringify(
      recordId
        ? {
            action,
            expectedVersion: version,
            note: String(form.get("note") ?? ""),
          }
        : {
            startDate: String(form.get("startDate")),
            days: Number(form.get("days")),
            handoverNote: String(form.get("handoverNote")),
          },
    );
    if (attempt.current?.body !== body)
      attempt.current = { body, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const r = await fetch(
        `/api/v1${base}/leave-requests${recordId ? `/${recordId}/transition` : ""}`,
        {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": attempt.current.key,
          },
          body,
        },
      );
      const result = (await r.json()) as {
        data?: { id: string };
        error?: { message: string };
      };
      if (!r.ok)
        throw new Error(result.error?.message ?? "Could not save leave.");
      if (!recordId)
        router.push(`${base}/tasks/people_payroll/${result.data?.id}`);
      else router.refresh();
      setOpen(false);
      attempt.current = null;
    } catch (e) {
      setError(
        e instanceof Error
          ? e.message
          : "Connection interrupted. Retry this action.",
      );
    } finally {
      setBusy(false);
    }
  }
  if (recordId)
    return (
      <section className="card">
        <form className="business-form" onSubmit={(e) => e.preventDefault()}>
          <label>
            Decision note
            <textarea name="note" maxLength={2048} />
          </label>
          <div className="form-actions">
            {canReject && (
              <button
                className="button secondary"
                disabled={busy}
                type="button"
                onClick={(e) =>
                  submit(new FormData(e.currentTarget.form!), "reject")
                }
              >
                Reject request
              </button>
            )}
            {canCancel && (
              <button
                className="button secondary"
                disabled={busy}
                type="button"
                onClick={(e) =>
                  submit(new FormData(e.currentTarget.form!), "cancel")
                }
              >
                Cancel request
              </button>
            )}
          </div>
          {error && <p role="alert">{error}</p>}
        </form>
      </section>
    );
  return (
    <section className="card">
      <button className="button" onClick={() => setOpen(!open)}>
        Request leave
      </button>
      {open && (
        <form
          className="business-form"
          onSubmit={(e) => {
            e.preventDefault();
            submit(new FormData(e.currentTarget));
          }}
        >
          <label>
            First day
            <input type="date" name="startDate" required />
          </label>
          <label>
            Requested working days
            <input
              type="number"
              name="days"
              min={1}
              max={365}
              defaultValue={1}
              required
            />
          </label>
          <label>
            Handover plan
            <textarea name="handoverNote" maxLength={2048} />
          </label>
          <p className="muted">
            Enter working days under your company’s leave policy. Your
            department approver reviews first, then an independent HR
            administrator.
          </p>
          <button className="button" disabled={busy}>
            Save leave draft
          </button>
          {error && <p role="alert">{error}</p>}
        </form>
      )}
    </section>
  );
}
