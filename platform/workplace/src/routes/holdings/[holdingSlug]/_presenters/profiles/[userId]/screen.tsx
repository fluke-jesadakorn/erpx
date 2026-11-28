import { requireWorkContext, publicScope } from "@erpx/identity-access-server/public/authorization";
import { memberProfileView } from "@erpx/organization-server/public/member-profiles";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { ProfilePage } from "@erpx/identity-access-ui/components/profile/profile-page";
import { pageError } from "@erpx/work-projection-server/public/page-error";

export const dynamic = "force-dynamic";

export default async function Page({ params }: { params: Promise<{holdingSlug:string;userId:string}> }) {
  const { holdingSlug, userId } = await params;
  let actor;
  let profile;
  try {
    actor = await requireWorkContext(holdingSlug);
    profile = await memberProfileView(actor,userId);
  } catch (error) { return pageError(error,holdingSlug); }
  return <ProfilePage initial={profile} base={workspaceBase(publicScope(actor))} />;
}
