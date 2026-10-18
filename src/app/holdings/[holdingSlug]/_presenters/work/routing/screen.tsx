import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { Button, Field, Select, Table } from "@/components/ui/foundation";
import { RecordLink as Link } from "@/components/records/record-link";
import { flowKinds } from "@/lib/shared/flow";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { routingAttention } from "@/server/modules/workflow/public/routing-attention";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { PageContent, FormLayout, Card, TableFrame } from "@/components/ui/foundation";
export const dynamic = "force-dynamic";
export default async function RoutingAttentionPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor)),
    q = await searchParams;
  const result = await routingAttention(actor, {
    kind: typeof q.kind === "string" ? q.kind : undefined,
    after: typeof q.after === "string" ? q.after : undefined,
  }).catch((error) => pageError(error, actor.holdingSlug));
  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={canonicalUiHref(`${base}/work?view=routing`)}>← Work routing attention</Link>
      <h1>Company routing review</h1>
      <p>
        Review named workflows across the selected source, without a recent-document cutoff. Only
        records you can read and manage appear here.
      </p>
      <FormLayout className="ui-form-grid">
        <Field appearance="standard">
          Source
          <Select appearance="standard" name="kind" defaultValue={result.kind}>
            {flowKinds.map((kind) => (
              <option value={kind} key={kind}>
                {kind.replaceAll("_", " ")}
              </option>
            ))}
          </Select>
        </Field>
        <Button appearance="standard" type="submit" variant="primary">
          Review source
        </Button>
      </FormLayout>
      <p>
        Coverage: named Flow sources. Legal, POS, PMS, overtime, lifecycle and payment owner queues
        remain separate. This page does not claim a scan of every domain or track how long a blocker
        has existed.
      </p>
      <Card as="section" appearance="standard" className="ui-panel">
        <TableFrame appearance="standard" label="Records"><Table className="business-table">
          <thead>
            <tr>
              <th>Record</th>
              <th>State / next action</th>
              <th>Responsible team</th>
              <th>Blocker</th>
            </tr>
          </thead>
          <tbody>
            {result.items.map((item) => (
              <tr key={item.id}>
                <td>
                  <Link href={canonicalUiHref(`${base}/work/routing/${item.kind}/${encodeURIComponent(item.id)}`)}>
                    {item.title}
                  </Link>
                </td>
                <td>
                  {item.state}
                  <br />
                  {item.action}
                </td>
                <td>{item.team ?? "Routing repair required"}</td>
                <td>{item.blocker}</td>
              </tr>
            ))}
          </tbody>
        </Table></TableFrame>
        {!result.items.length && (
          <p>No visible blocked workflows in this source after the selected position.</p>
        )}
      </Card>
      {result.nextAfter && (
        <UiActionLink appearance="standard"
          variant="secondary"
          href={`?kind=${result.kind}&after=${encodeURIComponent(result.nextAfter)}`}
        >
          Next blocked records
        </UiActionLink>
      )}
      <p>
        Refresh to recheck current eligibility. Reassignment requires a reason and keeps completed
        decisions unchanged.
      </p>
    </PageContent>
  );
}
