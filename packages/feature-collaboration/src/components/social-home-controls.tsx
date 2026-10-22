"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { useEffect, useState, type ReactNode } from "react";
import Link from "@erpx/workplace/components/workspace-link";
import { Button, InlineNotice } from "@erpx/ui/components/foundation";
import { useNavigationCounts } from "@erpx/workplace/components/navigation-counts";
import { WorkAttention } from "@erpx/workplace/components/work-attention";
import styles from "@erpx/feature-collaboration/components/social.module.css";

export function TodayTools({ summary, children }: { summary: ReactNode; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  useEffect(() => {
    const reveal = () => { if (window.location.hash === "#plan") setOpen(true); };
    const event = () => setOpen(true);
    const timer = setTimeout(reveal, 0);
    window.addEventListener("hashchange", reveal); window.addEventListener("erpx:open-plan", event);
    return () => { clearTimeout(timer); window.removeEventListener("hashchange", reveal); window.removeEventListener("erpx:open-plan", event); };
  }, []);
  return <>
    <div className={styles.mobileSummary} aria-label="Today at a glance">
      {summary}<button id="today-tools-toggle" type="button"
        aria-expanded={open} aria-controls="today-tools" onClick={() => {
          setOpen(true); window.dispatchEvent(new Event("erpx:open-plan"));
        }}>Plan my day</button>
    </div>
    <aside className={styles.todayRail} aria-label="Today tools" id="today-tools" data-open={open} tabIndex={-1}>
      <Button appearance="standard" variant="ghost" type="button" className={styles.closeTools} onClick={() => {
        setOpen(false); requestAnimationFrame(() => document.getElementById("today-tools-toggle")?.focus());
      }}>Close today&apos;s tools</Button>
      {children}
    </aside>
  </>;
}

export function HomePriorities({ base }: { base: string }) {
  const { attention, updating, snapshotAt, error, refresh } = useNavigationCounts();
  return <WorkAttention base={base} attention={{ ...attention, items: attention.items.slice(0, 2), incomplete: attention.incomplete || error }}
    compact rail loading={updating && !snapshotAt} updating={updating && !!snapshotAt}
    snapshotAt={snapshotAt} retry={() => void refresh()} />;
}
export function MobileAttentionCount() {
  const { attention, updating, snapshotAt, error } = useNavigationCounts();
  if (!snapshotAt) return updating ? "…" : "—";
  return `${attention.total}${attention.incomplete || error ? "+" : ""}`;
}
export function MobileAttentionNotice({ base }: { base: string }) {
  const { attention, error } = useNavigationCounts();
  return attention.incomplete || error ? <InlineNotice tone="info" className={styles.mobileNotice} role="status">
    Some work is missing or out of date. <Link href={canonicalUiHref(`${base}/work`)}>Check My work</Link>
  </InlineNotice> : null;
}
