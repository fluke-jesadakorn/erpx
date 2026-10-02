"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { Dialog } from "@/components/ui/dialog";
import { workFetch } from "@/lib/work-fetch";

type RelocationPreview = {
  impact: { staff: number; positions: number; assignments: number; child_nodes: number; warehouses: number };
  previewSnapshot: string;
};

export function DepartmentRelocationReview({ base, department, sourceBranch, destinationBranch, onClose, onComplete }: {
  base: string;
  department: { id: string; name: string; version: number };
  sourceBranch: string;
  destinationBranch: { id: string; name: string };
  onClose: () => void;
  onComplete: (message: string) => void;
}) {
  const router = useRouter();
  const [reason, setReason] = useState("");
  const [preview, setPreview] = useState<RelocationPreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const input = {
    action: "relocate", targetNodeId: department.id, kind: "department", slug: null,
    displayName: department.name, branchId: destinationBranch.id,
    expectedNodeVersion: department.version, expectedVersion: 0, reason,
  };
  async function send<T>(suffix: string, body: unknown): Promise<T> {
    const response = await workFetch(`/api/v1${base}/people/organization/changes/${suffix}`, {
      method: "POST",
      headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
      body: JSON.stringify(body),
    });
    const result = await response.json();
    if (!response.ok) throw new Error(result.error?.message ?? result.message ?? "Unable to review this move.");
    return result.data as T;
  }
  async function review() {
    setBusy(true); setError("");
    try { setPreview(await send<RelocationPreview>("preview", input)); }
    catch (caught) { setError(caught instanceof Error ? caught.message : "Unable to review this move."); }
    finally { setBusy(false); }
  }
  async function submit() {
    if (!preview) return;
    setBusy(true); setError("");
    try {
      await send("submit", { ...input, previewSnapshot: preview.previewSnapshot });
      onComplete("Department move submitted for independent review.");
      router.refresh();
    } catch (caught) { setError(caught instanceof Error ? caught.message : "Unable to submit this move."); }
    finally { setBusy(false); }
  }
  return <Dialog open onClose={busy ? () => {} : onClose} title="Review department move" presentation="people-drawer">
    <div className="business-form" style={{ padding: 20 }}>
      <p><strong>{department.name}</strong><br />{sourceBranch} → {destinationBranch.name}</p>
      {!preview ? <>
        <label>Reason<textarea value={reason} onChange={event => setReason(event.target.value)} minLength={3} maxLength={1000} required /></label>
        <p>The department and its current people, positions, and scoped access assignments move together after independent approval.</p>
        {error && <p role="alert" className="error">{error}</p>}
        <div><button className="button secondary" type="button" onClick={onClose} disabled={busy}>Cancel</button>{" "}<button className="button" type="button" onClick={() => void review()} disabled={busy || reason.trim().length < 3}>{busy ? "Checking…" : "Preview impact"}</button></div>
      </> : <>
        <dl className="staff-facts"><dt>Active people</dt><dd>{preview.impact.staff}</dd><dt>Positions</dt><dd>{preview.impact.positions}</dd><dt>Access assignments</dt><dd>{preview.impact.assignments}</dd></dl>
        <p>The reviewer will confirm this change. If affected records change before approval, a fresh preview is required.</p>
        {error && <p role="alert" className="error">{error}</p>}
        <div><button className="button secondary" type="button" onClick={() => setPreview(null)} disabled={busy}>Back</button>{" "}<button className="button" type="button" onClick={() => void submit()} disabled={busy}>{busy ? "Submitting…" : "Submit for review"}</button></div>
      </>}
    </div>
  </Dialog>;
}
