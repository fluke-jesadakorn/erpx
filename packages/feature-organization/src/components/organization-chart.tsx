"use client";
import { Button, Field, Select, TextInput } from "@erpx/ui/components/foundation";

import { useId, useMemo, useRef, useState } from "react";
import { AlertCircle } from "lucide-react";
import type { OrganizationOverview } from "@erpx/feature-organization/lib/organization";
import { buildOrganizationGraph, searchOrganization } from "@erpx/feature-organization/lib/organization-chart";
import { OrganizationTeamView } from "@erpx/feature-organization/components/organization-team-view";
import { OrganizationPersonDetails } from "@erpx/feature-organization/components/organization-person-details";
import styles from "@erpx/feature-organization/components/organization-chart.module.css";

export function OrganizationChart({ base, data }: { base: string; data: OrganizationOverview }) {
  const graph = useMemo(() => buildOrganizationGraph(data), [data]);
  const [focusId, setFocusId] = useState<string | null>(null);
  const [detailId, setDetailId] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [branch, setBranch] = useState("");
  const [department, setDepartment] = useState("");
  const [reveal, setReveal] = useState<{ id: string; request: number } | null>(null);
  const searchId = useId();
  const content = useRef<HTMLDivElement>(null);
  const results = useMemo(
    () => (query.trim() ? searchOrganization(graph, query) : []),
    [graph, query],
  );
  function focusTeam(id: string | null) {
    setFocusId(id);
    setQuery("");
    setBranch("");
    setDepartment("");
    setReveal(null);
    requestAnimationFrame(() => {
      content.current?.focus();
      content.current?.scrollIntoView({ block: "start" });
    });
  }
  return (
    <section className={styles.chartShell} aria-label="Organization teams">
      <header className={styles.chartHeading}>
        <div>
          <h2>Teams</h2>
          <p>Browse staff by level, department and branch.</p>
        </div>
        <span className={styles.totalBadge}>{data.people.length} profiles</span>
      </header>
      <div className={styles.chartToolbar}>
        <div className={styles.searchBox}>
          <label htmlFor={searchId} className={styles.srOnly}>
            Find staff in organization
          </label>
          <TextInput
            id={searchId}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Name, position, level or team"
            autoComplete="off"
          />
          {query && (
            <Button
              type="submit"
              variant="plain"
              aria-label="Clear staff search"
              onClick={() => setQuery("")}
            >
              ×
            </Button>
          )}
        </div>
        <Field className={styles.filter}>
          Branch
          <Select
            value={branch}
            onChange={(e) => {
              setBranch(e.target.value);
              setDepartment("");
              setReveal(null);
            }}
          >
            <option value="">All branches</option>
            <option value="unassigned">Company-wide / unassigned</option>
            {data.nodes
              .filter((n) => n.kind === "branch")
              .map((n) => (
                <option key={n.id} value={n.id}>
                  {n.display_name}
                </option>
              ))}
          </Select>
        </Field>
        <Field className={styles.filter}>
          Department
          <Select
            value={department}
            onChange={(e) => {
              setDepartment(e.target.value);
              setReveal(null);
            }}
          >
            <option value="">All departments</option>
            <option value="unassigned">Unassigned</option>
            {data.nodes
              .filter(
                (n) =>
                  n.kind === "department" && (!branch || (n.branch_id ?? "unassigned") === branch),
              )
              .map((n) => (
                <option key={n.id} value={n.id}>
                  {n.display_name} ·{" "}
                  {data.nodes.find((b) => b.id === n.branch_id)?.display_name ?? "Company-wide"}
                </option>
              ))}
          </Select>
        </Field>
      </div>
      {query.trim() && (
        <section className={styles.searchResults} aria-label="Staff search results">
          <p className={styles.muted} role="status">
            {results.length} people found across all authorized teams
          </p>
          {!results.length && <p>No matching staff.</p>}
          {results.map((id) => {
            const entry = graph.entries.get(id)!;
            return (
              <Button
                type="submit"
                variant="plain"
                key={id}
                className={styles.searchResult}
                onClick={() => {
                  setFocusId(null);
                  setBranch("");
                  setDepartment("");
                  setQuery("");
                  setReveal((old) => ({ id, request: (old?.request ?? 0) + 1 }));
                }}
              >
                <strong>{entry.person.legal_name}</strong>
                <span>
                  {entry.title} · {entry.placement}
                </span>
              </Button>
            );
          })}
        </section>
      )}
      <div ref={content} tabIndex={-1} className={styles.chartContent}>
        <OrganizationTeamView
          graph={graph}
          data={data}
          focusId={focusId}
          branch={branch}
          department={department}
          reveal={reveal}
          onFocus={focusTeam}
          onDetails={setDetailId}
        />
      </div>
      {graph.attention.length > 0 && (
        <details className={styles.attention}>
          <summary>
            <AlertCircle size={16} aria-hidden="true" />
            {graph.attention.length} profiles need setup
          </summary>
          <p>Review these profiles. Every person remains available in search and Teams.</p>
          <div>
            {graph.attention.map((id) => (
              <Button variant="plain" type="button" key={id} onClick={() => setDetailId(id)}>
                {graph.entries.get(id)!.person.legal_name}
              </Button>
            ))}
          </div>
        </details>
      )}
      <OrganizationPersonDetails
        base={base}
        data={data}
        graph={graph}
        personId={detailId}
        onClose={() => setDetailId(null)}
        onTeam={focusTeam}
      />
    </section>
  );
}
