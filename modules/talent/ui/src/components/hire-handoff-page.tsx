import "server-only";
import {FeaturePage} from "@erpx/platform-shared-ui/components/features/feature-page";
import Link from "@erpx/platform-workplace/components/workspace-link";
import {configuredProductHandoff} from "@erpx/platform-runtime-ui/lib/product-hosts";
import {workspaceBase} from "@erpx/work-projection-ui/lib/workspace";
import {publicScope,requireWorkContext} from "@erpx/identity-access-server/public/authorization";
import {hireHandoffWorkspace} from "@erpx/people-payroll-server/public/hire-handoff-workspace";
import {pageError} from "@erpx/work-projection-server/public/page-error";
import {HireHandoffEditor} from "@erpx/talent-ui/components/hire-handoff-editor";
import {talentProductPresentationOptions} from "@erpx/talent-server/public/product-presentation-request";

export async function HireHandoffPage({params}:{params:Promise<{holdingSlug:string;id:string}>}){
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug);
 let data;try{data=await hireHandoffWorkspace(actor,id,await talentProductPresentationOptions("hire"));}catch(error){return pageError(error,holdingSlug);}
 return <FeaturePage className="ui-page-content" data-source="live">
  <Link href={configuredProductHandoff("apps")??"/apps"} prefetch={false}>← Apps</Link>
  <p>Accepted hire handoff</p><h1>{data.handoff.legalName}</h1>
  <HireHandoffEditor base={workspaceBase(publicScope(actor))} data={data} owner={{userId:actor.userId,companyId:actor.companyId}}/>
 </FeaturePage>;
}
