"use client";

import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import { useState } from "react";
import type { PlatformDeadLetter } from "@/server/platform-operations";

export function PlatformJobRecovery({ holdingId, events, canRecover }: { holdingId: string; events: PlatformDeadLetter[]; canRecover: boolean }) {
  const router = useRouter();
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [reason, setReason] = useState<Record<string, string>>({});
  if (!events.length) return null;
  async function recover(event: PlatformDeadLetter) {
    const explanation = reason[event.id]?.trim() ?? "";
    if (explanation.length < 3) {
      setError(`อธิบายเหตุผลก่อนกู้คืนงาน ${event.id}`);
      return;
    }
    setBusy(event.id);
    setError("");
    try {
      const response = await workFetch("/api/v1/platform/operations/recover", {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
        body: JSON.stringify({ holdingId, eventId: event.id, expectedAttempts: event.attempts, expectedLastError: event.lastError, reason: explanation }),
      });
      const body = await response.json() as { error?: { message?: string } };
      if (!response.ok) throw new Error(body.error?.message ?? "กู้คืนงานไม่สำเร็จ");
      setReason((current) => ({ ...current, [event.id]: "" }));
      router.refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "กู้คืนงานไม่สำเร็จ");
    } finally {
      setBusy(null);
    }
  }
  return <div className="card-grid">
    {events.map((event) => <article className="card" key={event.id}>
      <strong>{event.eventType}</strong>
      <p className="muted">งาน {event.id} · รายการ {event.aggregateId} · ล้มเหลว {event.attempts} ครั้ง</p>
      {event.lastError && <p role="alert">{event.lastError}</p>}
      <p className="muted">กู้คืนแล้วจะล้าง dead-letter และส่งกลับเข้าคิว โดยเก็บเหตุผลและ audit ไว้</p>
      {canRecover && <>
        <label>เหตุผลการกู้คืน<textarea value={reason[event.id] ?? ""} onChange={(input) => setReason((current) => ({ ...current, [event.id]: input.target.value }))} minLength={3} maxLength={1000} required /></label>
        <button className="button" type="button" disabled={busy !== null} onClick={() => void recover(event)}>{busy === event.id ? "กำลังกู้คืน…" : "กู้คืนเข้าคิว"}</button>
      </>}
      {!canRecover && <p className="muted">บทบาทนี้ดูได้อย่างเดียว ผู้ดูแล Platform ต้องเป็นผู้กู้คืน</p>}
    </article>)}
    {error && <p role="alert" className="error">{error}</p>}
  </div>;
}
