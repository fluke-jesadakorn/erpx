"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import type { agentWorkspace } from "@/server/agent-governance";
import { FormLayout, Card } from "@/components/ui/foundation";
export function AgentGovernanceConsole({
  base,
  data,
}: {
  base: string;
  data: Awaited<ReturnType<typeof agentWorkspace>>;
}) {
  const router = useRouter(),
    last = useRef<{ body: string; key: string } | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  async function send(input: unknown) {
    const body = JSON.stringify(input);
    if (last.current?.body !== body) last.current = { body, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const r = await workFetch(`/api/v1${base}/agents`, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": last.current.key,
          },
          body,
        }),
        j = await r.json();
      if (!r.ok) {
        if (r.status < 500) last.current = null;
        throw new Error(j.error?.message ?? "Unable to update agent governance.");
      }
      last.current = null;
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry this action.");
    } finally {
      setBusy(false);
    }
  }
  const values = (e: React.FormEvent<HTMLFormElement>) =>
    Object.fromEntries(new FormData(e.currentTarget));
  return (
    <>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      <FormLayout
        className="ui-panel ui-form-grid"
        onSubmit={(e) => {
          e.preventDefault();
          void send({ ...values(e), entity: "agent" });
        }}
      >
        <h2>Draft agent</h2>
        <Field appearance="standard">
          Name
          <TextInput appearance="standard" name="name" required />
        </Field>
        <Field appearance="standard">
          Purpose
          <TextArea appearance="standard" name="purpose" required minLength={3} />
        </Field>
        <Button appearance="standard" type="submit" clientManaged variant="primary" disabled={busy}>
          Draft agent
        </Button>
      </FormLayout>
      {data.agents
        .filter((x) => x.status === "draft")
        .map((x) => (
          <Review key={String(x.id)} entity="agent" row={x} send={send} busy={busy} />
        ))}
      <FormLayout
        className="ui-panel ui-form-grid"
        onSubmit={(e) => {
          e.preventDefault();
          const f = values(e);
          void send({
            ...f,
            entity: "version",
            allowedSources: String(f.allowedSources).split(/\s+/).filter(Boolean),
          });
        }}
      >
        <h2>Draft version</h2>
        <Field appearance="standard">
          Approved agent
          <Select appearance="standard" name="agentId">
            {data.agents
              .filter((x) => x.status === "approved")
              .map((x) => (
                <option key={String(x.id)} value={String(x.id)}>
                  {String(x.name)}
                </option>
              ))}
          </Select>
        </Field>
        <Field appearance="standard">
          Read-only prompt
          <TextArea appearance="standard" name="prompt" required minLength={3} />
        </Field>
        <Field appearance="standard">
          Allowed paths
          <TextInput appearance="standard" name="allowedSources" placeholder="/home /work" required />
        </Field>
        <Button appearance="standard" type="submit" clientManaged variant="primary" disabled={busy}>
          Draft version
        </Button>
      </FormLayout>
      {data.versions
        .filter((x) => x.status === "draft")
        .map((x) => (
          <Review key={String(x.id)} entity="version" row={x} send={send} busy={busy} />
        ))}
      {data.versions.filter(x=>x.status==="approved").map(x=>(
        <Card as="section" appearance="standard" className="ui-panel" key={String(x.id)}>
          <h2>{String(x.agent_name)} · version {String(x.version_number)}</h2>
          <p>{String(x.prompt)}</p>
          <p>Approved sources: {(JSON.parse(String(x.allowed_sources_json)) as string[]).join(", ")}</p>
          <Lifecycle entity="version" row={x} actions={["retire"]} send={send} busy={busy}/>
        </Card>
      ))}
      <FormLayout
        className="ui-panel ui-form-grid"
        onSubmit={(e) => {
          e.preventDefault();
          const f = new FormData(e.currentTarget);
          void send({
            entity: "pool",
            name: f.get("name"),
            userIds: f.getAll("userIds"),
          });
        }}
      >
        <h2>Create routing pool</h2>
        <Field appearance="standard">
          Name
          <TextInput appearance="standard" name="name" required />
        </Field>
        <fieldset>
          <legend>Members</legend>
          {data.people.map((x) => (
            <Field appearance="standard" orientation="inline" key={String(x.id)}>
              <TextInput appearance="standard" type="checkbox" name="userIds" value={String(x.id)} />
              {String(x.name)}
            </Field>
          ))}
        </fieldset>
        <Button appearance="standard" type="submit" clientManaged variant="primary" disabled={busy}>
          Create pool
        </Button>
      </FormLayout>
      {data.pools.filter(x=>x.status==="active").map(pool=>(
        <Card as="section" appearance="standard" className="ui-panel" key={String(pool.id)}>
          <h2>Routing pool · {String(pool.name)}</h2>
          <FormLayout className="ui-form-grid" onSubmit={event=>{
            event.preventDefault();const f=new FormData(event.currentTarget);
            void send({entity:"pool",id:pool.id,action:"replace-members",expectedVersion:Number(pool.version),userIds:f.getAll("userIds"),reason:f.get("reason")});
          }}>
            <fieldset><legend>Current routing members</legend>
              {data.people.map(person=><Field appearance="standard" orientation="inline" key={String(person.id)}>
                <TextInput appearance="standard" type="checkbox" name="userIds" value={String(person.id)} defaultChecked={data.members.some(member=>member.pool_id===pool.id&&member.user_id===person.id&&member.revoked_at===null)}/>
                {String(person.name)}
              </Field>)}
            </fieldset>
            <Field appearance="standard">Replacement reason<TextArea appearance="standard" name="reason" required minLength={3}/></Field>
            <Button appearance="standard" type="submit" clientManaged disabled={busy}>Replace members</Button>
          </FormLayout>
          <Lifecycle entity="pool" row={pool} actions={["retire"]} send={send} busy={busy}/>
          <details><summary>Pool membership history</summary><ul>{data.poolHistory.filter(revision=>revision.pool_id===pool.id).map(revision=><li key={String(revision.id)}>Revision {String(revision.version)} · {String(revision.reason)} · {(JSON.parse(String(revision.members_json)) as string[]).join(", ")}</li>)}</ul></details>
        </Card>
      ))}
      <FormLayout
        className="ui-panel ui-form-grid"
        onSubmit={(e) => {
          e.preventDefault();
          const f = values(e);
          void send({
            entity: "assignment",
            agentVersionId: f.agentVersionId,
            poolId: f.poolId || undefined,
            userId: f.poolId ? undefined : f.userId,
            scopePath: f.scopePath,
          });
        }}
      >
        <h2>Assign approved version</h2>
        <Field appearance="standard">
          Version
          <Select appearance="standard" name="agentVersionId">
            {data.versions
              .filter((x) => x.status === "approved")
              .map((x) => (
                <option key={String(x.id)} value={String(x.id)}>
                  {String(x.agent_name)} v{String(x.version_number)}
                </option>
              ))}
          </Select>
        </Field>
        <Field appearance="standard">
          Pool
          <Select appearance="standard" name="poolId">
            <option value="">Direct user</option>
            {data.pools
              .filter((x) => x.status === "active")
              .map((x) => (
                <option key={String(x.id)} value={String(x.id)}>
                  {String(x.name)}
                </option>
              ))}
          </Select>
        </Field>
        <Field appearance="standard">
          Direct user
          <Select appearance="standard" name="userId">
            {data.people.map((x) => (
              <option key={String(x.id)} value={String(x.id)}>
                {String(x.name)}
              </option>
            ))}
          </Select>
        </Field>
        <Field appearance="standard">
          Path scope
          <TextInput appearance="standard" name="scopePath" defaultValue="/" required />
        </Field>
        <Button appearance="standard" type="submit" clientManaged variant="primary" disabled={busy}>
          Assign
        </Button>
      </FormLayout>
      {data.assignments.filter(x=>x.status==="active").map(row=><Card as="section" appearance="standard" className="ui-panel" key={String(row.id)}>
        <h2>Active assignment · {String(row.agent_name)} v{String(row.version_number)}</h2>
        <p>Scope {String(row.scope_path)} · {String(row.user_id??row.pool_id)}</p>
        <Lifecycle entity="assignment" row={row} actions={["revoke"]} send={send} busy={busy}/>
      </Card>)}
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Private session retention</h2>
        <p>A hold preserves evidence. Expiry starts a fresh private conversation and keeps retained evidence out of the new history.</p>
        {data.sessions.map(session=><article key={String(session.id)}>
          <h3>{String(session.conversation_key)} · generation {String(session.generation)}</h3>
          <p>{String(session.user_id)} · {String(session.effective_status)} · expires {new Date(Number(session.expires_at)).toISOString()} {session.legal_hold?"· evidence held":""}</p>
          <Lifecycle entity="session" row={session} actions={[session.legal_hold?"release-hold":"hold",...(session.effective_status==="active"?["expire"]:[])]} send={send} busy={busy}/>
        </article>)}
      </Card>
    </>
  );
}
function Lifecycle({entity,row,actions,send,busy}:{entity:"version"|"assignment"|"pool"|"session";row:Record<string,unknown>;actions:string[];send:(input:unknown)=>Promise<void>;busy:boolean}) {
  return <FormLayout className="ui-form-grid" onSubmit={event=>{
    event.preventDefault();const f=new FormData(event.currentTarget);
    void send({entity,id:row.id,action:f.get("action"),expectedVersion:Number(row.version),reason:f.get("reason")});
  }}>
    <Field appearance="standard">Action<Select appearance="standard" name="action">{actions.map(action=><option key={action} value={action}>{action.replaceAll("-"," ")}</option>)}</Select></Field>
    <Field appearance="standard">Reason<TextArea appearance="standard" name="reason" required minLength={3}/></Field>
    <Button appearance="standard" type="submit" clientManaged disabled={busy}>Apply {entity} action</Button>
  </FormLayout>;
}
function Review({
  entity,
  row,
  send,
  busy,
}: {
  entity: "agent" | "version";
  row: Record<string, unknown>;
  send: (i: unknown) => Promise<void>;
  busy: boolean;
}) {
  return (
    <FormLayout
      className="ui-panel ui-form-grid"
      onSubmit={(e) => {
        e.preventDefault();
        void send({
          entity,
          id: row.id,
          action: "approve",
          expectedVersion: Number(row.version),
          reason: String(new FormData(e.currentTarget).get("reason")),
        });
      }}
    >
      <h2>Independent review · {String(row.name ?? row.agent_name)}</h2>
      <Field appearance="standard">
        Reason
        <TextArea appearance="standard" name="reason" required minLength={3} />
      </Field>
      <Button appearance="standard" type="submit" clientManaged variant="primary" disabled={busy}>
        Approve
      </Button>
    </FormLayout>
  );
}
