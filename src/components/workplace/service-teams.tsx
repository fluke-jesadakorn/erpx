"use client";
import { useState } from "react";
import { Dialog } from "@/components/ui/dialog";
import { useHydrated } from "@/components/ui/use-hydrated";
import { useStaffCommand } from "@/components/modules/staff-work-controls";
import { useRecoverableDraft } from "@/components/ui/use-recoverable-draft";
import { serviceCategories, categoryLabel } from "@/lib/employee-services";
import type { serviceTeamOptions } from "@/server/service-team-options";
import styles from "./services.module.css";
type Options = Awaited<ReturnType<typeof serviceTeamOptions>>;
type Team = Options["teams"][number];
export function ServiceTeams({
  base,
  options,
  userId,
  companyId,
}: {
  base: string;
  options: Options;
  userId: string;
  companyId: string;
}) {
  const [editing, setEditing] = useState<{
    category: string;
    team?: Team;
  } | null>(null);
  return (
    <section className="card" id="service-teams">
      <h2>Service teams</h2>
      <p>
        Use shared company coverage or assign a local team. Team membership does
        not grant business roles.
      </p>
      <div className={styles.catalog}>
        {serviceCategories.map(([id, label]) => {
          const teams = options.teams.filter((t) =>
            t.coverage.some((c) => c.category === id),
          );
          return (
            <article className={styles.template} key={id}>
              <h3>{label}</h3>
              <p className={styles.quiet}>
                {teams.filter((t) => t.active).length
                  ? `${teams.filter((t) => t.active).length} configured team(s)`
                  : "Shared coverage needs setup"}
              </p>
              {teams.map((t) => (
                <button
                  type="button"
                  className="button secondary"
                  key={t.id}
                  onClick={() => setEditing({ category: id, team: t })}
                >
                  {t.name}
                  {!t.active ? " · Disabled" : ""}
                </button>
              ))}
              <button
                type="button"
                className="button ghost"
                onClick={() => setEditing({ category: id })}
              >
                {teams.length ? "Add coverage" : "Set up team"}
              </button>
            </article>
          );
        })}
      </div>
      <Dialog
        title={editing?.team ? "Edit service team" : "Set up service team"}
        open={!!editing}
        onClose={() => setEditing(null)}
      >
        {editing && (
          <TeamEditor
            key={`${editing.team?.id ?? editing.category}`}
            base={base}
            options={options}
            team={editing.team}
            category={editing.category}
            companyId={companyId}
            userId={userId}
            done={() => setEditing(null)}
          />
        )}
      </Dialog>
    </section>
  );
}
function TeamEditor({
  base,
  options,
  team,
  category,
  companyId,
  userId,
  done,
}: {
  base: string;
  options: Options;
  team?: Team;
  category: string;
  companyId: string;
  userId: string;
  done: () => void;
}) {
  const ready = useHydrated(),
    command = useStaffCommand(base),
    firstNode =
      options.nodes.find((n) => n.kind === "company") ?? options.nodes[0];
  const initial = {
    name: team?.name ?? categoryLabel(category),
    active: team ? String(!!team.active) : "true",
    members: JSON.stringify(
      team?.members.map((m) => ({
        userId: m.user_id,
        responsibility: m.responsibility,
      })) ?? [],
    ),
    coverage: JSON.stringify(
      team?.coverage.map((c) => ({
        category: c.category,
        scopeKind: c.scope_kind,
        scopeNodeId: c.scope_node_id,
      })) ?? [
        {
          category,
          scopeKind:
            firstNode?.kind === "company"
              ? "holding"
              : (firstNode?.kind ?? "holding"),
          scopeNodeId: firstNode?.id ?? companyId,
        },
      ],
    ),
  };
  const draft = useRecoverableDraft(
      `erpx:team:${companyId}:${userId}:${team?.id ?? category}`,
      initial,
      "team",
    ),
    f = draft.fields,
    set = (name: keyof typeof f, value: string) =>
      draft.update((p) => ({ ...p, [name]: value }));
  const members = JSON.parse(f.members) as {
      userId: string;
      responsibility: "lead" | "member";
    }[],
    coverage = JSON.parse(f.coverage) as {
      category: string;
      scopeKind: string;
      scopeNodeId: string;
    }[];
  async function save(e: React.FormEvent) {
    e.preventDefault();
    if (
      await command.send(`service-teams${team ? `/${team.id}` : ""}`, {
        name: f.name,
        active: f.active === "true",
        members,
        coverage,
        expectedVersion: team?.version ?? 1,
      })
    ) {
      draft.clear();
      done();
    }
  }
  return (
    <form onSubmit={save}>
      {draft.notice}
      <fieldset className={styles.form} disabled={!ready || command.busy}>
        <label>
          Team name
          <input
            value={f.name}
            onChange={(e) => set("name", e.target.value)}
            required
            maxLength={160}
          />
        </label>
        <label>
          Availability
          <select
            value={f.active}
            onChange={(e) => set("active", e.target.value)}
          >
            <option value="true">Active coverage</option>
            <option value="false">Disabled</option>
          </select>
        </label>
        <h3>Coverage</h3>
        {coverage.map((c, index) => (
          <div className={styles.members} key={index}>
            <label>
              Service
              <select
                value={c.category}
                onChange={(e) =>
                  set(
                    "coverage",
                    JSON.stringify(
                      coverage.map((v, i) =>
                        i === index ? { ...v, category: e.target.value } : v,
                      ),
                    ),
                  )
                }
              >
                {serviceCategories.map(([id, label]) => (
                  <option key={id} value={id}>
                    {label}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Scope
              <select
                value={c.scopeNodeId}
                onChange={(e) => {
                  const n = options.nodes.find((n) => n.id === e.target.value)!;
                  set(
                    "coverage",
                    JSON.stringify(
                      coverage.map((v, i) =>
                        i === index
                          ? {
                              ...v,
                              scopeNodeId: n.id,
                              scopeKind:
                                n.kind === "company" ? "holding" : n.kind,
                            }
                          : v,
                      ),
                    ),
                  );
                }}
              >
                {options.nodes.map((n) => (
                  <option key={n.id} value={n.id}>
                    {n.display_name} · {n.kind}
                  </option>
                ))}
              </select>
            </label>
            <button
              type="button"
              className="button ghost"
              onClick={() =>
                set(
                  "coverage",
                  JSON.stringify(coverage.filter((_, i) => i !== index)),
                )
              }
            >
              Remove coverage
            </button>
          </div>
        ))}
        <button
          type="button"
          className="button secondary"
          onClick={() =>
            set(
              "coverage",
              JSON.stringify([
                ...coverage,
                {
                  category,
                  scopeKind:
                    firstNode?.kind === "company" ? "holding" : firstNode?.kind,
                  scopeNodeId: firstNode?.id,
                },
              ]),
            )
          }
        >
          Add coverage
        </button>
        <h3>People</h3>
        <p className={styles.quiet}>
          Choose at least one lead with access covering the team’s locations.
        </p>
        {members.map((m, index) => (
          <div key={index} className={styles.members}>
            <label>
              Team member
              <select
                value={m.userId}
                onChange={(e) =>
                  set(
                    "members",
                    JSON.stringify(
                      members.map((v, i) =>
                        i === index ? { ...v, userId: e.target.value } : v,
                      ),
                    ),
                  )
                }
              >
                <option value="">Choose member</option>
                {options.members.map((p) => (
                  <option value={p.id} key={p.id}>
                    {p.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Responsibility
              <select
                value={m.responsibility}
                onChange={(e) =>
                  set(
                    "members",
                    JSON.stringify(
                      members.map((v, i) =>
                        i === index
                          ? { ...v, responsibility: e.target.value }
                          : v,
                      ),
                    ),
                  )
                }
              >
                <option value="lead">Team lead</option>
                <option value="member">Handler</option>
              </select>
            </label>
            <button
              type="button"
              className="button ghost"
              onClick={() =>
                set(
                  "members",
                  JSON.stringify(members.filter((_, i) => i !== index)),
                )
              }
            >
              Remove member
            </button>
          </div>
        ))}
        <button
          type="button"
          className="button secondary"
          onClick={() =>
            set(
              "members",
              JSON.stringify([
                ...members,
                {
                  userId: "",
                  responsibility: members.length ? "member" : "lead",
                },
              ]),
            )
          }
        >
          Add member
        </button>
        {team && (
          <details className={styles.details}>
            <summary>Coverage and membership history</summary>
            {team.history.map((h) => {
              const config = JSON.parse(h.configuration_json) as {
                name: string;
                active: boolean;
                members: { userId: string; responsibility: string }[];
                coverage: { category: string; scopeNodeId: string }[];
              };
              return (
                <div className={styles.history} key={h.version}>
                  <strong>
                    Revision {h.version} · {config.name}
                  </strong>
                  <p>
                    {config.active ? "Active" : "Disabled"} ·{" "}
                    {new Date(h.created_at).toLocaleString("en-GB", {
                      timeZone: "Asia/Bangkok",
                    })}{" "}
                    ·{" "}
                    {options.members.find((p) => p.id === h.actor_id)?.name ??
                      "Company owner"}
                  </p>
                  <p>
                    {(config.coverage ?? [])
                      .map(
                        (c) =>
                          `${categoryLabel(c.category)} · ${options.nodes.find((n) => n.id === c.scopeNodeId)?.display_name ?? "Previous location"}`,
                      )
                      .join("; ")}
                  </p>
                  <p>
                    {(config.members ?? [])
                      .map(
                        (m) =>
                          `${options.members.find((p) => p.id === m.userId)?.name ?? "Former member"} (${m.responsibility === "lead" ? "Lead" : "Handler"})`,
                      )
                      .join(", ")}
                  </p>
                </div>
              );
            })}
          </details>
        )}
        <button className="button" disabled={draft.conflict}>
          Save team
        </button>
      </fieldset>
      {command.error && <p role="alert">{command.error}</p>}
    </form>
  );
}
export function RequestRepair({
  base,
  id,
  version,
  approvers,
  companyId,
  userId,
}: {
  base: string;
  id: string;
  version: number;
  approvers: string[];
  companyId: string;
  userId: string;
}) {
  const ready = useHydrated(),
    command = useStaffCommand(base),
    draft = useRecoverableDraft(
      `erpx:request-repair:${companyId}:${userId}:${id}`,
      { note: "", approver: "" },
      "routing",
    );
  return (
    <form
      onSubmit={async (e) => {
        e.preventDefault();
        if (
          await command.send(`requests/${id}/actions`, {
            action: "repair",
            expectedVersion: version,
            note: draft.fields.note,
            approverUserId: draft.fields.approver || null,
          })
        )
          draft.clear();
      }}
    >
      <fieldset className={styles.form} disabled={!ready || command.busy}>
        <label>
          Audited reason
          <textarea
            required
            minLength={2}
            value={draft.fields.note}
            onChange={(e) =>
              draft.update((p) => ({ ...p, note: e.target.value }))
            }
          />
        </label>
        {!!approvers.length && (
          <label>
            Independent replacement
            <select
              value={draft.fields.approver}
              onChange={(e) =>
                draft.update((p) => ({ ...p, approver: e.target.value }))
              }
            >
              <option value="">Use current eligible pool</option>
              {approvers.map((id) => (
                <option key={id} value={id}>
                  {id}
                </option>
              ))}
            </select>
          </label>
        )}
        <p className={styles.quiet}>
          Revalidate department approval or move this request to the current
          covering team. Previous decisions and assignments remain in history.
        </p>
        <button className="button">Apply routing repair</button>
      </fieldset>
      {command.error && <p role="alert">{command.error}</p>}
    </form>
  );
}
