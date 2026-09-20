import { portalContext, portalList } from "@/server/legal/portal";
import { LegalPortal } from "@/components/legal/legal-portal";
import { redirect } from "next/navigation";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params,
    { write, actor } = await portalContext(holdingSlug).catch((error) => {
      if (error instanceof Error && error.message === "UNAUTHENTICATED")
        redirect(
          `/login?next=${encodeURIComponent(`/legal-portal/${holdingSlug}`)}`,
        );
      throw error;
    });
  return (
    <LegalPortal slug={holdingSlug} initial={await portalList(write, actor)} />
  );
}
