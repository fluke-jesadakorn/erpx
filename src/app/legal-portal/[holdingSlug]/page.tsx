import { portalContext, portalList } from "@/server/legal/portal";
import { LegalPortal } from "@/components/legal/legal-portal";
import { legalPortalQuickAccess } from "@/server/portal-quick-access";
import { readQuickAccessPreferences } from "@/server/quick-access-preferences";
import { holdingBindingBySlug } from "@/server/runtime";
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
    { write, actor } = await portalContext(holdingSlug).catch((error) => {
      if (error instanceof Error && error.message === "UNAUTHENTICATED")
        redirect(
          `/login?next=${encodeURIComponent(`/legal-portal/${holdingSlug}`)}`,
        );
      throw error;
    });
  const [binding, initial] = await Promise.all([
    holdingBindingBySlug(holdingSlug),
    portalList(write, actor),
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
