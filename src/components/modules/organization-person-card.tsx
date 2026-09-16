"use client";

import { ChevronDown, ChevronUp, Users, ArrowRight, AlertCircle } from "lucide-react";
import type { ChartEntry } from "@/lib/organization-chart";
import styles from "./organization-chart.module.css";

export type OrganizationPersonCardProps = {
  entry: ChartEntry;
  reports: number;
  focused?: boolean;
  compact?: boolean;
  expanded?: boolean;
  onDetails: () => void;
  onTeam: () => void;
  onToggle?: () => void;
};

export function OrganizationPersonCard({ entry, reports, focused, compact, expanded, onDetails, onTeam, onToggle }: OrganizationPersonCardProps) {
  const { person } = entry;
  const name = person.legal_name;
  const initials = name.trim().split(/\s+/).map(part => part[0]).join("").slice(0, 2);
  return <article className={[styles.personCard, focused ? styles.focusedCard : "", compact ? styles.canvasCard : ""].join(" ")} aria-label={name} data-person-id={person.engagement_id}>
    <button type="button" className={styles.cardDetails + " nodrag nopan"} aria-label={"View details for " + name} onClick={onDetails}>
      <span className={styles.identity}><span className={styles.avatar} aria-hidden="true">{initials}</span><span className={styles.name} title={name}>{name}</span></span>
      <span className={styles.jobTitle} title={entry.title}>{entry.title || "Position not assigned"}</span>
      <span className={styles.email} title={person.email || "No email"}>{person.email || "No email"}</span>
      <span className={styles.badges}>
        <span className={styles.level}>{person.level_number ? "Level " + person.level_number : "Level not mapped"}</span>
        {entry.issues.length > 0 && <span className={styles.issueBadge}><AlertCircle size={13} aria-hidden="true"/> Needs setup</span>}
      </span>
    </button>
    <div className={styles.cardFooter}>
      <span className={styles.reportCount}><Users size={15} aria-hidden="true"/>{reports} {reports === 1 ? "report" : "reports"}</span>
      <button type="button" className={styles.teamButton + " nodrag nopan"} onClick={onTeam} aria-label={"View team of " + name}>View team <ArrowRight size={15} aria-hidden="true"/></button>
      {onToggle && reports > 0 && <button type="button" className={styles.expandButton + " nodrag nopan"} onClick={onToggle} aria-expanded={expanded} aria-label={(expanded ? "Collapse team of " : "Expand team of ") + name}>{expanded ? <ChevronUp size={18} aria-hidden="true"/> : <ChevronDown size={18} aria-hidden="true"/>}</button>}
    </div>
  </article>;
}
