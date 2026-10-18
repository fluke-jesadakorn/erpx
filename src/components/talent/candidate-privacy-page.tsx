import "server-only";
import {FeaturePage} from "@/components/shared/features/feature-page";
import Link from "@/components/workplace/workspace-link";
import {configuredProductHandoff} from "@/lib/runtime/product-hosts";
import {workspaceBase} from "@/lib/work_projection/workspace";
import {publicScope,requireWorkContext} from "@/server/modules/identity_access/public/authorization";
import {candidatePrivacyWorkspace} from "@/server/modules/talent/public/candidate-privacy-workspace";
import {pageError} from "@/server/modules/work_projection/public/page-error";
import {CandidatePrivacyEditor} from "./candidate-privacy-editor";
import {talentProductPresentationOptions} from "@/server/modules/talent/public/product-presentation-request";

export async function CandidatePrivacyPage({params}:{params:Promise<{holdingSlug:string;id:string}>}){
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug);
 let data;try{data=await candidatePrivacyWorkspace(actor,id,await talentProductPresentationOptions("candidate-privacy"));}catch(error){return pageError(error,holdingSlug);}
 return <FeaturePage className="ui-page-content" data-source="live">
  <Link href={configuredProductHandoff("apps")??"/apps"} prefetch={false}>← Apps</Link>
  <h1>Candidate privacy</h1><p>Candidate {data.candidate.id}</p>
  <CandidatePrivacyEditor base={workspaceBase(publicScope(actor))} data={data}/>
 </FeaturePage>;
}
