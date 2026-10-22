import { PilotFeatureRollout } from '@erpx/workplace/components/pilot-feature-rollout';
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { companyFeatureCatalogue } from "@erpx/server-pms/public/hospitality-rollout";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { publicScope } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";

export const dynamic='force-dynamic';

export default async function PilotFeaturesPage({params}:{ params: Promise<{ holdingSlug: string;  }>; searchParams: Promise<Record<string,string|string[]|undefined>> }) {
  const actor=await requireWorkContext((await params).holdingSlug);
  requirePermission(actor,'organization.manage');
  return <PilotFeatureRollout base={workspaceBase(publicScope(actor))} initial={await companyFeatureCatalogue(actor)}/>;
}
