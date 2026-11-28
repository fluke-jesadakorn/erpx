import "server-only";
import {FeaturePage} from "@erpx/platform-shared-ui/components/features/feature-page";
import Link from "@erpx/platform-workplace/components/workspace-link";
import {configuredProductHandoff} from "@erpx/platform-runtime-ui/lib/product-hosts";
import {workspaceBase} from "@erpx/work-projection-ui/lib/workspace";
import {publicScope,requireWorkContext} from "@erpx/identity-access-server/public/authorization";
import {candidatePrivacyWorkspace} from "@erpx/talent-server/public/candidate-privacy-workspace";
import {pageError} from "@erpx/work-projection-server/public/page-error";
import {CandidatePrivacyEditor} from "@erpx/talent-ui/components/candidate-privacy-editor";
import {talentProductPresentationOptions} from "@erpx/talent-server/public/product-presentation-request";

export async function CandidatePrivacyPage({params}:{params:Promise<{holdingSlug:string;id:string}>}){
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug);
 let data;try{data=await candidatePrivacyWorkspace(actor,id,await talentProductPresentationOptions("candidate-privacy"));}catch(error){return pageError(error,holdingSlug);}
 return <FeaturePage className="ui-page-content" data-source="live">
  <Link href={configuredProductHandoff("apps")??"/apps"} prefetch={false}>← Apps</Link>
  <h1>Candidate privacy</h1><p>Candidate {data.candidate.id}</p>
  <CandidatePrivacyEditor base={workspaceBase(publicScope(actor))} data={data}/>
 </FeaturePage>;
}
