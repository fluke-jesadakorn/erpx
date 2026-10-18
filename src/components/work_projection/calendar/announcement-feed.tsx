"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { InlineNotice } from "@/components/ui/foundation";

import { Button } from "@/components/ui/foundation";

import Link from "@/components/workplace/workspace-link";
import { useCallback, useEffect, useState } from "react";
import type { CalendarItem } from "@/lib/work_projection/calendar";
import { today } from "@/lib/work_projection/calendar-date";
import { workFetch } from "@/lib/work_projection/work-fetch";
import styles from "@/components/work_projection/calendar/calendar.module.css";
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
      if (!r.ok || j.data.sources?.some((s: { status: string }) => s.status === "error"))
        throw new Error("Announcements could not be loaded.");
      setItems(
        j.data.items.filter(
          (i: CalendarItem) =>
            i.status === "published" && i.startDate! <= today() && i.endDate! > today(),
        ),
      );
      setError("");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Announcements could not be loaded.");
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
        <InlineNotice tone="danger" role="alert">
          {error}{" "}
          <Button appearance="standard" type="submit" variant="ghost" onClick={() => void refresh()}>
            Retry
          </Button>
        </InlineNotice>
      )}
      {items.map((i) => (
        <article key={i.id} className={styles.feedNotice}>
          <small>ANNOUNCEMENT · {i.owner}</small>
          <h3>{i.title}</h3>
          <p>{i.description}</p>
          <Link
            href={canonicalUiHref(`${base}/calendar?date=${i.startDate}&layers=announcements&event=${encodeURIComponent(i.id)}`)}
          >
            View in Calendar
          </Link>
        </article>
      ))}
    </>
  );
}
