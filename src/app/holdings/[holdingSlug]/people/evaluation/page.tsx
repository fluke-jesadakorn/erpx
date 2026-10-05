import { FeaturePage } from "@/components/features/feature-page";
import { PeopleNavigation } from "@/components/modules/people-navigation";
import { EvaluationConsole } from "@/components/people/evaluation-console";
import { PageHeader } from "@/components/ui/primitives";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { evaluationWorkspace } from "@/server/people-evaluation";
import { pageError } from "@/server/page-error";

export const dynamic = "force-dynamic";
export default async function EvaluationPage({ params, searchParams }: { params: Promise<{ holdingSlug: string }>; searchParams: Promise<Record<string, string | undefined>> }) {
  const { holdingSlug } = await params, actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor));
  const data = await evaluationWorkspace(actor, Object.fromEntries(Object.entries(await searchParams).filter(([, value]) => value !== undefined))).catch(e => pageError(e, holdingSlug));
  return <FeaturePage className="content-stack" data-source="live"><PageHeader appearance="standard" eyebrow="People" title="Skills, performance & compensation" description="Compare role requirements, demonstrated skills, annual results and approved pay using Company-confirmed criteria." /><PeopleNavigation base={base} actor={actor} scopeKind={actor.scope.kind} active="evaluation" /><EvaluationConsole initial={data} base={base} apiBase={`/api/v1${base}/people/evaluation`} /></FeaturePage>;
}
