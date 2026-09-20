"use client";
import Link from "next/link";
import { useCallback, useEffect, useState } from "react";
import type { CalendarItem } from "@/lib/calendar";
import { today } from "@/lib/calendar-date";
import { workFetch } from "@/lib/work-fetch";
import styles from "./calendar.module.css";
export function AnnouncementFeed({ base }: { base: string }) {
  const [items, setItems] = useState<CalendarItem[]>([]),
    [error, setError] = useState("");
  const refresh = useCallback(async () => {
    try {
      const r = await workFetch(
          `/api/v1${base}/calendar/announcements?date=${today()}&view=agenda&published=true`,
          { cache: "no-store" },
        ),
        j = await r.json();
      if (
        !r.ok ||
        j.data.sources?.some((s: { status: string }) => s.status === "error")
      )
        throw new Error("Announcements could not be loaded.");
      setItems(
        j.data.items.filter(
          (i: CalendarItem) =>
            i.status === "published" &&
            i.startDate! <= today() &&
            i.endDate! > today(),
        ),
      );
      setError("");
    } catch (e) {
      setError(
        e instanceof Error ? e.message : "Announcements could not be loaded.",
      );
    }
  }, [base]);
  useEffect(() => {
    const timer = window.setTimeout(() => void refresh(), 0);
    window.addEventListener("focus", refresh);
    return () => {
      window.clearTimeout(timer);
      window.removeEventListener("focus", refresh);
    };
  }, [refresh]);
  return (
    <>
      {error && (
        <p role="alert">
          {error} <button onClick={() => void refresh()}>Retry</button>
        </p>
      )}
      {items.map((i) => (
        <article key={i.id} className={styles.feedNotice}>
          <small>ANNOUNCEMENT · {i.owner}</small>
          <h3>{i.title}</h3>
          <p>{i.description}</p>
          <Link
            href={`${base}/calendar?date=${i.startDate}&layers=announcements&event=${encodeURIComponent(i.id)}`}
          >
            View in Calendar
          </Link>
        </article>
      ))}
    </>
  );
}
