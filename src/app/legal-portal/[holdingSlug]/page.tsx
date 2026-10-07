import { legalPortalPresentationQueries } from "@/server/modules/legal/public/presentation-context-queries";

import { LegalPortal } from "@/components/legal/legal-portal";
import { legalPortalQuickAccess } from "@/server/modules/work_projection/public/portal-quick-access";
import { readQuickAccessPreferences } from "@/server/modules/work_projection/public/quick-access-preferences";
import { holdingBindingBySlug } from "@/server/modules/runtime/public/runtime";
import { redirect } from "next/navigation";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ share?: string | string[] }>;
}) {
  const { holdingSlug } = await params,
    { queries, actor } = await legalPortalPresentationQueries(holdingSlug).catch((error) => {
      if (error instanceof Error && error.message === "UNAUTHENTICATED")
        redirect(
          `/login?next=${encodeURIComponent(`/legal-portal/${holdingSlug}`)}`,
        );
      throw error;
    });
  const [binding, initial] = await Promise.all([
    holdingBindingBySlug(holdingSlug),
    queries.portalList(actor),
  ]);
  const items = legalPortalQuickAccess(holdingSlug, initial);
  const preferences = await readQuickAccessPreferences(actor.userId, "legal_portal", binding.id);
  const requestedShare = (await searchParams).share;
  const initialShareId = typeof requestedShare === "string" ? requestedShare : undefined;
  return (
    <LegalPortal
      key={initialShareId ?? "shared-documents"}
      slug={holdingSlug}
      initial={initial}
      initialShareId={initialShareId}
      quickAccessItems={items}
      quickAccessPreferences={preferences}
    />
  );
}
