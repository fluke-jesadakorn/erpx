"use client";

import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { ancestorPath, levelGroups, teamIds, type OrganizationGraph } from "@/lib/organization-chart";
import type { OrganizationOverview } from "@/lib/organization";
import { OrganizationPersonCard } from "./organization-person-card";
import styles from "./organization-chart.module.css";

export function OrganizationTeamView({ graph, data, focusId, branch, department, reveal, onFocus, onDetails }: {
  graph: OrganizationGraph; data: OrganizationOverview; focusId: string | null;
  branch: string; department: string; reveal: { id: string; request: number } | null;
  onFocus: (id: string | null) => void; onDetails: (id: string) => void;
}) {
  const root = useRef<HTMLDivElement>(null);
  const [appliedReveal, setAppliedReveal] = useState<object | null>(null);
  const [columns, setColumns] = useState(1);
  const [groupCounts, setGroupCounts] = useState<Record<string, number>>({});
  const [personCounts, setPersonCounts] = useState<Record<string, number>>({});
  const levels = useMemo(() => {
    const ids = teamIds(graph, focusId);
    for (const id of ids) {
      const p = graph.entries.get(id)!.person;
      if ((branch && (p.branch_id ?? "unassigned") !== branch) || (department && (p.department_id ?? "unassigned") !== department)) ids.delete(id);
    }
    return levelGroups(graph, data, ids);
  }, [graph, data, focusId, branch, department]);
  // Record search expansion as explicit counts so resizing and later collapse behave normally.
  if (reveal && appliedReveal !== reveal) {
    setAppliedReveal(reveal);
    for (const { level, groups } of levels) {
      const index = groups.findIndex(group => group.ids.includes(reveal.id));
      if (index < 0) continue;
      const scope = JSON.stringify([focusId, branch, department, level]);
      const group = groups[index], key = scope + group.key;
      setGroupCounts(old => ({ ...old, [scope]: Math.max(old[scope] ?? columns, index + 1) }));
      setPersonCounts(old => ({ ...old, [key]: Math.max(old[key] ?? 2, group.ids.indexOf(reveal.id) + 1) }));
    }
  }
  useEffect(() => {
    const element = root.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => setColumns(entry.contentRect.width >= 960 ? 3 : entry.contentRect.width >= 620 ? 2 : 1));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  useEffect(() => {
    if (!reveal) return;
    const button = [...(root.current?.querySelectorAll<HTMLElement>("[data-person-id]") ?? [])].find(el => el.dataset.personId === reveal.id)?.querySelector("button");
    button?.focus({ preventScroll: true });
    button?.scrollIntoView({ block: "center" });
  }, [reveal]);
  const path = focusId ? ancestorPath(graph, focusId) : [];
  return <div ref={root} className={styles.teamView} data-testid="organization-team-view">
    {focusId && <nav className={styles.breadcrumb} aria-label="Reporting path"><button onClick={() => onFocus(null)}>All teams</button>{path.map(id => <button key={id} onClick={() => onFocus(id)}>/ {graph.entries.get(id)!.person.legal_name}</button>)}<span aria-current="location">/ {graph.entries.get(focusId)?.person.legal_name}</span></nav>}
    {levels.map(({ level, title, groups }) => {
      if (level === 7 && !groups.length) return null;
      const scope = JSON.stringify([focusId, branch, department, level]);
      const count = Math.max(columns, groupCounts[scope] ?? 0);
      const remaining = Math.max(0, groups.length - count);
      return <section key={level} aria-label={level === 7 ? title : `Level ${level} · ${title}`}>
        <div className={styles.teamHeading}><h3>{level < 7 && <span className={styles.level}>Level {level}</span>} {title}</h3><span className={styles.totalBadge}>{groups.reduce((n, g) => n + g.ids.length, 0)} people</span></div>
        {!groups.length && <p className={styles.muted}>No staff in this level for the current view.</p>}
        <div className={styles.departmentGrid}>
          {groups.slice(0, count).map(group => {
            const key = scope + group.key;
            const shown = Math.max(2, personCounts[key] ?? 2);
            const more = Math.max(0, group.ids.length - shown);
            return <section key={group.key} className={styles.departmentGroup} aria-label={`${group.department} · ${group.branch}`} style={{ "--department-hue": group.hue, "--tint": `${[24, 21, 18, 14, 10, 6, 6][level - 1]}%` } as CSSProperties}>
              <h4>{group.department}<span>{group.branch} · {group.ids.length} people</span></h4>
              {group.ids.slice(0, shown).map(id => {
                const entry = graph.entries.get(id)!;
                const parent = graph.parents.get(id);
                return <OrganizationPersonCard key={id} entry={entry} department={group.department} branch={group.branch} manager={parent ? graph.entries.get(parent)!.person.legal_name : entry.boundary ?? (entry.isCeo && !entry.person.manager_person_id ? "Company leadership · no manager" : "Reporting relationship needs review")} reports={graph.children.get(id)?.length ?? 0} focused={id === reveal?.id} onTeam={() => onFocus(id)} onDetails={() => onDetails(id)}/>;
              })}
              {group.ids.length > 2 && <button className={styles.moreButton} aria-expanded={shown > 2} onClick={() => { setPersonCounts(old => ({ ...old, [key]: more ? shown + 4 : 2 })); }}>{more ? `Show more · ${more} remaining` : "Show less"}</button>}
              {more > 0 && shown > 2 && <button className={styles.moreButton} onClick={event => { const button = event.currentTarget.previousElementSibling as HTMLButtonElement; button?.focus(); setPersonCounts(old => ({ ...old, [key]: 2 })); }}>Show less</button>}
            </section>;
          })}
        </div>
        {groups.length > columns && <button className={styles.moreButton} aria-expanded={count > columns} onClick={() => { setGroupCounts(old => ({ ...old, [scope]: remaining ? count + columns : columns })); }}>{remaining ? `Show more departments · ${remaining} remaining` : "Show fewer departments"}</button>}
      </section>;
    })}
  </div>;
}
