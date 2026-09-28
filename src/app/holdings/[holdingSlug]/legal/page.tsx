import { requireWorkContext, publicScope } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { workspaceBase } from "@/lib/workspace";
import { legalData, detail } from "@/server/legal/service";
import { LegalWorkspace } from "@/components/legal/legal-workspace";
export const dynamic = "force-dynamic";
export default async function LegalPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  const a = await requireWorkContext((await params).holdingSlug),
    { write } = await holdingResources(a.holdingId),
    q = await searchParams;
  return (
    <div className="content" data-source="live">
      <LegalWorkspace
        base={workspaceBase(publicScope(a))}
        initial={await legalData(write, a, { ...q, kind: q.kind ?? "request" })}
        initialDetail={
          typeof q.record === "string" ? await detail(write, a, q.record) : null
        }
      />
    </div>
  );
}
