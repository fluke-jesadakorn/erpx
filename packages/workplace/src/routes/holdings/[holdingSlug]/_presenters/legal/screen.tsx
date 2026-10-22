import { legalPresentationQueries } from "@erpx/server-legal/public/presentation-queries";

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
  const a = await requireWorkContext((await params).holdingSlug),
    presentation = await legalPresentationQueries(a.holdingId),
    q = await searchParams;
  return (
    <PageContent  data-source="live">
      <LegalWorkspace
        base={workspaceBase(publicScope(a))}
        initial={q.tool||q.workView?await presentation.recordData(a):await presentation.legalData(a, { ...q, kind: q.kind ?? "request" })}
        initialKind={typeof q.kind === "string" && kinds.includes(q.kind as LegalKind) ? q.kind as LegalKind : "request"}
        initialCreate={q.new === "request"}
        initialDetail={
          typeof q.record === "string" ? await presentation.detail(a, q.record) : null
        }
      />
    </PageContent>
  );
}
