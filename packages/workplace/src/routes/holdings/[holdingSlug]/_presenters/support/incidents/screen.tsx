import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { Button, Field, Select } from "@erpx/ui/components/foundation";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { incidentWorkspace } from "@erpx/server-support/public/support-incidents";
import { getSupportTicket, supportRecipients } from "@erpx/server-support/public/support";
import { SupportIncidentConsole } from "@erpx/feature-support/components/support-incident-console";
import { PageContent, FormLayout, Card } from "@erpx/ui/components/foundation";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ ticketId?: string }>;
}) {
  const { holdingSlug } = await params,
    a = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(a)),
    data = await incidentWorkspace(a).catch((e) => pageError(e, holdingSlug)),
    { ticketId } = await searchParams;
  const source =
      ticketId && data.canManage
        ? await getSupportTicket(a, ticketId).catch((e) => pageError(e, holdingSlug))
        : null,
    recipients = source ? await supportRecipients(a, String(source.order_id)) : [];
  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={canonicalUiHref(`${base}/support`)}>Support cases</Link>
      <h1>Shared incidents</h1>
      <p>
        Coordinate a common cause. Each affected customer case keeps its own owner, clock and
        customer confirmation.
      </p>
      {data.canManage && (
        <FormLayout className="ui-panel ui-form-grid">
          <Field appearance="standard">
            Initial customer case
            <Select appearance="standard" name="ticketId" required defaultValue={ticketId ?? ""}>
              <option value="">Choose case</option>
              {data.tickets.map((t) => (
                <option key={String(t.id)} value={String(t.id)}>
                  {String(t.title)}
                </option>
              ))}
            </Select>
          </Field>
          <Button appearance="standard" type="submit" variant="primary">
            Choose initial case
          </Button>
        </FormLayout>
      )}
      <SupportIncidentConsole
        key={ticketId}
        base={base}
        source={source ? { id: String(source.id), version: Number(source.version) } : undefined}
        recipients={recipients}
      />
      <Card as="section" appearance="standard" className="ui-panel">
        {data.items.map((r) => (
          <Link className="ui-record-item" key={String(r.id)} href={canonicalUiHref(`${base}/support/incidents/${r.id}`)}>
            {String(r.title)} · {String(r.severity)} · {String(r.status)} · Due {String(r.due_date)}
          </Link>
        ))}
        {!data.items.length && <p>No shared incidents in this scope.</p>}
      </Card>
    </PageContent>
  );
}
