"use client";

import Link from "next/link";
import { ArrowUpRight, ChevronDown, LayoutGrid, type LucideIcon } from "lucide-react";
import { useId, useState, type CSSProperties } from "react";
import styles from "./module-tiles.module.css";

export type ModuleTile = {
  id: string;
  label: string;
  description: string;
  icon: LucideIcon;
  tone: "blue" | "teal" | "violet" | "amber";
  href?: string;
};

/** One navigation model for the hotel desk, restaurant and retail counter. */
export function ModuleTiles({ items, current, label, onSelect }: {
  items: ModuleTile[];
  current: string;
  label: string;
  onSelect?: (id: string) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const menuId = useId();
  const selected = items.find(item => item.id === current);
  const SelectedIcon = selected?.icon ?? LayoutGrid;
  return <div className={styles.menu} data-expanded={expanded}>
    <button className={styles.mobileToggle} type="button" aria-expanded={expanded} aria-controls={menuId} onClick={() => setExpanded(value => !value)}>
      <SelectedIcon size={22} aria-hidden="true"/><span><small>{label}</small><strong>{selected?.label ?? label}</strong></span><ChevronDown size={18} aria-hidden="true"/>
    </button>
    <nav id={menuId} className={styles.tiles} aria-label={label}>
    {items.map(({ id, label: title, description, icon: Icon, tone, href }) => {
      const content = <><span className={styles.top}><Icon size={25} strokeWidth={1.65} aria-hidden="true"/><ArrowUpRight className={styles.arrow} size={16} aria-hidden="true"/></span><span className={styles.title}>{title}</span><span className={styles.description}>{description}</span></>;
      const props = {
        className: styles.tile,
        "aria-label": title,
        "aria-current": current === id ? "page" as const : undefined,
        style: { "--tile-accent": `var(--${tone})`, "--tile-soft": `var(--${tone}-soft)` } as CSSProperties,
      };
      return href ? <Link key={id} href={href} onClick={() => setExpanded(false)} {...props}>{content}</Link> : <button key={id} type="button" onClick={() => { setExpanded(false); onSelect?.(id); }} {...props}>{content}</button>;
    })}
    </nav>
  </div>;
}
