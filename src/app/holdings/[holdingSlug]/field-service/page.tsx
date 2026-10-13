import { RecordLink as Link } from "@/components/records/record-link";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { fieldServiceWorkspace } from "@/server/modules/field_service/public/field-service";
import { FieldServiceConsole } from "@/components/field_service/field-service-console";
import { PageContent, Card } from "@/components/ui/foundation";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params,
    a = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(a)),
    data = await fieldServiceWorkspace(a).catch((e) =>
      pageError(e, holdingSlug),
    );
  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={`${base}/work`}>Work</Link>
      <h1>Field service</h1>
      <p>
        Route customer cases through agreements, dispatch, parts, visit evidence
        and independent billing release.
      </p>
      <FieldServiceConsole base={base} data={data} />
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Service orders</h2>
        {data.orders.map((x) => (
          <Link
            className="ui-record-item"
            key={String(x.id)}
            href={`${base}/field-service/${x.id}`}
          >
            {String(x.ticket_title)} · {String(x.customer_name)} ·{" "}
            {String(x.status)}
          </Link>
        ))}
        {!data.orders.length && <p>No service orders in this scope.</p>}
      </Card>
    </PageContent>
  );
}
