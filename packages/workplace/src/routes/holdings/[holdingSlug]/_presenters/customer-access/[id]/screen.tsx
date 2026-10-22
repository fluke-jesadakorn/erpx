import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { customerAccessHistory, customerPortalInvitationHistory } from "@erpx/server-customer_authority/public/customer-access";
import { CustomerAccessConsole } from "@erpx/feature-records/components/loaders/modules--customer-access-console";
import { PageContent, Card } from "@erpx/ui/components/foundation";

export const dynamic = "force-dynamic";

export default async function Page({ params }: { params: Promise<{ holdingSlug: string; id: string }> }) {
  const { holdingSlug, id } = await params;
  const actor = await requireWorkContext(holdingSlug);
  const base = workspaceBase(publicScope(actor));
  let data: Awaited<ReturnType<typeof customerAccessHistory>> & Awaited<ReturnType<typeof customerPortalInvitationHistory>>;
  try {
    const [access, invitations] = await Promise.all([
      customerAccessHistory(actor, id),
      customerPortalInvitationHistory(actor, id),
    ]);
    data = { ...access, ...invitations };
  } catch (error) {
    return pageError(error, holdingSlug);
  }

  const binding = data.binding;
  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={canonicalUiHref(`${base}/customer-access`)}>Customer access</Link>
      <h1>{String(binding.number)}</h1>
      <CustomerAccessConsole key={`${binding.id}:${binding.version}`} base={base} detail={data} />
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Immutable decision history</h2>
        {data.events.map(event => <p key={`${event.version}`}>Version {String(event.version)} · {String(event.action)} · {String(event.reason)}</p>)}
      </Card>
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Customer read audit</h2>
        {data.reads.map((event, index) => (
          <p key={`${event.read_at}:${index}`}>
            <TimeText appearance="standard" value={Number(event.read_at)} /> · binding version {String(event.binding_version)} · projection {String(event.projection_version)}
          </p>
        ))}
        {!data.reads.length && <p>No customer reads recorded.</p>}
      </Card>
    </PageContent>
  );
}
