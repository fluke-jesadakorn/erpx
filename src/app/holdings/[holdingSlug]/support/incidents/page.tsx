import { Button, Field, Select } from "@/components/ui/foundation";
import { RecordLink as Link } from "@/components/records/record-link";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { incidentWorkspace } from "@/server/modules/support/public/support-incidents";
import { getSupportTicket, supportRecipients } from "@/server/modules/support/public/support";
import { SupportIncidentConsole } from "@/components/support/support-incident-console";
import { PageContent, FormLayout, Card } from "@/components/ui/foundation";
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
      <Link href={`${base}/support`}>Support cases</Link>
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
          <Link className="ui-record-item" key={String(r.id)} href={`${base}/support/incidents/${r.id}`}>
            {String(r.title)} · {String(r.severity)} · {String(r.status)} · Due {String(r.due_date)}
          </Link>
        ))}
        {!data.items.length && <p>No shared incidents in this scope.</p>}
      </Card>
    </PageContent>
  );
}
