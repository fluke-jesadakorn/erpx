import { readSourceRecord } from "@erpx/server-records/public/source-record-policy";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { createNativeSupportReader } from "@erpx/feature-support/lib/native-support-read";
import { SupportWorkspaceView } from "@erpx/feature-support/components/support-workspace";
import { ApiError } from "@erpx/server-runtime/public/http";
import { headers } from "next/headers";
import { PageContent } from "@erpx/ui/components/foundation";

export const dynamic = "force-dynamic";

type Query = Record<string, string | string[] | undefined>;

function value(query: Query, key: string): string {
  const raw = query[key];
  return typeof raw === "string" ? raw : "";
}

export default async function Page({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<Query>;
}) {
  const { holdingSlug } = await params,
    actor = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(actor)),
    config = process.env.ERPX_NATIVE_SUPPORT_READ_CONFIG;
  if (!config) throw new ApiError(503, "NATIVE_SUPPORT_READ_UNAVAILABLE", "The native Support reader is unavailable.");
  const query = await searchParams,
    reader = createNativeSupportReader(config),
    incoming = await headers(),
    data = await reader.workspace(incoming, holdingSlug).catch((error) => pageError(error, holdingSlug)),
    creating = value(query, "new") === "1",
    orderId = creating ? value(query, "orderId") : "",
    source = orderId ? data.sources.find((row) => String(row.id) === orderId) : undefined,
    recipients = source ? await reader.recipients(incoming, holdingSlug, String(source.id)) : [],
    sourceKind = creating ? value(query, "sourceKind") : "",
    sourceId = creating ? value(query, "sourceId") : "",
    typed =
      sourceKind && sourceId && ["pos_sale", "pms_record", "asset"].includes(sourceKind)
        ? await readSourceRecord(actor, {
            kind: sourceKind as "pos_sale" | "pms_record" | "asset",
            id: sourceId,
            version: Number(value(query, "sourceVersion")),
          })
        : undefined;
  return (
    <PageContent as="div" className="ui-page-content">
      <h1>{data.ownReportsOnly ? "My service reports" : "Service cases"}</h1>
      <p>Open, triage, reply and resolve customer tickets. Support triage chooses an owner and follow-up date.</p>
      <SupportWorkspaceView
        base={base}
        data={data}
        query={query}
        recipients={recipients}
        source={source ? { id: String(source.id), version: Number(source.version) } : undefined}
        typedSource={
          typed ? { kind: typed.kind as "pos_sale" | "pms_record" | "asset", id: typed.id, version: typed.version, label: typed.label } : undefined
        }
      />
    </PageContent>
  );
}
