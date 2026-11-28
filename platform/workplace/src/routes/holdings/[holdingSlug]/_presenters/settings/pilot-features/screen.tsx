import { PilotFeatureRollout } from '@erpx/platform-workplace/components/pilot-feature-rollout';
import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { companyFeatureCatalogue } from "@erpx/pms-server/public/hospitality-rollout";
import { requirePermission } from "@erpx/identity-access-server/public/iam-permissions";
import { publicScope } from "@erpx/identity-access-server/public/authorization";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";

export const dynamic='force-dynamic';

export default async function PilotFeaturesPage({params}:{ params: Promise<{ holdingSlug: string;  }>; searchParams: Promise<Record<string,string|string[]|undefined>> }) {
  const actor=await requireWorkContext((await params).holdingSlug);
  requirePermission(actor,'organization.manage');
  return <PilotFeatureRollout base={workspaceBase(publicScope(actor))} initial={await companyFeatureCatalogue(actor)}/>;
}
