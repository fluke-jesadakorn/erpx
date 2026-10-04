"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button } from "@/components/ui/foundation";

import { useNavigationCounts } from "@/components/workplace/navigation-counts";
import { workFetch } from "@/lib/work-fetch";
import { lastWorkChange, scheduleWorkRefresh } from "@/lib/work-refresh";
import type { SelfServiceSeed, SelfServiceSummary } from "@/server/self-service";
import { ArrowUpRight, CircleAlert, RefreshCw } from "lucide-react";
import Link from "next/link";
import { Suspense, use, useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import styles from "./self-service.module.css";
export function MyDayCard({
  base,
  plan,
  enabled,
  initial,
}: {
  base: string;
  plan: ReactNode;
  enabled: boolean;
  initial?: Promise<SelfServiceSeed | null>;
}) {
  return (
    <section className={styles.myDay} id="my-day" aria-label="My day">
      <header>
        <span>01 / YOUR DAY</span>
        <h2>My day</h2>
      </header>
      {enabled && (
        <Suspense fallback={<InlineNotice tone="info" role="status">Loading your day…</InlineNotice>}>
          <SelfServiceDay base={base} initial={initial} />
        </Suspense>
      )}
      {plan}
    </section>
  );
}
export function SelfServiceHome({
  base,
  children,
  enabled,
  initial,
}: {
  base: string;
  children: ReactNode;
  enabled: boolean;
  initial?: Promise<SelfServiceSeed | null>;
}) {
  if (!enabled) return children;
  return <MyDayCard base={base} plan={children} enabled initial={initial} />;
}
function SelfServiceDay({
  base,
  initial,
}: {
  base: string;
  initial?: Promise<SelfServiceSeed | null>;
}) {
  const { publishWork } = useNavigationCounts();
  const seed = initial ? use(initial) : null;
  const [data, setData] = useState<SelfServiceSummary | null>(seed?.data ?? null),
    [error, setError] = useState("");
  const [workCount, setWorkCount] = useState<number | null>(seed?.workCount ?? null);
  const generation = useRef(0);
  const load = useCallback(
    async (signal?: AbortSignal) => {
      const current = ++generation.current;
      try {
        const response = await workFetch(`/api/v1${base}/self-service/summary`, {
          cache: "no-store",
          signal,
        });
        const body = await response.json();
        if (!response.ok) throw Error(body.error?.message ?? "My day is unavailable.");
        if (!signal?.aborted && current === generation.current) {
          setData(body.data);
          setError("");
          const count = response.headers.get("X-ERPX-Work-Count");
          setWorkCount(count === null || count === "unknown" ? null : Number(count));
        }
      } catch (e) {
        if (!signal?.aborted && current === generation.current)
          setError(e instanceof Error ? e.message : "My day is unavailable.");
      }
    },
    [base],
  );
  // Reuse streamed data; refresh only if it was invalidated or has aged out.
  useEffect(() => {
    const controller = new AbortController();
    const timer =
      !seed || Date.now() - seed.data.asOf > 30_000 || seed.data.asOf < lastWorkChange()
        ? setTimeout(() => void load(controller.signal), 0)
        : undefined;
    const stop = scheduleWorkRefresh(() => void load(controller.signal));
    return () => {
      clearTimeout(timer);
      controller.abort();
      stop();
    };
  }, [seed, load]);
  useEffect(() => {
    if (!data) return;
    const timer = setTimeout(() => publishWork(workCount), 0);
    return () => clearTimeout(timer);
  }, [data, workCount, publishWork]);
  return (
    <div className={styles.myDayBody}>
      {error ? (
        <InlineNotice tone="danger" role="alert">
          {error}{" "}
          <Button appearance="standard" type="submit" variant="secondary" onClick={() => void load()}>
            Try again
          </Button>
        </InlineNotice>
      ) : !data ? (
        <InlineNotice tone="info" role="status">Loading your day…</InlineNotice>
      ) : (
        <>
          {data.rosterRequired && (
            <div className={styles.daySchedule}>
              <span>UPCOMING SHIFT</span>
              {data.staffSetupRequired ? (
                <p>Connect a staff profile to see your schedule.</p>
              ) : data.unavailableSources.includes("Schedule") ? (
                <p>Schedule could not refresh.</p>
              ) : data.nextShift ? (
                <Link className={styles.shift} href={`${base}/people/roster?view=mine`}>
                  <strong>{data.nextShift.name}</strong>
                  <span>
                    {data.nextShift.local_date} · {data.nextShift.start_time}–
                    {data.nextShift.end_time}
                    {data.nextShift.end_time < data.nextShift.start_time ? " (+1 day)" : ""}
                  </span>
                  <small>{data.nextShift.timezone}</small>
                </Link>
              ) : (
                <p>No upcoming published shift.</p>
              )}
            </div>
          )}
          <div className={styles.dayLinks}>
            <Link href={`${base}/calendar`}>
              Open Calendar <ArrowUpRight size={13} />
            </Link>
            <Button appearance="standard"
              variant="ghost"
              type="button"
              aria-controls="plan"
              onClick={() => {
                window.dispatchEvent(new Event("erpx:open-plan"));
                document
                  .getElementById("plan")
                  ?.scrollIntoView({ block: "start", behavior: "smooth" });
              }}
            >
              Plan my day <ArrowUpRight size={13} />
            </Button>
          </div>
          {!!data.setupBlockers.length && (
            <details className={styles.setup}>
              <summary>
                <CircleAlert size={18} aria-hidden="true" />
                <span>
                  <small>ACCOUNT SETUP</small>
                  <strong>
                    {data.staffSetupRequired ? "Connect your staff profile" : "Complete your setup"}
                  </strong>
                </span>
              </summary>
              <div className={styles.setupContent}>
                <ul>
                  {data.setupBlockers.map((b) => (
                    <li key={b}>{b}</li>
                  ))}
                </ul>
                <Link className={styles.setupAction} href={`${base}/people/settings`}>
                  Open People settings <ArrowUpRight size={13} />
                </Link>
              </div>
            </details>
          )}
          {!!data.unavailableSources.length && (
            <section className={styles.unavailable} role="status">
              <div>
                <RefreshCw size={16} aria-hidden="true" />
                <span>
                  <strong>Some information could not refresh</strong>
                  <small>{data.unavailableSources.join(" · ")}</small>
                </span>
              </div>
              <Button appearance="standard" type="submit" variant="secondary" onClick={() => void load()}>
                Try again
              </Button>
            </section>
          )}
        </>
      )}
    </div>
  );
}
