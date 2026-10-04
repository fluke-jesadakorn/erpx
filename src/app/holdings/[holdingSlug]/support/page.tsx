import { Button, Field, Select } from "@/components/ui/foundation";
import Link from "next/link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/page-error";
import { supportWorkspace, supportRecipients } from "@/server/support";
import { SupportConsole } from "@/components/modules/support-console";
import {readSourceRecord} from "@/server/source-record-policy";
import { PageContent, Card } from "@/components/ui/foundation";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ orderId?: string;sourceKind?:string;sourceId?:string;sourceVersion?:string }>;
}) {
  const { holdingSlug } = await params,
    a = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(a)),
    data = await supportWorkspace(a).catch((e) => pageError(e, holdingSlug)),
    query = await searchParams,{orderId}=query,
    order = data.sources.find((s) => s.id === orderId),
    recipients = order ? await supportRecipients(a, String(order.id)) : [],
    typedSource=query.sourceKind&&query.sourceId&&['pos_sale','pms_record','asset'].includes(query.sourceKind)?await readSourceRecord(a,{kind:query.sourceKind as 'pos_sale'|'pms_record'|'asset',id:query.sourceId,version:Number(query.sourceVersion)}).catch(e=>pageError(e,holdingSlug)):undefined;
  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={`${base}/home`}>Today</Link>
      <h1>Support issues</h1>
      {!data.ownReportsOnly&&<><Link href={`${base}/support/sla`}>Resolution-time watch and policies</Link>
      <Link href={`${base}/support/incidents`}>Shared incidents</Link></>}
      <p>
        Report an issue directly or connect a customer quotation or order. Every case shows who is
        responsible and what happens next.
      </p>
      <SupportConsole
        key={orderId}
        base={base}
        data={data}
        source={order ? { id: String(order.id), version: Number(order.version) } : undefined}
        recipients={recipients}
        typedSource={typedSource?{...typedSource,kind:typedSource.kind as 'pos_sale'|'pms_record'|'asset'}:undefined}
      />
      {data.canCreateOrder && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Link a customer document</h2>
          <form>
            <Field appearance="standard">
              Customer quotation or sales order
              <Select appearance="standard" name="orderId" defaultValue={orderId ?? ""} required>
                <option value="">Choose source</option>
                {data.sources.map((s) => (
                  <option key={String(s.id)} value={String(s.id)}>
                    {String(s.customer_name)} · {String(s.number)}
                  </option>
                ))}
              </Select>
            </Field>
            <Button appearance="standard" type="submit" variant="primary">
              Choose source
            </Button>
          </form>
        </Card>
      )}
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>{data.ownReportsOnly?'My reports':'Cases in this scope'}</h2>
        {data.items.map((r) => (
          <Link className="ui-record-item" key={String(r.id)} href={`${base}/support/${r.id}`}>
            {String(r.title)} · {String(r.status)} ·{" "}
            {r.due_date ? `Due ${String(r.due_date)}` : "Needs triage"}
          </Link>
        ))}
        {!data.items.length && <p>No cases in this scope.</p>}
      </Card>
    </PageContent>
  );
}
