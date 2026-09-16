"use client";

import dynamic from "next/dynamic";
import { Component, useId, useMemo, useRef, useState, useSyncExternalStore, type ReactNode } from "react";
import { Network, Users, Search, X, AlertCircle } from "lucide-react";
import { Dialog } from "@/components/ui/dialog";
import type { OrganizationOverview } from "@/lib/organization";
import { ancestorPath, buildOrganizationGraph, searchOrganization } from "@/lib/organization-chart";
import { OrganizationTeamView } from "./organization-team-view";
import { OrganizationPersonDetails } from "./organization-person-details";
import styles from "./organization-chart.module.css";

const OrganizationOverviewChart = dynamic(() => import("./organization-chart-overview"), {
  ssr: false,
  loading: () => <p className={styles.loading} role="status">Loading organization chart…</p>,
});
function subscribeViewport(callback: () => void) {
  const media = window.matchMedia("(min-width: 1024px)");
  media.addEventListener("change", callback);
  return () => media.removeEventListener("change", callback);
}
function desktopSnapshot() { return window.matchMedia("(min-width: 1024px)").matches; }
function serverSnapshot() { return false; }

class ChartBoundary extends Component<{ children: ReactNode; fallback: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  render() { return this.state.failed ? this.props.fallback : this.props.children; }
}

export function OrganizationChart({ base, data }: { base: string; data: OrganizationOverview }) {
  const graph = useMemo(() => buildOrganizationGraph(data), [data]);
  const desktop = useSyncExternalStore(subscribeViewport, desktopSnapshot, serverSnapshot);
  const [mode, setMode] = useState<"team" | "overview" | null>(null);
  const activeMode = mode ?? (desktop ? "overview" : "team");
  const fullscreen = !desktop && activeMode === "overview";
  const [focusId, setFocusId] = useState<string | null>(() => graph.roots.length === 1 ? graph.roots[0] : null);
  const [detailId, setDetailId] = useState<string | null>(null);
  const [expanded, setExpanded] = useState(() => new Set(graph.roots));
  const [focusRequest, setFocusRequest] = useState(0);
  const [query, setQuery] = useState("");
  const searchId = useId();
  const searchRef = useRef<HTMLInputElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const results = useMemo(() => query.trim() ? searchOrganization(graph, query) : [], [graph, query]);
  const currentFocus = focusId && graph.entries.has(focusId) ? focusId : graph.roots.length === 1 ? graph.roots[0] : null;

  function focusPerson(id: string, showTeam = false) {
    setFocusId(id);
    setExpanded(previous => new Set([...previous, ...ancestorPath(graph, id), id]));
    setFocusRequest(value => value + 1);
    setQuery("");
    if (showTeam) setMode("team");
    requestAnimationFrame(() => {
      contentRef.current?.focus({ preventScroll: true });
      if (showTeam || activeMode === "team") contentRef.current?.scrollIntoView({ block: "start", behavior: "instant" });
    });
  }
  function toggleTeam(id: string) {
    if (expanded.has(id) && currentFocus && ancestorPath(graph, currentFocus).includes(id)) setFocusId(id);
    setExpanded(previous => {
      const next = new Set(previous);
      if (next.has(id)) {
        next.delete(id);
      } else next.add(id);
      return next;
    });
  }
  function collapseAll() {
    setExpanded(new Set(graph.roots));
    setFocusId(graph.roots.length === 1 ? graph.roots[0] : null);
  }
  const teamView = <OrganizationTeamView graph={graph} focusId={currentFocus} onFocus={id => focusPerson(id, true)} onDetails={setDetailId}/>;
  const content = <>
    <div className={styles.chartToolbar}>
      <div className={styles.modeSwitch} role="group" aria-label="Chart display">
        <button type="button" aria-pressed={activeMode === "team"} onClick={() => setMode("team")}><Users size={16} aria-hidden="true"/>{fullscreen ? "Back to team view" : "Team view"}</button>
        <button type="button" aria-pressed={activeMode === "overview"} onClick={() => setMode("overview")}><Network size={16} aria-hidden="true"/>Overview</button>
      </div>
      <div className={styles.searchBox}>
        <label htmlFor={searchId} className={styles.srOnly}>Find staff in organization</label>
        <Search size={17} aria-hidden="true"/>
        <input ref={searchRef} id={searchId} value={query} onChange={event => setQuery(event.target.value)} placeholder="Name, position, level or team" autoComplete="off"/>
        {query && <button type="button" aria-label="Clear staff search" onClick={() => { setQuery(""); searchRef.current?.focus(); }}><X size={16} aria-hidden="true"/></button>}
      </div>
    </div>
    <div ref={contentRef} tabIndex={-1} className={styles.chartContent}>
      {query.trim() ? <section className={styles.searchResults} aria-label="Staff search results">
        <p className={styles.muted} role="status">{results.length} {results.length === 1 ? "person" : "people"} found in this view</p>
        {results.length === 0 && <p className={styles.empty}>No matching staff. Try another name, position or team.</p>}
        {results.map(id => {
          const entry = graph.entries.get(id)!;
          return <button key={id} type="button" className={styles.searchResult} onClick={() => focusPerson(id)}>
            <strong>{entry.person.legal_name}</strong><span>{entry.title} · {entry.person.level_number ? "Level " + entry.person.level_number : "Level not mapped"}</span><span>{entry.placement}</span>
          </button>;
        })}
      </section> : activeMode === "team" ? teamView : <ChartBoundary fallback={<div className={styles.empty}><p>The overview could not load. You can still browse every team.</p><button type="button" className="button secondary" onClick={() => setMode("team")}>Use team view</button></div>}>
        <OrganizationOverviewChart graph={graph} expanded={expanded} focusId={currentFocus} focusRequest={focusRequest} onDetails={setDetailId} onTeam={id => focusPerson(id, true)} onToggle={toggleTeam} onExpandAll={() => setExpanded(new Set(graph.entries.keys()))} onCollapseAll={collapseAll}/>
      </ChartBoundary>}
    </div>
    {!query.trim() && activeMode === "team" && graph.roots.length > 1 && currentFocus && <button type="button" className={styles.rootsButton} onClick={() => setFocusId(null)}>Choose another starting team</button>}
  </>;
  return <section className={styles.chartShell} aria-label="Organization reporting chart">
    <header className={styles.chartHeading}><div><h2>Who reports to whom</h2><p>Explore a team or see how the organization connects.</p></div><span className={styles.totalBadge}>{data.people.length} profiles</span></header>
    {data.people.length === 0 ? <p className={styles.empty}>Add staff in the People directory to build your reporting chart.</p> : <>
      {!fullscreen ? content : <div className={styles.empty}><p>Organization overview is open.</p><button type="button" className="button secondary" onClick={() => setMode("team")}>Back to team view</button></div>}
      <Dialog open={fullscreen} onClose={() => setMode("team")} title="Organization overview" presentation="fullscreen">{fullscreen && <div className={styles.fullscreenContent}>{content}</div>}</Dialog>
      {graph.attention.length > 0 && <details className={styles.attention}><summary><AlertCircle size={16} aria-hidden="true"/>{graph.attention.length} profiles need setup</summary><p>Review these profiles. Every person remains available in search and team view.</p><div>{graph.attention.map(id => <button type="button" key={id} onClick={() => setDetailId(id)}>{graph.entries.get(id)!.person.legal_name}</button>)}</div></details>}
      <OrganizationPersonDetails base={base} data={data} graph={graph} personId={detailId} onClose={() => setDetailId(null)} onTeam={id => focusPerson(id, true)}/>
    </>}
  </section>;
}
