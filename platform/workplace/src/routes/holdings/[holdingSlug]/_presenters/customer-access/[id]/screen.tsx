import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { TimeText } from "@erpx/people-payroll-ui/components/time/time-preference-context";
import { RecordLink as Link } from "@erpx/records-ui/components/record-link";
import { requireWorkContext, publicScope } from "@erpx/identity-access-server/public/authorization";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { pageError } from "@erpx/work-projection-server/public/page-error";
import { customerAccessHistory } from "@erpx/customer-authority-server/public/customer-access";
import { nativeCustomerAccessDetail, nativeCustomerAccessInvitationHistory } from "@erpx/sales-ui/public/native-customer-access-page";
import { CustomerAccessConsole } from "@erpx/records-ui/components/loaders/modules--customer-access-console";
import { PageContent, Card } from "@erpx/platform-ui/components/foundation";

export const dynamic = "force-dynamic";

export default async function Page({ params }: { params: Promise<{ holdingSlug: string; id: string }> }) {
  const { holdingSlug, id } = await params;
  const actor = await requireWorkContext(holdingSlug);
  const base = workspaceBase(publicScope(actor));
  let data: Awaited<ReturnType<typeof customerAccessHistory>> & Awaited<ReturnType<typeof nativeCustomerAccessInvitationHistory>>;
  try {
    const [nativeAccess, invitations] = await Promise.all([
      nativeCustomerAccessDetail(actor, id),
      nativeCustomerAccessInvitationHistory(actor, id),
    ]);
    const access = nativeAccess ?? (await customerAccessHistory(actor, id));
    data = { ...access, ...invitations } as typeof data;
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
