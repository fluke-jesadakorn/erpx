import { requireWorkContext, publicScope } from "@/server/authorization";
import { memberProfileView } from "@/server/member-profiles";
import { workspaceBase } from "@/lib/workspace";
import { ProfilePage } from "@/components/profile/profile-page";
import { pageError } from "@/server/page-error";

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
