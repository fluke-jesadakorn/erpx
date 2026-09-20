"use client";
import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { Dialog } from "@/components/ui/dialog";
import { workFetch } from "@/lib/work-fetch";
import {
  provisioningInput,
  type ProvisioningInput,
  type ProvisioningPreview,
  type TechnologyData,
} from "@/lib/technology";
import type { PeopleLifecycleData } from "@/lib/people-lifecycle";
import styles from "./technology.module.css";
export function ProvisioningWorkspace({
  base,
  lifecycle,
  data,
  canApply,
}: {
  base: string;
  lifecycle: PeopleLifecycleData;
  data: TechnologyData;
  canApply: boolean;
}) {
  const router = useRouter(),
    receipt = useRef<{ body: string; key: string } | null>(null);
  const [selected, setSelected] = useState<string[]>([]),
    [open, setOpen] = useState(false),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [preview, setPreview] = useState<ProvisioningPreview | null>(null),
    [input, setInput] = useState<ProvisioningInput | null>(null),
    [detail, setDetail] = useState<string | null>(null),
    [source, setSource] = useState<"position" | "supplemental">("position"),
    [withdraw, setWithdraw] = useState<
      import("@/lib/technology").AppliedPackage | null
    >(null);
  const pending = data.operations.some((op) =>
    ["queued", "running", "retry"].includes(op.status),
  );
  useEffect(() => {
    if (!pending) return;
    const timer = setInterval(() => router.refresh(), 5000);
    return () => clearInterval(timer);
  }, [pending, router]);
  async function send(path: string, body: unknown) {
    setBusy(true);
    setError("");
    const encoded = JSON.stringify(body),
      fingerprint = path + encoded;
    if (receipt.current?.body !== fingerprint)
      receipt.current = { body: fingerprint, key: crypto.randomUUID() };
    try {
      const r = await workFetch(
          "/api/v1" + base + "/technology/provisioning/" + path,
          {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
              "Idempotency-Key": receipt.current.key,
            },
            body: encoded,
          },
        ),
        result = await r.json();
      if (!r.ok)
        throw Error(result.error?.message ?? "Unable to provision access.");
      receipt.current = null;
      return result.data;
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unable to continue.");
    } finally {
      setBusy(false);
    }
  }
  function status(id: string, positionId: string | null) {
    const position = lifecycle.positions.find((p) => p.id === positionId),
      applied = data.applications.filter(
        (a) =>
          a.engagement_id === id && a.active && a.source_kind === "position",
      );
    if (
      data.operations.some(
        (op) =>
          op.status === "blocked" &&
          data.applications.some(
            (a) => a.engagement_id === id && a.id.startsWith(op.id + ":"),
          ),
      )
    )
      return "Needs attention";
    if (
      data.operations.some(
        (op) =>
          (["queued", "running", "retry"].includes(op.status) ||
            op.reviewsPending > 0) &&
          data.applications.some(
            (a) => a.engagement_id === id && a.id.startsWith(op.id + ":"),
          ),
      )
    )
      return "In progress";
    return applied.some(
      (a) =>
        a.position_id === positionId &&
        a.package_id === position?.package_id &&
        a.package_revision === position?.package_revision,
    )
      ? "Ready"
      : "Waiting for IT";
  }
  const person = lifecycle.roster.find((p) => p.engagementId === detail);
  return (
    <div className={styles.stack}>
      <div className={styles.toolbar}>
        <div>
          <h2>Employee provisioning</h2>
          <p className="muted">
            Compare applied access with the current position, then select up to
            100 employees to update.
          </p>
        </div>
        {canApply && (
          <button
            className="button"
            disabled={!selected.length}
            onClick={() => {
              setPreview(null);
              setWithdraw(null);
              setOpen(true);
            }}
          >
            Prepare {selected.length || ""} selected
          </button>
        )}
      </div>
      {notice && <p role="status">{notice}</p>}
      <div style={{ overflowX: "auto" }}>
        <table className={styles.employeeTable}>
          <thead>
            <tr>
              <th>
                <input
                  aria-label="Select the first 100 active employees"
                  type="checkbox"
                  disabled={!canApply}
                  checked={
                    !!lifecycle.roster.length &&
                    lifecycle.roster
                      .filter((p) => p.status === "active")
                      .slice(0, 100)
                      .every((p) => selected.includes(p.engagementId))
                  }
                  onChange={(e) =>
                    setSelected(
                      e.target.checked
                        ? lifecycle.roster
                            .filter((p) => p.status === "active")
                            .slice(0, 100)
                            .map((p) => p.engagementId)
                        : [],
                    )
                  }
                />
              </th>
              <th>Employee</th>
              <th>HR position</th>
              <th>IT status</th>
              <th>Applied revision</th>
            </tr>
          </thead>
          <tbody>
            {lifecycle.roster.map((p) => (
              <tr key={p.engagementId}>
                <td>
                  <input
                    type="checkbox"
                    aria-label={`Select ${p.name}`}
                    disabled={
                      !canApply ||
                      p.status !== "active" ||
                      (selected.length >= 100 &&
                        !selected.includes(p.engagementId))
                    }
                    checked={selected.includes(p.engagementId)}
                    onChange={(e) =>
                      setSelected(
                        e.target.checked
                          ? [...selected, p.engagementId]
                          : selected.filter((id) => id !== p.engagementId),
                      )
                    }
                  />
                </td>
                <td>
                  <button
                    className="button secondary"
                    onClick={() => setDetail(p.engagementId)}
                  >
                    {p.name}
                  </button>
                </td>
                <td>
                  {lifecycle.positions.find((x) => x.id === p.positionId)
                    ?.title ?? "No position"}
                </td>
                <td>
                  <span className={styles.state}>
                    {status(p.engagementId, p.positionId)}
                  </span>
                </td>
                <td>
                  {data.applications
                    .filter(
                      (a) => a.engagement_id === p.engagementId && a.active,
                    )
                    .map((a) => (
                      <div key={a.id}>
                        {a.definition.name} · r{a.package_revision} ·{" "}
                        {a.source_kind}
                      </div>
                    ))}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <details>
        <summary>Provisioning operations</summary>
        {data.operations.map((op) => (
          <article className={styles.preview} key={op.id}>
            <strong>{op.status.replaceAll("_", " ")}</strong> ·{" "}
            {new Date(op.created_at).toLocaleString()}
            <p>{op.reviewsPending} ERPX role reviews pending</p>
            {op.steps.map((s) => (
              <p key={s.id}>
                {s.label} · {s.status}
              </p>
            ))}
            {op.last_error && <p role="status">{op.last_error}</p>}
            {canApply &&
              ["FRESH_MFA_REQUIRED", "PROVISIONING_RETRY_REQUIRED"].includes(
                op.last_error ?? "",
              ) && (
                <button
                  className="button secondary"
                  disabled={busy}
                  onClick={async () => {
                    if (
                      await send("retry", {
                        id: op.id,
                        expectedVersion: op.version,
                      })
                    )
                      router.refresh();
                  }}
                >
                  Retry unfinished operation
                </button>
              )}
            {op.steps.length > 0 && (
              <Link href={base + "/technology?view=directory"}>
                Inspect directory steps and retry
              </Link>
            )}
            <Link href={base + "/people/settings?section=reviews"}>
              Review ERPX role requests
            </Link>
          </article>
        ))}
      </details>
      <Dialog
        open={open}
        onClose={() => setOpen(false)}
        title="Prepare employee resources"
        presentation="responsive-panel"
      >
        <form
          className={styles.form}
          onChange={() => {
            setPreview(null);
            setInput(null);
          }}
          onSubmit={async (e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget),
              pkg = lifecycle.packages.find(
                (p) => p.id === String(f.get("packageId")),
              ),
              parsed = provisioningInput.safeParse({
                engagementIds: selected,
                sourceKind: withdraw?.source_kind ?? source,
                packageId: withdraw?.package_id ?? pkg?.id ?? null,
                revision:
                  withdraw?.package_revision ?? pkg?.published_revision ?? null,
                revokeOnly: !!withdraw,
                revokeApplicationIds: withdraw ? [withdraw.id] : [],
                restoreAccess: f.has("restoreAccess"),
                exclude: String(f.get("exclude") ?? "")
                  .split("\n")
                  .map((s) => s.trim())
                  .filter(Boolean),
                validUntil: withdraw
                  ? Math.max(withdraw.valid_until, Date.now() + 60000)
                  : new Date(String(f.get("expires"))).getTime(),
                reason: String(f.get("reason")),
              });
            if (!parsed.success) {
              setError(parsed.error.issues[0]?.message ?? "Check the request.");
              return;
            }
            const result = await send("preview", parsed.data);
            if (result) {
              setInput(parsed.data);
              setPreview(result);
            }
          }}
        >
          <p>
            {withdraw
              ? `Withdraw applied source: ${withdraw.definition.name} · r${withdraw.package_revision}`
              : `${selected.length} employees selected. HR placement is already effective.`}
          </p>
          {!withdraw && (
            <>
              <label>
                Access source
                <select
                  value={source}
                  onChange={(e) => setSource(e.target.value as typeof source)}
                >
                  <option value="position">
                    Replace applied position package
                  </option>
                  <option value="supplemental">
                    Additional access for these people
                  </option>
                </select>
              </label>
              <label>
                Access package
                <select name="packageId" required={source === "supplemental"}>
                  <option value="">Use each employee’s position package</option>
                  {lifecycle.packages
                    .filter((p) => p.published_revision)
                    .map((p) => (
                      <option key={p.id} value={p.id}>
                        {p.definition.name} · r{p.published_revision}
                      </option>
                    ))}
                </select>
              </label>
              <label>
                Access expires
                <input type="datetime-local" name="expires" required />
              </label>
              <label>
                <input type="checkbox" name="restoreAccess" /> Restore access
                after a completed HR return (old grants stay revoked)
              </label>
              <details>
                <summary>Exceptions</summary>
                <label>
                  Exclude resource keys (one per line)
                  <textarea
                    name="exclude"
                    placeholder={
                      "role:role-id\ngroup:group-id\ndirectory:group-id\ngpo:gpo-id"
                    }
                  />
                </label>
              </details>
            </>
          )}
          <label>
            Reason
            <textarea name="reason" required minLength={3} maxLength={1000} />
          </label>
          <button className="button secondary" disabled={busy}>
            Preview changes
          </button>
          {preview && input && (
            <div className={styles.preview}>
              {preview.employees.map((p) => (
                <section key={p.id}>
                  <h3>{p.name}</h3>
                  <p>
                    {p.packageName} · revision {p.revision}
                  </p>
                  <p>Add: {p.added.join(", ") || "No new grants"}</p>
                  <p>Replace source: {p.removed.join(", ") || "None"}</p>
                  <p>
                    Retain:{" "}
                    {p.retained.join(", ") ||
                      "Other independent access keeps its own conditions"}
                  </p>
                  <p>{p.reviews} ERPX role requests need independent review.</p>
                </section>
              ))}
              {preview.warnings.map((w) => (
                <p key={w}>{w}</p>
              ))}
              <button
                type="button"
                className="button"
                disabled={busy}
                onClick={async () => {
                  const result = await send("apply", {
                    ...input,
                    previewToken: preview.token,
                  });
                  if (result) {
                    setOpen(false);
                    setSelected([]);
                    setNotice(
                      "Provisioning queued. ERPX role requests remain subject to independent review.",
                    );
                    router.refresh();
                  }
                }}
              >
                Apply resources & request role reviews
              </button>
            </div>
          )}
          {error && <p role="alert">{error}</p>}
        </form>
      </Dialog>
      <Dialog
        open={!!person}
        onClose={() => setDetail(null)}
        title={person?.name ?? "Applied access"}
        presentation="responsive-panel"
      >
        {person && (
          <div className={styles.stack}>
            <p>
              HR position:{" "}
              {lifecycle.positions.find((p) => p.id === person.positionId)
                ?.title ?? "Unassigned"}
            </p>
            {data.applications
              .filter((a) => a.engagement_id === person.engagementId)
              .map((a) => (
                <section className={styles.preview} key={a.id}>
                  <h3>
                    {a.definition.name} · r{a.package_revision}
                  </h3>
                  <p>
                    Source: {a.source_kind} ·{" "}
                    {a.active ? "Effective" : "Inactive / pending"}
                  </p>
                  <p>Expires: {new Date(a.valid_until).toLocaleString()}</p>
                  <p>
                    ERPX roles:{" "}
                    {a.definition.grants.map((g) => g.roleId).join(", ") ||
                      "None"}{" "}
                    (reviewed separately)
                  </p>
                  <p>
                    Groups:{" "}
                    {[
                      ...a.definition.groupIds,
                      ...a.definition.directoryGroupIds,
                    ].join(", ") || "None"}
                  </p>
                  <p>Exceptions: {a.scope.exclude?.join(", ") || "None"}</p>
                  {canApply && !a.ended_at && (
                    <button
                      className="button secondary"
                      onClick={() => {
                        setSelected([a.engagement_id]);
                        setDetail(null);
                        setWithdraw(a);
                        setPreview(null);
                        setInput(null);
                        setOpen(true);
                      }}
                    >
                      Withdraw this source
                    </button>
                  )}
                </section>
              ))}
          </div>
        )}
      </Dialog>
    </div>
  );
}
