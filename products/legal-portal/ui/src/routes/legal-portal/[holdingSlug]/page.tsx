import {legalPortalSharesDisplay,legalPortalPreferenceDisplay} from '@erpx/legal-ui/lib/portal-display';
import {legalPortalSnapshot} from '@erpx/legal-ui/lib/portal-snapshot';
import {legalPortalPresentationSnapshot} from '@erpx/legal-server/public/presentation-context-queries';

import { LegalPortal } from "@erpx/legal-ui/components/legal-portal";
import { legalPortalQuickAccess } from "@erpx/work-projection-server/public/portal-quick-access";
import { redirect } from "next/navigation";
import {ApiError} from '@erpx/platform-runtime-server/public/http';
export const dynamic = "force-dynamic";
export default async function Page({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ share?: string | string[] }>;
}) {
  const { holdingSlug } = await params,requestedShare=(await searchParams).share,initialShareId=typeof requestedShare==='string'?requestedShare:undefined,returnPath=`/legal-portal/${encodeURIComponent(holdingSlug)}`+(initialShareId?'?'+new URLSearchParams({share:initialShareId}):''),
    {initial,initialDetail:detail,preferences,commandScope}=await legalPortalPresentationSnapshot(holdingSlug,initialShareId).catch((error) => {
      if (error instanceof Error && error.message === "UNAUTHENTICATED"||error instanceof ApiError&&error.status===401)
        redirect(
          `/login?next=${encodeURIComponent(returnPath)}`,
        );
      throw error;
    });
  const items = legalPortalQuickAccess(holdingSlug, initial);
  const initialDetail=detail?legalPortalSnapshot(detail):undefined;
  return (
    <LegalPortal
      key={`${commandScope}:${initialShareId ?? "shared-documents"}`}
      slug={holdingSlug}
      commandScope={commandScope}
      initial={legalPortalSharesDisplay(initial)}
      initialShareId={initialShareId}
      initialDetail={initialDetail}
      quickAccessItems={items}
      quickAccessPreferences={legalPortalPreferenceDisplay(preferences)}
    />
  );
}
