import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { peoplePayrollPresentationQueries } from "@erpx/server-people_payroll/public/presentation-queries";

import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import {FeaturePage} from "@erpx/feature-shared/components/features/feature-page";
import {HireHandoffConsole} from "@erpx/feature-people_payroll/components/hire-handoff-console";
import {workspaceBase} from "@erpx/feature-work_projection/lib/workspace";
import {publicScope,requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {hireHandoffQueue} from "@erpx/server-people_payroll/public/people-handoffs";

import {pageError} from "@erpx/server-work_projection/public/page-error";
export const dynamic="force-dynamic";
export default async function HireHandoffsPage({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));let handoffs;try{handoffs=await hireHandoffQueue(actor);}catch(error){return pageError(error,holdingSlug);}const presentation=await peoplePayrollPresentationQueries(actor.holdingId),[groups,people]=await Promise.all([presentation.hireHandoffPayGroups(actor.companyId),presentation.hireHandoffPeople(actor.companyId)]);
 return <FeaturePage className="ui-page-content" data-source="live"><Link href={canonicalUiHref(`${base}/people`)}>← People</Link><h1>Accepted offers and HR onboarding</h1><p>Review the current offer and candidate identity, choose a Company pay group, and reconcile an existing person when appropriate. The original Talent handoff and its HR consumption receipt remain immutable.</p><HireHandoffConsole base={base} handoffs={handoffs} groups={groups.results} people={people.results}/></FeaturePage>;
}
