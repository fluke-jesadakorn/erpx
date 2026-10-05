import {recordCanonicalHref} from "@/lib/record-drawer";
import { RecordCatalog } from "@/components/records/record-catalog";
import { TimeText } from "@/components/time/time-preference-context";
import { RecordLink as Link } from "@/components/records/record-link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/page-error";
import { agentWorkspace } from "@/server/agent-governance";
import { AgentGovernanceConsole } from "@/components/modules/agent-governance-console";
import { PageContent, Card } from "@/components/ui/foundation";
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
    <PageContent as="div" className="ui-page-content">
      <Link href={`${base}/settings?section=ai`}>AI settings</Link>
      <h1>Agent governance</h1>
      <p>
        Approve read-only agent versions, route them to users or pools and
        inspect immutable private-session invocation evidence.
      </p>
      <AgentGovernanceConsole base={base} data={data} />
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Assignments</h2>
        {data.assignments.map((x) => (
          <p key={String(x.id)}>
            <Link href={`${base}/record-details/agent_assignment/${x.id}`} record={{kind:"agent_assignment",id:String(x.id)}}>{String(x.agent_name)} v{String(x.version_number)}</Link> ·{" "}
            {String(x.scope_path)} · {String(x.status)}
          </p>
        ))}
      </Card>
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Recent invocation evidence</h2>
        {data.invocations.map((x) => (
          <p key={String(x.id)}>
            <Link href={recordCanonicalHref(base,{kind:"agent_invocation",id:String(x.id)})}>{String(x.outcome)} · {String(x.turn_id)}</Link> ·{" "}
            <TimeText appearance="standard" value={Number(x.created_at)} />
          </p>
        ))}
      </Card>
    <RecordCatalog base={base} kind="agent_pool" title="Agent pools" rows={data.pools} label="name"/><RecordCatalog base={base} kind="agent_version" title="Agent versions" rows={data.versions} label="agent_name"/></PageContent>
  );
}
