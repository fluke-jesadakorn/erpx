"use client";

import { useEffect, useRef, useState } from "react";
import { Button, InlineNotice } from "@erpx/ui/components/foundation";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import type { NativeQualityCancellationPage } from "../adapters/server/native-quality";
import { qualityHistorySessionKey, mergeQualityHistory, beginQualityHistoryRequest, finishQualityHistoryRequest } from "../lib/quality-cancellation-history";

/** A page is only appended when the server confirms the same full-history revision. */
export function QualityCancellationHistory({
  holdId, endpoint, initialPage, contextResetKey,
}: {
  holdId: string;
  endpoint: string;
  initialPage: NativeQualityCancellationPage;
  /** Trusted selected Holding, Company and assignment identity/version. */
  contextResetKey: string;
}) {
  if (!contextResetKey) return <InlineNotice tone="danger" role="alert">Select a current work context to view cancellation history.</InlineNotice>;
  const sessionKey = JSON.stringify([qualityHistorySessionKey(holdId, contextResetKey), endpoint, initialPage.headVersion, initialPage.historyRevision]);
  return <QualityCancellationHistorySession key={sessionKey} holdId={holdId} endpoint={endpoint} initialPage={initialPage} />;
}

function QualityCancellationHistorySession({holdId, endpoint, initialPage}: {
  holdId: string; endpoint: string; initialPage: NativeQualityCancellationPage;
}) {
  const [page, setPage] = useState(initialPage);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const generation = useRef(0);
  const active = useRef<AbortController | null>(null);

  useEffect(() => {
    return () => { generation.current += 1; active.current?.abort(); active.current = null; };
  }, []);

  async function load(restart = false) {
    if (pending || (!restart && page.nextBefore === null)) return;
    const controller = new AbortController();
    if (!beginQualityHistoryRequest(active, controller)) return;
    const current = ++generation.current;
    setPending(true);
    setError("");
    try {
      const url = new URL(endpoint, window.location.origin);
      if (url.origin !== window.location.origin) throw new Error("Invalid history address");
      url.searchParams.delete("before");
      url.searchParams.delete("head");
      url.searchParams.delete("revision");
      if (!restart) {
        url.searchParams.set("before", String(page.nextBefore));
        url.searchParams.set("head", String(page.headVersion));
        url.searchParams.set("revision", page.historyRevision);
      }
      const response = await workFetch(url.pathname + url.search, { signal: controller.signal });
      const body: unknown = await response.json();
      if (current !== generation.current) return;
      if (!response.ok) {
        const code = typeof body === "object" && body !== null && "error" in body && typeof body.error === "object" && body.error !== null && "code" in body.error ? body.error.code : undefined;
        if (response.status === 409 && code === "QUALITY_HISTORY_CHANGED") {
          setPage({ ...initialPage, items: [], nextBefore: null });
          setError("Cancellation history changed. Reload it to see the current history.");
          return;
        }
        throw new Error("Unable to load cancellation history.");
      }
      const value = typeof body === "object" && body !== null && "data" in body ? body.data : body;
      if (!validPage(value, holdId)) throw new Error("Invalid cancellation history response.");
      const next = restart ? value : mergeQualityHistory(page, value);
      if (!next) throw new Error("Invalid cancellation history response.");
      setPage(next);
    } catch (cause) {
      if (current === generation.current && !controller.signal.aborted) setError(cause instanceof Error ? cause.message : "Unable to load cancellation history.");
    } finally {
      finishQualityHistoryRequest(active, controller);
      if (current === generation.current) setPending(false);
    }
  }

  return <section>
    <h2>Cancellation history</h2>
    {page.items.map(item => <p key={item.id}>Version {item.version} · {item.action} · {item.actorId} · {item.reason}</p>)}
    {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
    {error ? <Button type="button" disabled={pending} onClick={() => void load(true)}>Reload history</Button> : page.nextBefore !== null && <Button type="button" disabled={pending} onClick={() => void load()}>{pending ? "Loading…" : "Load more"}</Button>}
  </section>;
}

function validPage(value: unknown, holdId: string): value is NativeQualityCancellationPage {
  if (!value || typeof value !== "object") return false;
  const p = value as Record<string, unknown>;
  if (!Array.isArray(p.items) || p.items.length > 200 || !Number.isSafeInteger(p.headVersion) || (p.headVersion as number) < 1 || typeof p.historyRevision !== "string" || !/^qch1:[0-9a-f]{64}$/.test(p.historyRevision) || !(p.nextBefore === null || Number.isSafeInteger(p.nextBefore) && !Object.is(p.nextBefore, -0))) return false;
  let previous = Infinity;
  for (const row of p.items) {
    if (!row || typeof row !== "object" || row.holdId !== holdId || typeof row.id !== "string" || !row.id || !Number.isSafeInteger(row.version) || Object.is(row.version, -0) || row.version >= previous || !["request", "approve", "reject"].includes(row.action) || typeof row.reason !== "string" || typeof row.actorId !== "string" || !Number.isSafeInteger(row.createdAt)) return false;
    previous = row.version;
  }
  return p.nextBefore === null || (p.items.length > 0 && p.nextBefore === previous);
}
