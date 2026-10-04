"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button } from "@/components/ui/foundation";

import { workFetch } from "@/lib/work-fetch";
import { useEffect, useState } from "react";
import { useHydrated } from "@/components/ui/use-hydrated";
import { useStaffCommand } from "@/components/modules/staff-work-controls";
import type { requestAiStatus } from "@/server/request-ai";
import styles from "./services.module.css";
type Status = Awaited<ReturnType<typeof requestAiStatus>>;
export function RequestAi({
  base,
  id,
  version,
  kind,
  editable,
}: {
  base: string;
  id: string;
  version: number;
  kind: string;
  editable: boolean;
}) {
  const ready = useHydrated(),
    command = useStaffCommand(base),
    [status, setStatus] = useState<Status | null>(null),
    [error, setError] = useState("");
  const url = `/api/v1${base}/requests/${id}/ai`;
  useEffect(() => {
    let current = true,
      timer: ReturnType<typeof setTimeout> | undefined;
    async function load() {
      try {
        const r = await workFetch(url),
          body = (await r.json()) as {
            data: Status;
            error?: { message: string };
          };
        if (!r.ok) throw Error(body.error?.message ?? "AI is unavailable.");
        if (current) {
          setStatus(body.data);
          window.dispatchEvent(
            new CustomEvent("erpx:request-ai", {
              detail: {
                requestId: id,
                sourceVersion: version,
                proposalId: body.data.proposalId,
                invalidate: true,
                fields: [],
                overwrite: false,
              },
            }),
          );
          if (["queued", "running"].includes(body.data.state))
            timer = setTimeout(() => void load(), 2500);
        }
      } catch (e) {
        if (current) setError(e instanceof Error ? e.message : "AI is unavailable.");
      }
    }
    void load();
    return () => {
      current = false;
      if (timer) clearTimeout(timer);
    };
  }, [url, id, version, command.busy]);
  async function apply(name: "title" | "description" | "category") {
    try {
      const r = await workFetch(url),
        body = (await r.json()) as {
          data: Status;
          error?: { message: string };
        };
      if (!r.ok) throw Error(body.error?.message ?? "AI is unavailable.");
      setStatus(body.data);
      if (body.data.state !== "ready" || body.data.sourceVersion !== version || !body.data.result)
        throw Error("The source changed. Prepare new suggestions before applying them.");
      window.dispatchEvent(
        new CustomEvent("erpx:request-ai", {
          detail: {
            requestId: id,
            sourceVersion: version,
            proposalId: body.data.proposalId,
            fields: [{ name, value: body.data.result[name] }],
            overwrite: true,
          },
        }),
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unable to apply suggestion.");
    }
  }
  return (
    <details className={`card ${styles.details}`}>
      <summary>AI preparation</summary>
      <p className={styles.quiet}>
        Suggests wording and a service category from the saved request text. Review suggestions
        before using them.
      </p>
      {status?.unavailable && <InlineNotice tone="info" role="status">{status.unavailable}</InlineNotice>}
      {status?.result && (
        <>
          <p className={styles.article}>{status.result.summary}</p>
          <p className={styles.quiet}>
            Source: this saved request, version {status.sourceVersion}.
          </p>
          {editable &&
            (["title", "description", ...(kind === "service" ? ["category"] : [])] as const).map(
              (name) => {
                const value = status.result?.[name as "title" | "description" | "category"];
                return value ? (
                  <div className={styles.history} key={name}>
                    <strong>Suggested {name}</strong>
                    <p>{value}</p>
                    <Button appearance="standard"
                      variant="secondary"
                      type="button"

                      onClick={() => void apply(name as "title" | "description" | "category")}
                    >
                      Use suggested {name}
                    </Button>
                  </div>
                ) : null;
              },
            )}
        </>
      )}
      {status?.allowed && (
        <Button appearance="standard"
          variant="secondary"
          type="button"

          disabled={
            !ready ||
            command.busy ||
            !!status.unavailable ||
            ["queued", "running"].includes(status.state)
          }
          onClick={() =>
            void command.send(`requests/${id}/ai`, {
              expectedVersion: version,
              retry: ["failed", "stale"].includes(status.state),
            })
          }
        >
          {status.state === "queued" || status.state === "running"
            ? "Preparing suggestions…"
            : status.state === "failed"
              ? "Retry AI preparation"
              : "Prepare with AI"}
        </Button>
      )}
      {(error || command.error || status?.error) && (
        <InlineNotice tone="danger" role="alert">
          {error || command.error || status?.error} Your saved request and manual edits are
          preserved.
        </InlineNotice>
      )}
    </details>
  );
}
