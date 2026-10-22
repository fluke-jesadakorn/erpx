"use client";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";

import { ReviewEvidence } from "@erpx/workplace/components/review-evidence";
import { Button } from "@erpx/ui/components/foundation";

import { useEffect, useRef, useState } from "react";
import { Dialog } from "@erpx/ui/components/dialog";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import { workItemHref, type WorkItem } from "@erpx/feature-work_projection/lib/work-item";
import {
  workReviewCommand,
  type WorkReview,
  type WorkReviewKind,
  type ReviewDecision,
} from "@erpx/feature-work_projection/lib/work-review";
import { ReviewDecisionForm } from "@erpx/workplace/components/review-decision";
import styles from "@erpx/workplace/components/work-review-panel.module.css";

export function WorkReviewPanel({
  base,
  item,
  returnTo,
  onClose,
  onDecided,
}: {
  base: string;
  item: WorkItem | null;
  returnTo: string;
  onClose: () => void;
  onDecided: () => Promise<void>;
}) {
  return (
    <Dialog appearance="standard" open={!!item} onClose={onClose} title="Review approval" presentation="form-drawer" className={styles.drawer}>
      {item && (
        <Review
          key={`${base}:${item.type}:${item.id}`}
          base={base}
          item={item}
          returnTo={returnTo}
          onDecided={onDecided}
        />
      )}
    </Dialog>
  );
}
function Review({
  base,
  item,
  returnTo,
  onDecided,
}: {
  base: string;
  item: WorkItem;
  returnTo: string;
  onDecided: () => Promise<void>;
}) {
  const [data, setData] = useState<WorkReview | null>(null),
    [error, setError] = useState(""),
    [loading, setLoading] = useState(true),
    [revision, setRevision] = useState(0),
    [note, setNote] = useState(""),
    [pending, setPending] = useState(false);
  const attempt = useRef<{ body: string; key: string } | null>(null);
  useEffect(() => {
    const abort = new AbortController();
    void (async () => {
      try {
        const response = await workFetch(
          `/api/v1${base}/work/review?${new URLSearchParams({ kind: item.type, id: item.id })}`,
          { cache: "no-store", signal: abort.signal },
        );
        const body = await response.json();
        if (!response.ok) throw Error(body.error?.message ?? "This approval could not be loaded.");
        if (!abort.signal.aborted) {
          setData(body.data);
          setError("");
        }
      } catch (cause) {
        if (!abort.signal.aborted)
          setError(cause instanceof Error ? cause.message : "This approval could not be loaded.");
      } finally {
        if (!abort.signal.aborted) setLoading(false);
      }
    })();
    return () => abort.abort();
  }, [base, item.type, item.id, revision]);
  function reload() {
    setData(null);
    setLoading(true);
    attempt.current = null;
    setRevision((value) => value + 1);
  }
  async function decide(decision: ReviewDecision) {
    if (!data || !decision || pending || !data.permittedActions.includes(decision))
      return;
    const command = workReviewCommand(
        item.type as WorkReviewKind,
        item.id,
        data.version,
        decision,
        note.trim(),
      ),
      body = JSON.stringify(command.body);
    if (attempt.current?.body !== body) attempt.current = { body, key: crypto.randomUUID() };
    setPending(true);
    try {
      const response = await workFetch(`/api/v1${base}${command.path}`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
        body,
      });
      const result = await response.json();
      if (!response.ok) {
        if ([401, 403, 404, 409].includes(response.status)) {
          setData(null);
                      attempt.current = null;
        }
        throw Error(
          result.error?.message ??
            "The decision could not be saved. You can retry the same decision.",
        );
      }
      await onDecided();
    } catch (cause) {
      setError(
        cause instanceof Error
          ? cause.message
          : "Connection lost. Retry to check the same decision.",
      );
    } finally {
      setPending(false);
    }
  }
  return (
    <div className={styles.review}>
      {loading && <InlineNotice tone="info" role="status">Loading the source record and your current authority…</InlineNotice>}
      {error && (
        <InlineNotice tone="danger" role="alert">
          <p>{error}</p>
          <Button appearance="standard" type="submit" variant="secondary" disabled={pending} onClick={reload}>
            Reload and review
          </Button>
        </InlineNotice>
      )}
      {data && (
        <>
          <div>
            <small>
              {data.source.label} · {data.status}
            </small>
            <h3>{data.title}</h3>
            <p>Submitted by {data.requester}</p>
          </div>
          {data.description && <p className={styles.description}>{data.description}</p>}
          <dl className={styles.facts}>
            {data.facts.map((fact, index) => (
              <div key={index}>
                <dt>{fact.label}</dt>
                <dd>{fact.value}</dd>
              </div>
            ))}
          </dl>
          <section>
            <h3>Evidence</h3>
            {data.evidenceMessage && <p>{data.evidenceMessage}</p>}
            {data.evidence.length ? (
              <div className={styles.evidence}>{data.evidence.map((file,index)=><ReviewEvidence key={index} file={file}/>)}</div>
            ) : (
              !data.evidenceMessage && <p>No attached evidence.</p>
            )}
          </section>
          <section>
            <h3>Recorded route</h3>
            <p>Current: {data.currentStep}</p>
            <ol>
              {data.steps.map((step, index) => (
                <li key={index}>
                  {step.label} · {step.person} <small>({step.state})</small>
                </li>
              ))}
            </ol>
            <p>Next: {data.nextStep}</p>
          </section>
          {data.blocker && (
            <InlineNotice tone="info" role="status" className={styles.blocker}>
              {data.blocker}
            </InlineNotice>
          )}
          {data.fullReviewReason && <p>{data.fullReviewReason}</p>}
          {data.permittedActions.length ? <ReviewDecisionForm actions={data.permittedActions} busy={pending} note={note} onNote={setNote} minimum={action=>data.reasonMinimum?.[action] ?? 3} onConfirm={action=>void decide(action)} /> : !data.fullReviewReason && <p>No decision is currently available for your role. Open the full record for its next steps.</p>}

        </>
      )}
      <UiActionLink appearance="standard" variant="secondary" href={workItemHref(base, item, returnTo)}>
        Open full record
      </UiActionLink>
    </div>
  );
}
