import { organizationPresentationQueries } from "@erpx/organization-server/public/presentation-queries";

import styles from "@erpx/people-payroll-ui/components/organization-draft-workspace.module.css";
import { FeaturePage } from "@erpx/platform-shared-ui/components/features/feature-page";
import { OrganizationDraftWorkspace } from "@erpx/people-payroll-ui/components/organization-draft-workspace";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { requireWorkContext, publicScope } from "@erpx/identity-access-server/public/authorization";
import { draftAuthority } from "@erpx/organization-server/public/organization-drafts";


export async function DraftPage({holdingSlug,id,review=false}:{holdingSlug:string;id?:string;review?:boolean}) {
 const actor=await requireWorkContext(holdingSlug);draftAuthority(actor);
 const presentation=await organizationPresentationQueries(actor.holdingId);
 const organization=await presentation.organizationAccessData(actor);
 const company=await presentation.draftCompanyName(actor.companyId, actor.companyId);
 return <FeaturePage className={`content-stack ${styles.page}`} data-source="live"><OrganizationDraftWorkspace base={workspaceBase(publicScope(actor))} companyName={company?.display_name??"Company"} organization={organization} draftId={id} review={review}/></FeaturePage>;
}
