import { legalPresentationQueries } from "@erpx/server-legal/public/presentation-queries";
import { nativeLegalFailure, nativeLegalRegister } from "@erpx/feature-legal/lib/native-legal";
import { ApiError } from "@erpx/server-runtime/public/http";
import { headers } from "next/headers";

import { PageContent } from "@erpx/ui/components/patterns";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";

import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";

import { LegalWorkspace } from "@erpx/feature-records/components/loaders/legal--legal-workspace";
import { kinds, type LegalKind } from "@erpx/server-legal/public/rules/model";
export const dynamic = "force-dynamic";
export default async function LegalPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  const { holdingSlug } = await params,
    a = await requireWorkContext(holdingSlug),
    presentation = await legalPresentationQueries(a.holdingId),
    q = await searchParams,
    retained = q.tool || q.workView
      ? await presentation.recordData(a)
      : await presentation.legalData(a, { ...q, kind: q.kind ?? "request" }),
    native = q.tool || q.workView
      ? undefined
      : await nativeLegalRegister(a, holdingSlug, { ...q, kind: q.kind ?? "request" }, await headers()).catch((error) => {
          const failure = nativeLegalFailure(error);
          if (failure) throw new ApiError(failure.status, failure.code, failure.message);
          throw error;
        }),
    initial = native
      ? { ...retained, items: native.records, total: native.total, page: native.page, counts: native.counts, enabled: native.enabled }
      : retained;
  return (
    <PageContent  data-source="live">
      <LegalWorkspace
        base={workspaceBase(publicScope(a))}
        initial={initial}
        initialKind={typeof q.kind === "string" && kinds.includes(q.kind as LegalKind) ? q.kind as LegalKind : "request"}
        initialCreate={q.new === "request"}
        initialDetail={
          typeof q.record === "string" ? await presentation.detail(a, q.record) : null
        }
      />
    </PageContent>
  );
}
