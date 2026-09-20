"use client";
import { useState } from "react";
import {
  thailandPackVersion,
  thailandReviewPack,
  thailandTemplates,
} from "@/lib/legal/thailand";
import s from "./legal.module.css";
export function LegalThailand({
  people,
  call,
  refresh,
}: {
  people: { id: string; name: string }[];
  call: (path: string, body?: unknown) => Promise<unknown>;
  refresh: () => Promise<void>;
}) {
  const [owner, setOwner] = useState(""),
    [preview, setPreview] = useState<{
      rows: unknown[];
      digest: string;
      errors: unknown[];
    } | null>(null),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false);
  return (
    <div className={s.toolContent}>
      <p className={s.small}>6 หัวข้อกฎหมาย · 4 แม่แบบสัญญา · {thailandPackVersion}</p>
      <p className={s.notice}>
        นำเข้าเป็นร่างเท่านั้น ผู้ตรวจต้องระบุวันมีผล เงื่อนไขใช้
        และหลักฐานตรวจรับก่อนเปิดใช้กฎหรือแม่แบบ
      </p>
      <label className={s.field}>
        ผู้รับผิดชอบ
        <select
          value={owner}
          onChange={(e) => {
            setOwner(e.target.value);
            setPreview(null);
          }}
        >
          <option value="">เลือกผู้รับผิดชอบตรวจร่าง</option>
          {people.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </label>
      <div className={s.packGrid}>{thailandReviewPack.map((r) => (
        <article className={s.packCard} key={r.title}>
          <strong>{r.title.replace(/^ไทย · /, "")}</strong>
          <a href={r.source} target="_blank" rel="noreferrer">
            แหล่งทางการ
          </a>
        </article>
      ))}</div>
      {error && <p role="alert">{error}</p>}
      <button
        className={s.button}
        disabled={busy || !owner}
        onClick={async () => {
          setBusy(true);
          try {
            const rows = [
              ...thailandReviewPack.map((r) => ({
                kind: "rule",
                title: r.title,
                ownerId: owner,
                description: `${thailandPackVersion} · รอตรวจรับ`,
                data: {
                  source: r.source,
                  applicability: r.applicability,
                  procedure: r.procedure,
                  effectiveDate: "",
                  reviewNote: "",
                },
              })),
              ...thailandTemplates.map((t) => ({
                kind: "template",
                title: t.title,
                ownerId: owner,
                description: `${thailandPackVersion} · รอตรวจรับ`,
                data: {
                  templateType: "template",
                  variables: t.variables,
                  body: t.body,
                },
              })),
            ];
            setPreview(
              (await call("", {
                action: "import.preview",
                rows,
              })) as typeof preview,
            );
            setError("");
          } catch (e) {
            setError((e as Error).message);
          } finally {
            setBusy(false);
          }
        }}
      >
        ตรวจสอบก่อนนำเข้า
      </button>
      {preview && (
        <>
          <p>
            ร่าง {preview.rows.length} รายการ · ข้อผิดพลาด{" "}
            {preview.errors.length}
          </p>
          <p className={s.small}>รายการจะถูกสร้างเป็นฉบับร่างเพื่อให้ผู้รับผิดชอบตรวจรับก่อนเปิดใช้</p>
          <button
            className={s.button}
            disabled={busy || !!preview.errors.length}
            onClick={async () => {
              setBusy(true);
              try {
                await call("", {
                  action: "import.commit",
                  rows: preview.rows,
                  digest: preview.digest,
                });
                setPreview(null);
                await refresh();
              } catch (e) {
                setError((e as Error).message);
              } finally {
                setBusy(false);
              }
            }}
          >
            ยืนยันนำเข้าร่างที่ตรวจแล้ว
          </button>
        </>
      )}
    </div>
  );
}
