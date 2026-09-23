import { FutureFeatures } from "@/components/workplace/future-features";
import { requireWorkContext } from "@/server/authorization";

export const dynamic = "force-dynamic";

export default async function FutureFeaturesPage({params}:{params:Promise<{holdingSlug:string}>}) {
  const {holdingSlug}=await params;
  await requireWorkContext(holdingSlug);
  return <FutureFeatures/>;
}
