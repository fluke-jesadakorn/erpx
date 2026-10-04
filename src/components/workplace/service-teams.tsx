"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { TimeText } from "@/components/time/time-preference-context";
import { useState } from "react";
import { Combobox } from "@/components/ui/popups";
import { workFetch } from "@/lib/work-fetch";
import type { previewServiceTeam } from "@/server/service-teams";
import { Dialog } from "@/components/ui/dialog";
import { useHydrated } from "@/components/ui/use-hydrated";
import { useStaffCommand } from "@/components/modules/staff-work-controls";
import { useRecoverableDraft } from "@/components/ui/use-recoverable-draft";
import { serviceCategories, categoryLabel } from "@/lib/employee-services";
import type { serviceTeamOptions } from "@/server/service-team-options";
import styles from "./services.module.css";
import { Card } from "@/components/ui/foundation";
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
    <Card as="section" appearance="standard" className="ui-panel" id="service-teams">
      <h2>Service teams</h2>
      <p>
        Use shared company coverage or assign a local team. Team membership does not grant business
        roles.
      </p>
      <div className={styles.catalog}>
        {serviceCategories.map(([id, label]) => {
          const teams = options.teams.filter((t) => t.coverage.some((c) => c.category === id));
          return (
            <article className={styles.template} key={id}>
              <h3>{label}</h3>
              <p className={styles.quiet}>
                {teams.filter((t) => t.active).length
                  ? `${teams.filter((t) => t.active).length} configured team(s)`
                  : "Shared coverage needs setup"}
              </p>
              {teams.map((t) => (
                <Button appearance="standard"
                  variant="secondary"
                  type="button"

                  key={t.id}
                  onClick={() => setEditing({ category: id, team: t })}
                >
                  {t.name}
                  {!t.active ? " · Disabled" : ""}
                </Button>
              ))}
              <Button appearance="standard"
                variant="ghost"
                type="button"

                onClick={() => setEditing({ category: id })}
              >
                {teams.length ? "Add coverage" : "Set up team"}
              </Button>
            </article>
          );
        })}
      </div>
      <Dialog appearance="standard"
        title={editing?.team ? "Edit service team" : "Set up service team"}
        open={!!editing}
        onClose={() => setEditing(null)}
       presentation="form-drawer">
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
    </Card>
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
  const [preview,setPreview]=useState<Awaited<ReturnType<typeof previewServiceTeam>>|null>(null),[previewError,setPreviewError]=useState(""),[previewing,setPreviewing]=useState(false);
  const ready = useHydrated(),
    command = useStaffCommand(base),
    firstNode = options.nodes.find((n) => n.kind === "company") ?? options.nodes[0];
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
          scopeKind: firstNode?.kind === "company" ? "holding" : (firstNode?.kind ?? "holding"),
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
    set = (name: keyof typeof f, value: string) => {setPreview(null);draft.update((p) => ({ ...p, [name]: value }));};
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
    if(!preview){setPreviewError("");setPreviewing(true);try{
      const response=await workFetch(`/api/v1${base}/service-teams/preview`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({id:team?.id,name:f.name,active:f.active==="true",members,coverage,expectedVersion:team?.version??1})});
      const body=await response.json();if(!response.ok)throw Error(body.error?.message??"Unable to preview coverage.");setPreview(body.data);
    }catch(error){setPreviewError(error instanceof Error?error.message:"Unable to preview coverage.");}finally{setPreviewing(false);}return;}
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
      <fieldset className={styles.form} disabled={!ready || command.busy || previewing}>
        <Field appearance="standard">
          Team name
          <TextInput appearance="standard"
            value={f.name}
            onChange={(e) => set("name", e.target.value)}
            required
            maxLength={160}
          />
        </Field>
        <Field appearance="standard">
          Availability
          <Select appearance="standard" value={f.active} onChange={(e) => set("active", e.target.value)}>
            <option value="true">Active coverage</option>
            <option value="false">Disabled</option>
          </Select>
        </Field>
        <h3>Coverage</h3>
        {coverage.map((c, index) => (
          <div className={styles.members} key={index}>
            <Field appearance="standard">
              Service
              <Select appearance="standard"
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
              </Select>
            </Field>
            <Field appearance="standard">
              Scope
              <Combobox label={`Coverage scope ${index+1}`} options={options.nodes.map(n=>({value:n.id,label:`${n.display_name} · ${n.kind}`}))} value={c.scopeNodeId} disabled={command.busy||previewing} onValueChange={value=>{const n=options.nodes.find(n=>n.id===value)!;set("coverage",JSON.stringify(coverage.map((v,i)=>i===index?{...v,scopeNodeId:n.id,scopeKind:n.kind==="company"?"holding":n.kind}:v)));}}/>

            </Field>
            <Button appearance="standard"
              variant="ghost"
              type="button"

              onClick={() =>
                set("coverage", JSON.stringify(coverage.filter((_, i) => i !== index)))
              }
            >
              Remove coverage
            </Button>
          </div>
        ))}
        <Button appearance="standard"
          variant="secondary"
          type="button"

          onClick={() =>
            set(
              "coverage",
              JSON.stringify([
                ...coverage,
                {
                  category,
                  scopeKind: firstNode?.kind === "company" ? "holding" : firstNode?.kind,
                  scopeNodeId: firstNode?.id,
                },
              ]),
            )
          }
        >
          Add coverage
        </Button>
        <h3>People</h3>
        <p className={styles.quiet}>
          Choose at least one lead with access covering the team’s locations.
        </p>
        {members.map((m, index) => (
          <div key={index} className={styles.members}>
            <Field appearance="standard">
              Team member
              <Combobox label={`Team member ${index+1}`} options={options.members.map(p=>({value:p.id,label:p.name}))} value={m.userId} disabled={command.busy||previewing} onValueChange={value=>set("members",JSON.stringify(members.map((v,i)=>i===index?{...v,userId:value}:v)))}/>

            </Field>
            <Field appearance="standard">
              Responsibility
              <Select appearance="standard"
                value={m.responsibility}
                onChange={(e) =>
                  set(
                    "members",
                    JSON.stringify(
                      members.map((v, i) =>
                        i === index ? { ...v, responsibility: e.target.value } : v,
                      ),
                    ),
                  )
                }
              >
                <option value="lead">Team lead</option>
                <option value="member">Handler</option>
              </Select>
            </Field>
            <Button appearance="standard"
              variant="ghost"
              type="button"

              onClick={() => set("members", JSON.stringify(members.filter((_, i) => i !== index)))}
            >
              Remove member
            </Button>
          </div>
        ))}
        <Button appearance="standard"
          variant="secondary"
          type="button"

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
        </Button>
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
                    {config.active ? "Active" : "Disabled"} · <TimeText appearance="standard" value={h.created_at} /> ·{" "}
                    {options.members.find((p) => p.id === h.actor_id)?.name ?? "Company owner"}
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
        {preview&&<section className="request-route" aria-label="Coverage preview"><h3>Review version {preview.previousVersion} → {preview.newVersion}</h3><p>{f.name} · {f.active==="true"?"Active coverage":"Disabled"}</p><ol>{coverage.map((c,i)=><li key={i}>{categoryLabel(c.category)} · {options.nodes.find(n=>n.id===c.scopeNodeId)?.display_name}</li>)}</ol><p>{members.map(m=>`${options.members.find(p=>p.id===m.userId)?.name} (${m.responsibility})`).join(", ")}</p>{preview.previous&&<p>Previous configuration: {preview.previous.name} · {preview.previous.members.length} members · {preview.previous.coverage.length} coverage entries</p>}{preview.warnings.map((warning,i)=><p key={i} role="status">{warning}</p>)}<p>{preview.effect}</p><p>Saving rechecks active people, scope, leads and your current authority.</p></section>}
        <div className="request-footer"><Button appearance="standard" clientManaged type="submit" variant="primary" disabled={draft.conflict}>
          {preview?"Confirm team settings":"Review coverage"}
        </Button></div>
      </fieldset>
      {previewError&&<InlineNotice tone="danger" role="alert">{previewError}</InlineNotice>}
      {command.error && <InlineNotice tone="danger" role="alert">{command.error}</InlineNotice>}
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
        <Field appearance="standard">
          Audited reason
          <TextArea appearance="standard"
            required
            minLength={2}
            value={draft.fields.note}
            onChange={(e) => draft.update((p) => ({ ...p, note: e.target.value }))}
          />
        </Field>
        {!!approvers.length && (
          <Field appearance="standard">
            Independent replacement
            <Select appearance="standard"
              value={draft.fields.approver}
              onChange={(e) => draft.update((p) => ({ ...p, approver: e.target.value }))}
            >
              <option value="">Use current eligible pool</option>
              {approvers.map((id) => (
                <option key={id} value={id}>
                  {id}
                </option>
              ))}
            </Select>
          </Field>
        )}
        <p className={styles.quiet}>
          Revalidate department approval or move this request to the current covering team. Previous
          decisions and assignments remain in history.
        </p>
        <Button appearance="standard" clientManaged type="submit" variant="primary">
          Apply routing repair
        </Button>
      </fieldset>
      {command.error && <InlineNotice tone="danger" role="alert">{command.error}</InlineNotice>}
    </form>
  );
}
