import "server-only";
import {FeaturePage} from "@erpx/feature-shared/components/features/feature-page";
import Link from "@erpx/workplace/components/workspace-link";
import {configuredProductHandoff} from "@erpx/feature-runtime/lib/product-hosts";
import {workspaceBase} from "@erpx/feature-work_projection/lib/workspace";
import {publicScope,requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {candidatePrivacyWorkspace} from "@erpx/server-talent/public/candidate-privacy-workspace";
import {pageError} from "@erpx/server-work_projection/public/page-error";
import {CandidatePrivacyEditor} from "@erpx/feature-talent/components/candidate-privacy-editor";
import {talentProductPresentationOptions} from "@erpx/server-talent/public/product-presentation-request";

export async function CandidatePrivacyPage({params}:{params:Promise<{holdingSlug:string;id:string}>}){
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug);
 let data;try{data=await candidatePrivacyWorkspace(actor,id,await talentProductPresentationOptions("candidate-privacy"));}catch(error){return pageError(error,holdingSlug);}
 return <FeaturePage className="ui-page-content" data-source="live">
  <Link href={configuredProductHandoff("apps")??"/apps"} prefetch={false}>← Apps</Link>
  <h1>Candidate privacy</h1><p>Candidate {data.candidate.id}</p>
  <CandidatePrivacyEditor base={workspaceBase(publicScope(actor))} data={data}/>
 </FeaturePage>;
}
