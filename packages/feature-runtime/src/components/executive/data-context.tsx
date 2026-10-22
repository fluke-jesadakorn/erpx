"use client";
import { Button } from "@erpx/ui/components/foundation";

import { useState } from "react";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import styles from "@erpx/feature-runtime/components/executive/canvas.module.css";
import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";
import { type UiAppearance } from "@erpx/ui/components/foundation";
type ContextSummary = {
  scope: { companyId: string; branchId: string | null; departmentId: string | null; role: string };
  catalog: { label: string; breakdowns: string[] }[];
  modules: { name: string }[];
  organization: {
    available: boolean;
    nodes?: unknown[];
    positions?: unknown[];
    truncated?: boolean;
  };
  snapshots: unknown[];
  capturedAt: number;
  coverage: string;
};
export function AiDataContext({ appearance = "legacy", api }: ({ api: string }) & { appearance?: UiAppearance }) {
  const [context, setContext] = useState<ContextSummary | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  async function load() {
    setBusy(true);
    setError("");
    try {
      const r = await workFetch(`${api}/context`, { cache: "no-store" });
      const body = await r.json();
      if (!r.ok) throw Error(body.error?.message ?? "Could not inspect context.");
      setContext(body.data);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not inspect context.");
    } finally {
      setBusy(false);
    }
  }
  return (
    <details data-ui-appearance={appearance} className={styles.aiContext}>
      <summary data-ui-appearance={appearance}>AI data context · Current role</summary>
      <p data-ui-appearance={appearance}>
        Every generation reads current authorized organization structure and metric snapshots.
        Changing role never grants AI extra access.
      </p>
      <Button appearance={appearance} type="submit" variant="plain" disabled={busy} onClick={() => void load()}>
        {busy
          ? "Reading permitted sources…"
          : context
            ? "Refresh context"
            : "Inspect permitted sources"}
      </Button>
      {error && <p data-ui-appearance={appearance} role="alert">{error}</p>}
      {context && (
        <>
          <strong data-ui-appearance={appearance}>
            {context.scope.role.replaceAll("_", " ")} ·{" "}
            {context.scope.departmentId
              ? "Department scope"
              : context.scope.branchId
                ? "Branch scope"
                : "Company scope"}
          </strong>
          <p data-ui-appearance={appearance}>
            {context.catalog.length} metrics · {context.snapshots.length} groupings ·{" "}
            {context.organization.nodes?.length ?? 0} organization nodes ·{" "}
            {context.organization.positions?.length ?? 0} positions
          </p>
          <ul data-ui-appearance={appearance}>
            {context.catalog.map((m) => (
              <li data-ui-appearance={appearance} key={m.label}>
                {m.label} <small data-ui-appearance={appearance}>{m.breakdowns.join(", ")}</small>
              </li>
            ))}
          </ul>
          <p data-ui-appearance={appearance}>{context.coverage}</p>
          {context.organization.truncated && (
            <p data-ui-appearance={appearance}>Large structure: the first 500 nodes/positions are included; totals remain exact.</p>
          )}
          <small data-ui-appearance={appearance}>
            Read at <TimeText appearance={appearance} value={context.capturedAt} />
          </small>
        </>
      )}
    </details>
  );
}
