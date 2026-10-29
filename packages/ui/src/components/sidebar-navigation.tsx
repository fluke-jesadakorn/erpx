"use client";

import { ChevronDown } from "lucide-react";
import { useEffect, useId, useState, type ReactNode } from "react";
import { Button } from "@erpx/ui/components/button";
import styles from "./sidebar-navigation.module.css";

export type SidebarTone = "brand" | "blue" | "violet" | "teal" | "green" | "amber";
export const sidebarItemClassName = styles.item;

export function SidebarIcon({ children, tone = "brand" }: { children: ReactNode; tone?: SidebarTone }) {
  return <span className={styles.icon} data-sidebar-icon data-tone={tone}>{children}</span>;
}

export function SidebarItemContent({ icon, children, tone, description }: {
  icon: ReactNode; children: ReactNode; tone?: SidebarTone; description?: string;
}) {
  return <><SidebarIcon tone={tone}>{icon}</SidebarIcon><span className={styles.label}>{children}{description && <small>{description}</small>}</span></>;
}

/** Presentation only. The caller retains its own links, permissions and guards. */
export function SidebarSection({ label, icon, tone, open, active = false, onToggle, children, layout = "sidebar" }: {
  label: string; icon: ReactNode; tone?: SidebarTone; open: boolean; active?: boolean;
  onToggle: () => void; children: ReactNode; layout?: "sidebar" | "tiles";
}) {
  const id = useId();
  return <section className={styles.group} data-active={active || undefined} data-layout={layout}>
    <Button appearance="standard" variant="ghost" className={styles.groupToggle} type="button"
      aria-expanded={open} aria-controls={id} onClick={onToggle}>
      <SidebarItemContent icon={icon} tone={tone}>{label}</SidebarItemContent>
      <ChevronDown className={styles.chevron} size={16} aria-hidden="true" />
    </Button>
    <div id={id} className={styles.children} hidden={!open}>{children}</div>
  </section>;
}

/** A saved closed state is distinct from a first visit. Storage is optional. */
export function sidebarInitialGroup(ids: readonly string[], active: string | null, saved: string | null) {
  if (active && ids.includes(active)) return active;
  if (saved === "") return null;
  return saved && ids.includes(saved) ? saved : ids[0] ?? null;
}

export function useSidebarAccordion({ ids, activeId = null, storageKey, locationKey = "" }: {
  ids: readonly string[]; activeId?: string | null; storageKey: string; locationKey?: string;
}) {
  const signature = ids.join("|");
  const [openId, setOpenId] = useState<string | null>(() => sidebarInitialGroup(ids, activeId, null));
  useEffect(() => {
    const frame = requestAnimationFrame(() => {
      let saved: string | null = null;
      try { saved = localStorage.getItem(`${storageKey}:${window.location.host}`); } catch { /* Storage is optional. */ }
      setOpenId(sidebarInitialGroup(signature ? signature.split("|") : [], activeId, saved));
    });
    return () => cancelAnimationFrame(frame);
  }, [signature, activeId, storageKey, locationKey]);
  function toggle(id: string) {
    const next = openId === id ? null : id;
    setOpenId(next);
    try { localStorage.setItem(`${storageKey}:${window.location.host}`, next ?? ""); } catch { /* Storage is optional. */ }
  }
  return { openId, toggle };
}
