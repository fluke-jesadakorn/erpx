import {forbidden} from "next/navigation";
import { FeaturePage } from "@erpx/platform-shared-ui/components/features/feature-page";
import { PeopleNavigation } from "@erpx/people-payroll-ui/components/people-navigation";
import { EvaluationConsole } from "@erpx/people-payroll-ui/components/evaluation-console";
import { PageHeader } from "@erpx/platform-ui/components/primitives";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import {evaluationWorkspace} from '@erpx/people-payroll-server/public/people-evaluation';
import { pageError } from "@erpx/work-projection-server/public/page-error";

export const dynamic = "force-dynamic";
export default async function EvaluationPage({ params, searchParams }: { params: Promise<{ holdingSlug: string }>; searchParams: Promise<Record<string, string | undefined>> }) {
  const { holdingSlug } = await params, actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor));
  const data = await evaluationWorkspace(actor, Object.fromEntries(Object.entries(await searchParams).filter(([, value]) => value !== undefined))).catch(e => pageError(e, holdingSlug));
  if((await searchParams).view==="criteria"&&!data.canConfigure)forbidden();
  return <FeaturePage className="content-stack" data-source="live"><PageHeader appearance="standard" eyebrow="People" title="Skills, performance & compensation" description="Compare role requirements, demonstrated skills, annual results and approved pay using Company-confirmed criteria." /><PeopleNavigation base={base} actor={actor} scopeKind={actor.scope.kind} active="evaluation" /><EvaluationConsole initial={data} base={base} apiBase={`/api/v1${base}/people/evaluation`} /></FeaturePage>;
}
