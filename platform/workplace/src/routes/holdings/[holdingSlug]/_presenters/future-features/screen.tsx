import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { FutureFeatures } from "@erpx/platform-workplace/components/future-features";
import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { hasPermission } from "@erpx/identity-access-ui/lib/permissions";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { publicScope } from "@erpx/identity-access-server/public/authorization";

export const dynamic = "force-dynamic";

export default async function FutureFeaturesPage({params}:{params:Promise<{holdingSlug:string}>}) {
  const {holdingSlug}=await params;
  const actor=await requireWorkContext(holdingSlug);
  return <FutureFeatures pilotSetupHref={hasPermission(actor,'organization.manage')?canonicalUiHref(`${workspaceBase(publicScope(actor))}/settings/pilot-features`):undefined}/>;
}
