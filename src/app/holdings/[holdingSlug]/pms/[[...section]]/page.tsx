import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pmsWorkspace } from "@/server/pms";
import { PmsWorkspaceView } from "@/components/pms/workspace";
import { pmsSections } from "@/lib/pms-model";
import { pageError } from "@/server/page-error";
import { companyFeatureCatalogue } from "@/server/hospitality-rollout";
import { pmsCrossLinks } from "@/lib/pms-cross-links";
import { notFound } from "next/navigation";
export const dynamic = "force-dynamic";
export default async function PmsPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string; section?: string[] }>;
  searchParams: Promise<{ property?: string; folio?: string }>;
}) {
  const { holdingSlug, section } = await params;
  if (
    section &&
    (section.length !== 1 || !pmsSections.some(([id]) => id === section[0]))
  )
    notFound();
  const search = await searchParams,
    actor = await requireWorkContext(holdingSlug);
  let data;
  try {
    data = await pmsWorkspace(actor, search.property, section?.[0] === "night-audit");
  } catch (error) {
    return pageError(error, actor.holdingSlug);
  }
  const enabled = process.env.NODE_ENV === "production"
    ? new Set((await companyFeatureCatalogue(actor))
        .filter((entry) => ["Available", "Pilot", "Setup required"].includes(entry.state))
        .map((entry) => entry.id))
    : null;
  return (
    <PmsWorkspaceView
      initial={data}
      base={workspaceBase(publicScope(actor))}
      section={section?.[0] ?? "dashboard"}
      selectedFolio={search.folio}
      crossLinks={pmsCrossLinks(actor, enabled ?? undefined)}
    />
  );
}
