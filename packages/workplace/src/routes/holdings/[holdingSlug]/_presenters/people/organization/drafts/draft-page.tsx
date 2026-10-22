import { organizationPresentationQueries } from "@erpx/server-organization/public/presentation-queries";

import styles from "@erpx/feature-people_payroll/components/organization-draft-workspace.module.css";
import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import { OrganizationDraftWorkspace } from "@erpx/feature-people_payroll/components/organization-draft-workspace";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import { draftAuthority } from "@erpx/server-organization/public/organization-drafts";


export async function DraftPage({holdingSlug,id,review=false}:{holdingSlug:string;id?:string;review?:boolean}) {
 const actor=await requireWorkContext(holdingSlug);draftAuthority(actor);
 const presentation=await organizationPresentationQueries(actor.holdingId);
 const organization=await presentation.organizationAccessData(actor);
 const company=await presentation.draftCompanyName(actor.companyId, actor.companyId);
 return <FeaturePage className={`content-stack ${styles.page}`} data-source="live"><OrganizationDraftWorkspace base={workspaceBase(publicScope(actor))} companyName={company?.display_name??"Company"} organization={organization} draftId={id} review={review}/></FeaturePage>;
}
