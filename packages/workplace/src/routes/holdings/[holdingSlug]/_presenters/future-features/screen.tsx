import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { FutureFeatures } from "@erpx/workplace/components/future-features";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope } from "@erpx/server-identity_access/public/authorization";

export const dynamic = "force-dynamic";

export default async function FutureFeaturesPage({params}:{params:Promise<{holdingSlug:string}>}) {
  const {holdingSlug}=await params;
  const actor=await requireWorkContext(holdingSlug);
  return <FutureFeatures pilotSetupHref={hasPermission(actor,'organization.manage')?canonicalUiHref(`${workspaceBase(publicScope(actor))}/settings/pilot-features`):undefined}/>;
}
