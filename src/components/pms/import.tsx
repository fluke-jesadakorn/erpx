"use client";
import { Button, Field, TextArea, TextInput } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { betaFetch } from "@/lib/beta-client";
import type { PmsImportReport } from "@/server/modules/pms/public/rules/pms-import";

type ImportBatch = {
  id: string;
  state: string;
  version: number;
  created_by: string;
  approved_by: string | null;
  opening_journal_id: string | null;
  report: PmsImportReport;
};
export function PmsImport({
  api,
  base,
  propertyId,
  propertyVersion,
  businessDate,
  initial,
  userId,
  canPrepare,
  canApprove,
}: {
  api: string;
  base: string;
  propertyId: string;
  propertyVersion: number;
  businessDate: string;
  initial: ImportBatch[];
  userId: string;
  canPrepare: boolean;
  canApprove: boolean;
}) {
  const router = useRouter(),
    [batches, setBatches] = useState(initial),
    [body, setBody] = useState(""),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [journal, setJournal] = useState("");
  const attempts = useRef(new Map<string, string>());
  const template = {
    propertyId,
    expectedVersion: propertyVersion,
    cutoverDate: businessDate,
    currency: "THB",
    source: "legacy-cutover",
    roomTypes: [],
    rooms: [],
    rates: [],
    guests: [],
    reservations: [],
  };
  async function run(action: "preview" | "approve" | "apply", input: unknown) {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      const fingerprint = JSON.stringify([action, input]),
        key = attempts.current.get(fingerprint) ?? crypto.randomUUID();
      attempts.current.set(fingerprint, key);
      await betaFetch(api, { action, input }, key);
      attempts.current.delete(fingerprint);
      setBatches(
        await betaFetch<ImportBatch[]>(`${api}?property=${encodeURIComponent(propertyId)}`),
      );
      router.refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Please try again.");
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="content-stack">
      <p>
        Preview rooms, rates, guests and open stays before saving. Amounts are THB satang. Cutover
        must match the hotel&apos;s business date: <strong>{businessDate}</strong>.
      </p>
      <p>
        Finance reviews the guest opening totals against an independently posted opening journal.
        Applying the approved batch commits all records together. An interrupted attempt can be
        retried with the same key.
      </p>
      <p>
        <Link href={`${base}/finance/accounting`}>Opening journal in Finance →</Link> ·{" "}
        <Link href={`${base}/finance/hotels?property=${propertyId}`}>Hotel reconciliation →</Link> ·{" "}
        <a href="/pms-cutover-example.json" download>
          Example JSON
        </a>
      </p>
      {error && (
        <p className="alert" role="alert">
          {error}
        </p>
      )}
      {canPrepare && (
        <section className="card">
          <h2>Preview / ตรวจข้อมูลก่อนนำเข้า</h2>
          <Button
            variant="secondary"
            type="button"
            onClick={() => setBody(JSON.stringify(template, null, 2))}
          >
            Start with this hotel&apos;s template
          </Button>
          <Field className="form-field">
            Import JSON
            <TextArea
              aria-label="Import JSON"
              rows={12}
              value={body}
              disabled={busy}
              onChange={(event) => setBody(event.target.value)}
            />
          </Field>
          <Button
            type="submit"
            variant="primary"
            disabled={busy || !body.trim()}
            onClick={() => {
              try {
                void run("preview", JSON.parse(body));
              } catch {
                setError("Enter valid JSON.");
              }
            }}
          >
            Preview only
          </Button>
        </section>
      )}
      {canApprove && (
        <Field className="form-field">
          Posted opening journal ID / สมุดรายวันยอดยกมา
          <TextInput
            value={journal}
            onChange={(event) => setJournal(event.target.value)}
            placeholder="Leave empty only for a batch with zero opening balances"
          />
        </Field>
      )}
      {batches.map((batch) => (
        <section className="card" key={batch.id}>
          <div className="section-head">
            <h2>
              {batch.id.slice(-8)} · {batch.state}
            </h2>
            <span>Version {batch.version}</span>
          </div>
          <p>
            {Object.entries(batch.report.counts)
              .map(([kind, count]) => `${kind}: ${count}`)
              .join(" · ")}
          </p>
          <p>
            Opening: receivable {(batch.report.totals.due / 100).toFixed(2)} · advance{" "}
            {(batch.report.totals.advance / 100).toFixed(2)} · deposit{" "}
            {(batch.report.totals.deposit / 100).toFixed(2)} THB · prepaid tax{" "}
            {(batch.report.totals.advanceTax / 100).toFixed(2)}
          </p>
          {batch.report.errors.length > 0 && (
            <ul>
              {batch.report.errors.map((row, index) => (
                <li key={index}>
                  <strong>{row.row}</strong> · {row.message} ({row.code})
                </li>
              ))}
            </ul>
          )}
          {batch.opening_journal_id && (
            <Link href={`${base}/finance/journals/${batch.opening_journal_id}`}>
              Approved opening journal →
            </Link>
          )}
          {batch.state === "preview" && canApprove && batch.created_by !== userId && (
            <Button
              type="submit"
              variant="primary"
              disabled={busy || !batch.report.canApprove}
              onClick={() =>
                void run("approve", {
                  id: batch.id,
                  expectedVersion: batch.version,
                  openingJournalId: journal.trim() || null,
                })
              }
            >
              Finance: approve this preview
            </Button>
          )}
          {batch.state === "approved" && canPrepare && (
            <Button
              type="submit"
              variant="primary"
              disabled={busy}
              onClick={() => void run("apply", { id: batch.id, expectedVersion: batch.version })}
            >
              Apply approved batch / นำเข้าชุดที่อนุมัติ
            </Button>
          )}
          {batch.state === "applied" && (
            <p role="status">
              Applied successfully. Original keys and the approved opening ledger remain linked.
            </p>
          )}
        </section>
      ))}
      {!batches.length && <p>No import batches for this hotel yet.</p>}
    </div>
  );
}
