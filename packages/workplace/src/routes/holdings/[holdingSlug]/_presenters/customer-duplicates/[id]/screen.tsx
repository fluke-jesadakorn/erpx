import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { Table } from "@erpx/ui/components/foundation";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { customerDuplicateDetail } from "@erpx/server-customer_authority/public/customer-duplicates";
import { CustomerDuplicateConsole } from "@erpx/feature-records/components/loaders/modules--customer-duplicate-console";
import { PageContent, Card, TableFrame } from "@erpx/ui/components/foundation";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params,
    a = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(a)),
    data = await customerDuplicateDetail(a, id).catch((e) => pageError(e, holdingSlug)),
    r = data.row;
  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={canonicalUiHref(`${base}/customer-duplicates`)}>Duplicate reviews</Link>
      <h1>
        {String(r.left_name)} / {String(r.right_name)}
      </h1>
      <p>
        {String(r.status)}
        {r.canonical_customer_id ? ` · canonical ${String(r.canonical_customer_id)}` : ""}
        {r.merged_into_id ? ` · merged into ${String(r.merged_into_id)}` : ""}
      </p>
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Compare customer records</h2>
        <TableFrame appearance="standard" label="Records"><Table>
          <caption>Customer identity comparison</caption>
          <thead>
            <tr>
              <th scope="col">Identity field</th>
              <th scope="col">{String(r.left_name)}</th>
              <th scope="col">{String(r.right_name)}</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <th scope="row">Customer code</th>
              <td>{String(r.left_code || "Not provided")}</td>
              <td>{String(r.right_code || "Not provided")}</td>
            </tr>
            <tr>
              <th scope="row">Tax registration</th>
              <td>{String(r.left_tax || "Not provided")}</td>
              <td>{String(r.right_tax || "Not provided")}</td>
            </tr>
            <tr>
              <th scope="row">Record status</th>
              <td>{String(r.left_status)}</td>
              <td>{String(r.right_status)}</td>
            </tr>
          </tbody>
        </Table></TableFrame>
        <h3>Submitted evidence</h3>
        <p>{String(r.evidence)}</p>
      </Card>
      <CustomerDuplicateConsole key={`${r.id}:${r.version}`} base={base} detail={data} />
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Immutable decision history</h2>
        {data.history.map((e) => (
          <p key={String(e.version)}>
            Version {String(e.version)} · {String(e.action)} · {String(e.reason)}
          </p>
        ))}
      </Card>
    </PageContent>
  );
}
