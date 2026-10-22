import {forbidden} from "next/navigation";
import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import { PeopleNavigation } from "@erpx/feature-people_payroll/components/people-navigation";
import { EvaluationConsole } from "@erpx/feature-people_payroll/components/evaluation-console";
import { PageHeader } from "@erpx/ui/components/primitives";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { evaluationWorkspace } from "@erpx/server-people_payroll/public/people-evaluation";
import { pageError } from "@erpx/server-work_projection/public/page-error";

export const dynamic = "force-dynamic";
export default async function EvaluationPage({ params, searchParams }: { params: Promise<{ holdingSlug: string }>; searchParams: Promise<Record<string, string | undefined>> }) {
  const { holdingSlug } = await params, actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor));
  const data = await evaluationWorkspace(actor, Object.fromEntries(Object.entries(await searchParams).filter(([, value]) => value !== undefined))).catch(e => pageError(e, holdingSlug));
  if((await searchParams).view==="criteria"&&!data.canConfigure)forbidden();
  return <FeaturePage className="content-stack" data-source="live"><PageHeader appearance="standard" eyebrow="People" title="Skills, performance & compensation" description="Compare role requirements, demonstrated skills, annual results and approved pay using Company-confirmed criteria." /><PeopleNavigation base={base} actor={actor} scopeKind={actor.scope.kind} active="evaluation" /><EvaluationConsole initial={data} base={base} apiBase={`/api/v1${base}/people/evaluation`} /></FeaturePage>;
}
