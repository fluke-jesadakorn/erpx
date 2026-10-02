import { TimeText } from "@/components/time/time-preference-context";
import Link from "next/link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/page-error";
import { agentWorkspace } from "@/server/agent-governance";
import { AgentGovernanceConsole } from "@/components/modules/agent-governance-console";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params,
    a = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(a)),
    data = await agentWorkspace(a).catch((e) => pageError(e, holdingSlug));
  return (
    <div className="content-stack">
      <Link href={`${base}/settings?section=ai`}>AI settings</Link>
      <h1>Agent governance</h1>
      <p>
        Approve read-only agent versions, route them to users or pools and
        inspect immutable private-session invocation evidence.
      </p>
      <AgentGovernanceConsole base={base} data={data} />
      <section className="card">
        <h2>Assignments</h2>
        {data.assignments.map((x) => (
          <p key={String(x.id)}>
            {String(x.agent_name)} v{String(x.version_number)} ·{" "}
            {String(x.scope_path)} · {String(x.status)}
          </p>
        ))}
      </section>
      <section className="card">
        <h2>Recent invocation evidence</h2>
        {data.invocations.map((x) => (
          <p key={String(x.id)}>
            {String(x.outcome)} · {String(x.turn_id)} ·{" "}
            <TimeText value={Number(x.created_at)} />
          </p>
        ))}
      </section>
    </div>
  );
}
