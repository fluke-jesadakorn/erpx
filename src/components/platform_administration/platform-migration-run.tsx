"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, TextArea } from "@/components/ui/foundation";

import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work_projection/work-fetch";
import { useState } from "react";
import styles from "@/components/platform_administration/platform-migration-run.module.css";

type MigrationState = {
  applied: number;
  pending: string[];
  latestApplied: string | null;
  error: string | null;
};

export function PlatformMigrationRun({
  holdingId,
  migration,
  canRun,
}: {
  holdingId: string;
  migration: MigrationState;
  canRun: boolean;
}) {
  const router = useRouter();
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [result, setResult] = useState("");
  async function run() {
    if (reason.trim().length < 3) {
      setError("ระบุเหตุผลก่อนรัน migration");
      return;
    }
    setBusy(true);
    setError("");
    setResult("");
    try {
      const response = await workFetch("/api/v1/platform/operations/migrate", {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
        body: JSON.stringify({ holdingId, track: "holding-write", reason: reason.trim() }),
      });
      const body = (await response.json()) as {
        data?: { appliedBefore: number; appliedAfter: number; pending: string[] };
        error?: { message?: string };
      };
      if (!response.ok) throw new Error(body.error?.message ?? "Migration ไม่สำเร็จ");
      setResult(
        `รันสำเร็จ: ${body.data?.appliedBefore ?? 0} → ${body.data?.appliedAfter ?? 0} migration และเหลือ pending ${body.data?.pending.length ?? 0}`,
      );
      setReason("");
      router.refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Migration ไม่สำเร็จ");
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className={styles.layout}>
      <div className={styles.status}>
        <strong>
          {migration.error
            ? "Migration receipt unavailable"
            : migration.pending.length
              ? `${migration.pending.length} pending migration(s)`
              : "Schema is up to date"}
        </strong>
        <p className="muted">
          Applied {migration.applied}
          {migration.latestApplied ? ` · latest ${migration.latestApplied}` : ""}
        </p>
        {migration.pending.length > 0 && (
          <ul>
            {migration.pending.slice(0, 8).map((name) => (
              <li key={name}>
                <code>{name}</code>
              </li>
            ))}
          </ul>
        )}
        {migration.pending.length > 8 && (
          <p className="muted">และอีก {migration.pending.length - 8} รายการ</p>
        )}
      </div>
      {canRun ? (
        <div className={styles.action}>
          <Field appearance="standard">
            เหตุผลการรัน migration
            <TextArea appearance="standard"
              value={reason}
              onChange={(event) => setReason(event.target.value)}
              minLength={3}
              maxLength={1000}
              required
              placeholder="เช่น ติดตั้ง release ที่ผ่านการตรวจสอบแล้ว"
            />
          </Field>
          <Button appearance="standard" variant="primary" type="button" disabled={busy} onClick={() => void run()}>
            {busy ? "กำลังรัน…" : "Run Holding migrations"}
          </Button>
        </div>
      ) : (
        <p className="muted">
          Platform auditor ดูสถานะได้เท่านั้น ผู้ดูแล Platform ต้องเป็นผู้รัน migration
        </p>
      )}
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      {result && <InlineNotice tone="info" role="status">{result}</InlineNotice>}
    </div>
  );
}
