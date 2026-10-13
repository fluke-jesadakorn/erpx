import { FutureFeatures } from "@/components/workplace/future-features";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { hasPermission } from "@/lib/identity_access/permissions";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { publicScope } from "@/server/modules/identity_access/public/authorization";

export const dynamic = "force-dynamic";

export default async function FutureFeaturesPage({params}:{params:Promise<{holdingSlug:string}>}) {
  const {holdingSlug}=await params;
  const actor=await requireWorkContext(holdingSlug);
  return <FutureFeatures pilotSetupHref={hasPermission(actor,'organization.manage')?`${workspaceBase(publicScope(actor))}/settings/pilot-features`:undefined}/>;
}
