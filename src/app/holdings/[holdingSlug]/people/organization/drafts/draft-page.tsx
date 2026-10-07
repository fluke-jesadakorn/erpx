import { organizationPresentationQueries } from "@/server/modules/organization/public/presentation-queries";

import styles from "@/components/people/organization-draft-workspace.module.css";
import { FeaturePage } from "@/components/features/feature-page";
import { OrganizationDraftWorkspace } from "@/components/people/organization-draft-workspace";
import { workspaceBase } from "@/lib/workspace";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { draftAuthority } from "@/server/modules/organization/public/organization-drafts";


export async function DraftPage({holdingSlug,id,review=false}:{holdingSlug:string;id?:string;review?:boolean}) {
 const actor=await requireWorkContext(holdingSlug);draftAuthority(actor);
 const presentation=await organizationPresentationQueries(actor.holdingId);
 const organization=await presentation.organizationAccessData(actor);
 const company=await presentation.draftCompanyName(actor.companyId, actor.companyId);
 return <FeaturePage className={`content-stack ${styles.page}`} data-source="live"><OrganizationDraftWorkspace base={workspaceBase(publicScope(actor))} companyName={company?.display_name??"Company"} organization={organization} draftId={id} review={review}/></FeaturePage>;
}
