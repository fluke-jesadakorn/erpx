import "server-only";
import {FeaturePage} from "@/components/shared/features/feature-page";
import Link from "@/components/workplace/workspace-link";
import {configuredProductHandoff} from "@/lib/runtime/product-hosts";
import {workspaceBase} from "@/lib/work_projection/workspace";
import {publicScope,requireWorkContext} from "@/server/modules/identity_access/public/authorization";
import {hireHandoffWorkspace} from "@/server/modules/people_payroll/public/hire-handoff-workspace";
import {pageError} from "@/server/modules/work_projection/public/page-error";
import {HireHandoffEditor} from "./hire-handoff-editor";
import {talentProductPresentationOptions} from "@/server/modules/talent/public/product-presentation-request";

export async function HireHandoffPage({params}:{params:Promise<{holdingSlug:string;id:string}>}){
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug);
 let data;try{data=await hireHandoffWorkspace(actor,id,await talentProductPresentationOptions("hire"));}catch(error){return pageError(error,holdingSlug);}
 return <FeaturePage className="ui-page-content" data-source="live">
  <Link href={configuredProductHandoff("apps")??"/apps"} prefetch={false}>← Apps</Link>
  <p>Accepted hire handoff</p><h1>{data.handoff.legalName}</h1>
  <HireHandoffEditor base={workspaceBase(publicScope(actor))} data={data} owner={{userId:actor.userId,companyId:actor.companyId}}/>
 </FeaturePage>;
}
