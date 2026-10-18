import { legalPresentationQueries } from "@/server/modules/legal/public/presentation-queries";

import { PageContent } from "@/components/ui/patterns";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";

import { workspaceBase } from "@/lib/work_projection/workspace";

import { LegalWorkspace } from "@/components/records/loaders/legal--legal-workspace";
import { kinds, type LegalKind } from "@/server/modules/legal/public/rules/model";
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
