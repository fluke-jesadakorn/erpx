import "server-only";
import {FeaturePage} from "@erpx/feature-shared/components/features/feature-page";
import Link from "@erpx/workplace/components/workspace-link";
import {configuredProductHandoff} from "@erpx/feature-runtime/lib/product-hosts";
import {workspaceBase} from "@erpx/feature-work_projection/lib/workspace";
import {publicScope,requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {hireHandoffWorkspace} from "@erpx/server-people_payroll/public/hire-handoff-workspace";
import {pageError} from "@erpx/server-work_projection/public/page-error";
import {HireHandoffEditor} from "@erpx/feature-talent/components/hire-handoff-editor";
import {talentProductPresentationOptions} from "@erpx/server-talent/public/product-presentation-request";

export async function HireHandoffPage({params}:{params:Promise<{holdingSlug:string;id:string}>}){
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug);
 let data;try{data=await hireHandoffWorkspace(actor,id,await talentProductPresentationOptions("hire"));}catch(error){return pageError(error,holdingSlug);}
 return <FeaturePage className="ui-page-content" data-source="live">
  <Link href={configuredProductHandoff("apps")??"/apps"} prefetch={false}>← Apps</Link>
  <p>Accepted hire handoff</p><h1>{data.handoff.legalName}</h1>
  <HireHandoffEditor base={workspaceBase(publicScope(actor))} data={data} owner={{userId:actor.userId,companyId:actor.companyId}}/>
 </FeaturePage>;
}
