import styles from "@/components/people/organization-draft-workspace.module.css";
import { FeaturePage } from "@/components/features/feature-page";
import { OrganizationDraftWorkspace } from "@/components/people/organization-draft-workspace";
import { workspaceBase } from "@/lib/workspace";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { draftAuthority } from "@/server/organization-drafts";
import { organizationAccessData } from "@/server/organization-access";
import { holdingResources } from "@/server/runtime";
export async function DraftPage({holdingSlug,id,review=false}:{holdingSlug:string;id?:string;review?:boolean}) {
 const actor=await requireWorkContext(holdingSlug);draftAuthority(actor);
 const {write}=await holdingResources(actor.holdingId);
 const organization=await organizationAccessData(write,actor);
 const company=await write.prepare("SELECT display_name FROM organization_nodes WHERE id=? AND company_id=?").bind(actor.companyId,actor.companyId).first<{display_name:string}>();
 return <FeaturePage className={`content-stack ${styles.page}`} data-source="live"><OrganizationDraftWorkspace base={workspaceBase(publicScope(actor))} companyName={company?.display_name??"Company"} organization={organization} draftId={id} review={review}/></FeaturePage>;
}
