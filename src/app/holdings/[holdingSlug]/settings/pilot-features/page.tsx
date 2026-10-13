import { PilotFeatureRollout } from '@/components/workplace/pilot-feature-rollout';
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { companyFeatureCatalogue } from "@/server/modules/pms/public/hospitality-rollout";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { publicScope } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";

export const dynamic='force-dynamic';

export default async function PilotFeaturesPage({params}:PageProps<'/holdings/[holdingSlug]/settings/pilot-features'>) {
  const actor=await requireWorkContext((await params).holdingSlug);
  requirePermission(actor,'organization.manage');
  return <PilotFeatureRollout base={workspaceBase(publicScope(actor))} initial={await companyFeatureCatalogue(actor)}/>;
}
