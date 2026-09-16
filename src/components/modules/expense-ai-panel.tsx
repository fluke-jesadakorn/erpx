"use client";

import { workFetch } from "@/lib/work-fetch";
import Link from "next/link";
import { LoaderCircle, Sparkles } from "lucide-react";
import styles from "./expense.module.css";
import { useCallback, useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import {
  expenseAiFieldLabels,
  type ExpenseAiResult,
  type ExpenseAiSuggestionEvent,
} from "@/lib/expense-ai";
import { prepareDocumentPages } from "@/lib/document-pages";
type Status = {
  workerReady?: boolean;
  allowed: boolean;
  unavailable: string | null;
  hasEvidence: boolean;
  incomplete: { id: string; name: string; type: string; checksum: string }[];
  proposal: {
    id: string;
    state: string;
    result: ExpenseAiResult | null;
    error: string | null;
  } | null;
  stale: boolean;
  sourceVersion: number;
  evidenceDigest: string;
};
export function ExpenseAiPanel({
  base,
  expenseId,
  version,
  paymentId = null,
  evidenceKey = "",
  canManageAi = false,
  compact = false,
}: {
  base: string;
  expenseId: string;
  version: number;
  paymentId?: string | null;
  evidenceKey?: string;
  canManageAi?: boolean;
  compact?: boolean;
}) {
  const router = useRouter(),
    [status, setStatus] = useState<Status | null>(null),
    [error, setError] = useState(""),
    [pending, setPending] = useState(false),
    [progress, setProgress] = useState("");
  const attempted = useRef(""),
    filled = useRef(""),
    key = useRef<{ body: string; id: string } | null>(null),
    preparing = useRef(false);
  const path = `/api/v1${base}/expenses/${expenseId}/ai`,
    query = paymentId ? `?payment=${encodeURIComponent(paymentId)}` : "";
  function fill(result: ExpenseAiResult, overwrite: boolean) {
    window.dispatchEvent(
      new CustomEvent<ExpenseAiSuggestionEvent>("erpx:expense-ai", {
        detail: {
          expenseId,
          paymentId,
          fields: result.fields,
          proposalId: status?.proposal?.id,
          overwrite,
        },
      }),
    );
  }
  const refresh = useCallback(async () => {
    const response = await workFetch(path + query, { cache: "no-store" }),
      body = (await response.json()) as {
        data: Status;
        error?: { message: string };
      };
    if (!response.ok)
      throw Error(body.error?.message ?? "AI preparation is unavailable.");
    setStatus(body.data);
    return body.data;
  }, [path, query]);
  const prepare = useCallback(
    async (current: Status, retry = false) => {
      if (preparing.current) return;
      preparing.current = true;
      setPending(true);
      setError("");
      try {
        for (const attachment of current.incomplete) {
          setProgress(`Preparing ${attachment.name}…`);
          const response = await workFetch(
            `/api/v1${base}/attachments/${attachment.id}/content`,
          );
          if (!response.ok)
            throw Error("The original document is unavailable.");
          const file = new File([await response.blob()], attachment.name, {
            type: attachment.type,
          });
          await prepareDocumentPages(file, async (page, total, image) => {
            setProgress(
              `Preparing ${attachment.name}: page ${page} of ${total}`,
            );
            const response = await workFetch(
              `/api/v1${base}/attachments/${attachment.id}/derivatives?page=${page}&total=${total}&original=${attachment.checksum}`,
              {
                method: "POST",
                headers: {
                  "Content-Type": "image/jpeg",
                  "Idempotency-Key": `render-${attachment.id}-${page}`,
                },
                body: image,
              },
            );
            const data = (await response.json()) as {
              error?: { message: string };
            };
            if (!response.ok)
              throw Error(
                data.error?.message ?? "Unable to prepare this page.",
              );
          });
        }
        const body = JSON.stringify({
          expectedVersion: current.sourceVersion,
          paymentId,
          retry,
        });
        if (key.current?.body !== body)
          key.current = { body, id: crypto.randomUUID() };
        const response = await workFetch(path, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": key.current.id,
          },
          body,
        });
        const data = (await response.json()) as { error?: { message: string } };
        if (!response.ok)
          throw Error(data.error?.message ?? "AI could not start.");
        key.current = null;
        await refresh();
      } catch (error) {
        setError(
          error instanceof Error
            ? error.message
            : "AI could not prepare this document. Complete the fields manually.",
        );
      } finally {
        preparing.current = false;
        setPending(false);
        setProgress("");
      }
    },
    [base, path, paymentId, refresh],
  );
  useEffect(() => {
    let stopped = false;
    async function load() {
      try {
        const current = await refresh();
        if (stopped) return;
        const fingerprint = `${version}:${current.evidenceDigest}`;
        if (
          current.allowed &&
          !current.unavailable &&
          current.hasEvidence &&
          !current.proposal &&
          attempted.current !== fingerprint
        ) {
          attempted.current = fingerprint;
          await prepare(current);
        }
      } catch (error) {
        if (!stopped)
          setError(
            error instanceof Error ? error.message : "AI is unavailable.",
          );
      }
    }
    void load();
    return () => {
      stopped = true;
    };
  }, [refresh, prepare, version, evidenceKey]);
  useEffect(() => {
    if (
      !status?.proposal ||
      !["queued", "running"].includes(status.proposal.state)
    )
      return;
    const interval = window.setInterval(() => {
      void refresh().catch(() =>
        setError("Unable to check AI progress. Your draft is safe."),
      );
    }, 3000);
    return () => clearInterval(interval);
  }, [status?.proposal, refresh]);
  useEffect(() => {
    const proposal = status?.proposal;
    if (status)
      window.dispatchEvent(
        new CustomEvent<ExpenseAiSuggestionEvent>("erpx:expense-ai", {
          detail: {
            expenseId,
            paymentId,
            fields: [],
            overwrite: false,
            invalidate: true,
            proposalId: proposal?.state === "ready" ? proposal.id : null,
          },
        }),
      );
    if (
      proposal?.state === "ready" &&
      proposal.result &&
      filled.current !== proposal.id
    ) {
      filled.current = proposal.id;
      window.dispatchEvent(
        new CustomEvent<ExpenseAiSuggestionEvent>("erpx:expense-ai", {
          detail: {
            expenseId,
            paymentId,
            fields: proposal.result.fields,
            proposalId: proposal.id,
            overwrite: false,
          },
        }),
      );
    }
  }, [status, expenseId, paymentId]);
  const result = status?.proposal?.result;
  const running = pending || ["queued", "running"].includes(status?.proposal?.state ?? "");
  async function extract() {
    if (running) return;
    setError("");
    try {
      const current = await refresh();
      if (!current.allowed || current.unavailable || !current.hasEvidence) return;
      if (!["queued", "running"].includes(current.proposal?.state ?? "")) {
        await prepare(current, !!current.proposal);
      }
    } catch (error) { setError(error instanceof Error ? error.message : "Unable to extract this receipt. Try again."); }
  }
  return (
    <section className={compact ? styles.aiExtract : "card"} aria-label="Receipt AI extraction" aria-busy={running}>
      <div className={styles.aiExtractHeader}>
        <div><h2><Sparkles size={18} /> Fill from receipt</h2><p>Read attached receipts and fill empty fields for review.</p></div>
        <button type="button" className="button" disabled={running || !!status && (!status.allowed || !status.hasEvidence || !!status.unavailable)} onClick={() => void extract()}>
          {running ? <LoaderCircle size={16} className={styles.aiSpinner} /> : <Sparkles size={16} />}
          {running ? "Extracting…" : "AI extract"}
        </button>
      </div>
      {!compact && <p className="muted">
        Suggestions are based on attached evidence. Existing entries are
        preserved. Review every field before saving or approving.
      </p>}
      {(error || status?.unavailable || status?.proposal?.error) && (
        <p className="notice" role="status">
          {error || status?.unavailable || status?.proposal?.error}
        </p>
      )}
      {status?.unavailable && <p>{canManageAi ? <Link href={`${base}/settings?section=ai`}>Set up receipt reading in AI providers</Link> : "Ask your company owner or IT administrator to enable receipt reading."}</p>}
      {status?.workerReady === false && status.proposal && ["queued", "running"].includes(status.proposal.state) && <p role="status">The AI worker is currently unavailable. Your receipt and draft are saved; processing will resume when it returns. You can complete the fields manually.</p>}
      {result && <p role="status">AI has prepared the receipt details. Available fields are filled for review; your own edits are preserved.</p>}
      {progress && <p role="status">{progress}</p>}
      {!status?.hasEvidence && (
        <p>Attach your receipt to prepare suggestions automatically.</p>
      )}
      {status?.proposal &&
        ["queued", "running"].includes(status.proposal.state) && (
          <p role="status">
            {status.proposal.state === "queued"
              ? "Queued for document preparation. You can continue editing."
              : "Reading evidence and preparing suggestions…"}
          </p>
        )}
      {status?.stale && !result && (
        <p>The source changed. Earlier suggestions are no longer current.</p>
      )}
      {result && (
        <details className={styles.aiResult}>
          <summary>Extraction details and sources{result.conflicts.length ? " · Needs review" : ""}</summary>
          <p>
            <strong>AI summary:</strong> {result.summary}
          </p>
          {result.conflicts.length > 0 && (
            <div className="notice">
              <strong>Evidence needs review</strong>
              <ul>
                {result.conflicts.map((c) => (
                  <li key={c}>{c}</li>
                ))}
              </ul>
            </div>
          )}
          {result.missing.length > 0 && (
            <p>Still needed: {result.missing.join(" · ")}</p>
          )}
          <details>
            <summary>Suggested fields and sources</summary>
            {result.fields
              .filter((f) => f.value !== null)
              .map((field) => (
                <div className="row" key={field.name}>
                  <div className="row-main">
                    <strong>{expenseAiFieldLabels[field.name]}</strong>
                    <p>{field.value}</p>
                    <div>
                      {field.citations.map((c) => (
                        <a
                          key={`${c.attachmentId}:${c.page}`}
                          href={`/api/v1${base}/attachments/${c.attachmentId}/content?page=${c.page}`}
                          target="_blank"
                          rel="noreferrer"
                        >
                          Evidence · page {c.page}{" "}
                        </a>
                      ))}
                    </div>
                  </div>
                  {status?.allowed && field.name !== "receiptTotal" && (
                    <button
                      className="button secondary"
                      type="button"
                      onClick={() => fill({ ...result, fields: [field] }, true)}
                    >
                      Use suggestion
                    </button>
                  )}
                </div>
              ))}
          </details>
        </details>
      )}
      {!compact && result && status?.allowed &&
        status.hasEvidence &&
        !status.unavailable &&
        !["queued", "running"].includes(status.proposal?.state ?? "") && (
          <button
            className="button secondary"
            disabled={pending}
            onClick={() => {
              key.current = null;
              void refresh().then(current => prepare(current, true)).catch(error => setError(error instanceof Error ? error.message : "Unable to extract receipt."));
              router.refresh();
            }}
          >
            {pending
              ? "Preparing…"
              : result
                ? "Extract again"
                : "Retry AI preparation"}
          </button>
        )}
    </section>
  );
}
