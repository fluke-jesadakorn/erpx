"use client";
import { Button } from "@erpx/ui/components/foundation";

import { Users, ArrowRight, AlertCircle } from "lucide-react";
import { assignedLevel, type ChartEntry } from "@erpx/feature-organization/lib/organization-chart";
import styles from "@erpx/feature-organization/components/organization-chart.module.css";

export function OrganizationPersonCard({
  entry,
  reports,
  focused,
  department,
  branch,
  manager,
  onDetails,
  onTeam,
}: {
  entry: ChartEntry;
  reports: number;
  focused?: boolean;
  department: string;
  branch: string;
  manager: string;
  onDetails: () => void;
  onTeam: () => void;
}) {
  const { person } = entry;
  const name = person.legal_name;
  const initials = name
    .trim()
    .split(/\s+/)
    .map((part) => part[0])
    .join("")
    .slice(0, 2);
  return (
    <article
      className={[styles.personCard, focused ? styles.focusedCard : ""].join(" ")}
      aria-label={name}
      data-person-id={person.engagement_id}
    >
      <Button
        variant="plain"
        type="button"
        className={styles.cardDetails}
        aria-label={"View details for " + name}
        onClick={onDetails}
      >
        <span className={styles.identity}>
          <span className={styles.avatar} aria-hidden="true">
            {initials}
          </span>
          <span className={styles.name}>{name}</span>
        </span>
        <span className={styles.jobTitle}>{entry.title || "Position not assigned"}</span>
        <span className={styles.cardFact}>{department}</span>
        <span className={styles.cardFact}>{branch}</span>
        <span className={styles.badges}>
          <span className={styles.level}>
            {assignedLevel(person.level_number) < 7
              ? "Level " + person.level_number
              : "Level not assigned"}
          </span>
          {entry.issues.length > 0 && (
            <span className={styles.issueBadge}>
              <AlertCircle size={13} aria-hidden="true" /> Needs setup
            </span>
          )}
        </span>
        <span className={styles.cardFact}>Reports to: {manager}</span>
      </Button>
      <div className={styles.cardFooter}>
        <span className={styles.reportCount}>
          <Users size={15} aria-hidden="true" />
          {reports} direct {reports === 1 ? "report" : "reports"}
        </span>
        <Button
          variant="plain"
          type="button"
          className={styles.teamButton}
          onClick={onTeam}
          aria-label={"View team of " + name}
        >
          View team <ArrowRight size={15} aria-hidden="true" />
        </Button>
      </div>
    </article>
  );
}
