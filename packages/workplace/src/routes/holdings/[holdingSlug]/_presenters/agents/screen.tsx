import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import {recordCanonicalHref} from "@erpx/feature-records/lib/record-drawer";
import { RecordCatalog } from "@erpx/feature-records/components/record-catalog";
import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { agentWorkspace } from "@erpx/server-agent/public/agent-governance";
import { nativeAgentEvidence, nativeAgentEvidenceFailure } from "@erpx/workplace/lib/native-agent-evidence";
import { ApiError } from "@erpx/server-runtime/public/http";
import { AgentGovernanceConsole } from "@erpx/feature-agent/components/agent-governance-console";
import { PageContent, Card, InlineNotice } from "@erpx/ui/components/foundation";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params,
    a = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(a)),
    workspace = await agentWorkspace(a, false).catch((e) => pageError(e, holdingSlug));
  let evidence;
  try {
    evidence = await nativeAgentEvidence(a, holdingSlug);
  } catch (error) {
    const failure = nativeAgentEvidenceFailure(error);
    return pageError(
      failure ? new ApiError(failure.status, failure.code, failure.message) : error,
      holdingSlug,
    );
  }
  const data = evidence
    ? { ...workspace, invocations: evidence.invocations, sessions: evidence.sessions }
    : { ...workspace, invocations: [], sessions: [] };
  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={canonicalUiHref(`${base}/settings?section=ai`)}>AI settings</Link>
      <h1>Agent governance</h1>
      <p>
        Approve read-only agent versions, route them to users or pools and
        inspect immutable private-session invocation evidence.
      </p>
      <AgentGovernanceConsole base={base} data={data} evidenceUnavailable={!evidence} />
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Assignments</h2>
        {data.assignments.map((x) => (
          <p key={String(x.id)}>
            <Link href={canonicalUiHref(`${base}/record-details/agent_assignment/${x.id}`)} record={{kind:"agent_assignment",id:String(x.id)}}>{String(x.agent_name)} v{String(x.version_number)}</Link> ·{" "}
            {String(x.scope_path)} · {String(x.status)}
          </p>
        ))}
      </Card>
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Recent invocation evidence</h2>
        {!evidence ? (
          <InlineNotice tone="danger" role="alert">
            Native invocation evidence is unavailable.
          </InlineNotice>
        ) : (
          data.invocations.map((x) => (
            <p key={String(x.id)}>
              <Link href={recordCanonicalHref(base,{kind:"agent_invocation",id:String(x.id)})}>{String(x.outcome)} · {String(x.turn_id)}</Link> ·{" "}
              <TimeText appearance="standard" value={Number(x.created_at)} />
            </p>
          ))
        )}
      </Card>
    <RecordCatalog base={base} kind="agent_pool" title="Agent pools" rows={data.pools} label="name"/><RecordCatalog base={base} kind="agent_version" title="Agent versions" rows={data.versions} label="agent_name"/></PageContent>
  );
}
