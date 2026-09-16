"use client";

import { ArrowUp, Network } from "lucide-react";
import { ancestorPath, type OrganizationGraph } from "@/lib/organization-chart";
import { OrganizationPersonCard } from "./organization-person-card";
import styles from "./organization-chart.module.css";

export function OrganizationTeamView({ graph, focusId, onFocus, onDetails }: {
  graph: OrganizationGraph;
  focusId: string | null;
  onFocus: (id: string) => void;
  onDetails: (id: string) => void;
}) {
  const entry = focusId ? graph.entries.get(focusId) : null;
  const parentId = focusId ? graph.parents.get(focusId) : null;
  const reports = focusId ? graph.children.get(focusId) ?? [] : [];
  const path = focusId ? ancestorPath(graph, focusId) : [];
  function card(id: string, focused = false) {
    return <OrganizationPersonCard key={id} entry={graph.entries.get(id)!} reports={graph.children.get(id)?.length ?? 0} focused={focused} onTeam={() => onFocus(id)} onDetails={() => onDetails(id)}/>;
  }
  if (!entry || !focusId) return <div className={styles.teamView} data-testid="organization-team-view">
    <div className={styles.teamHeading}><Network size={20} aria-hidden="true"/><h3>Choose a starting team</h3></div>
    <p className={styles.muted}>These are the starting points available in your workspace.</p>
    <div className={styles.teamGrid}>{graph.roots.map(id => card(id))}</div>
  </div>;
  return <div className={styles.teamView} data-testid="organization-team-view">
    {path.length > 0 && <nav className={styles.breadcrumb} aria-label="Reporting path">
      {path.map(id => <button type="button" key={id} onClick={() => onFocus(id)}>{graph.entries.get(id)!.person.legal_name}<span aria-hidden="true">/</span></button>)}
      <span aria-current="location">{entry.person.legal_name}</span>
    </nav>}
    {parentId && <section className={styles.managerBlock} aria-label="Reporting manager">
      <button type="button" className={styles.backButton} onClick={() => onFocus(parentId)}><ArrowUp size={16} aria-hidden="true"/>Up to {graph.entries.get(parentId)!.person.legal_name}&apos;s team</button>
      {card(parentId)}
    </section>}
    <section className={styles.focusBlock} aria-label="Selected team">
      <span className={styles.eyebrow}>Selected team</span>
      {card(focusId, true)}
      {entry.boundary && <p className={styles.muted}>{entry.boundary}</p>}
    </section>
    <section aria-label="Direct reports">
      <div className={styles.teamHeading}><h3>Direct reports</h3><span className={styles.totalBadge}>{reports.length}</span></div>
      {reports.length ? <div className={styles.teamGrid}>{reports.map(id => card(id))}</div> : <p className={styles.empty}>No direct reports in this view.</p>}
    </section>
  </div>;
}
