import { PilotFeatureRollout } from '@/components/workplace/pilot-feature-rollout';
import { requireWorkContext } from '@/server/authorization';
import { companyFeatureCatalogue } from '@/server/hospitality-rollout';
import { requirePermission } from '@/server/iam-permissions';
import { publicScope } from '@/server/authorization';
import { workspaceBase } from '@/lib/workspace';

export const dynamic='force-dynamic';

export default async function PilotFeaturesPage({params}:PageProps<'/holdings/[holdingSlug]/settings/pilot-features'>) {
  const actor=await requireWorkContext((await params).holdingSlug);
  requirePermission(actor,'organization.manage');
  return <PilotFeatureRollout base={workspaceBase(publicScope(actor))} initial={await companyFeatureCatalogue(actor)}/>;
}
