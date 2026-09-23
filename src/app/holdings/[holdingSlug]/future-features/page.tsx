import { FutureFeatures } from "@/components/workplace/future-features";
import { requireWorkContext } from "@/server/authorization";
import { hasPermission } from '@/lib/permissions';
import { workspaceBase } from '@/lib/workspace';
import { publicScope } from '@/server/authorization';

export const dynamic = "force-dynamic";

export default async function FutureFeaturesPage({params}:{params:Promise<{holdingSlug:string}>}) {
  const {holdingSlug}=await params;
  const actor=await requireWorkContext(holdingSlug);
  return <FutureFeatures pilotSetupHref={hasPermission(actor,'organization.manage')?`${workspaceBase(publicScope(actor))}/settings/pilot-features`:undefined}/>;
}
