import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";

import { PageContent } from "@erpx/ui/components/patterns";
import Link from "@erpx/workplace/components/workspace-link";
import { notFound } from "next/navigation";
import { publicScope } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { MemberProfile } from "@erpx/feature-identity_access/components/member-profile";
import { companyMembers } from "@erpx/server-organization/public/member-directory";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { Card } from "@erpx/ui/components/foundation";
export async function BetaPeople({
  actor,
  member,
}: {
  actor: WorkContext;
  member?: string;
}) {
  const people = await companyMembers(actor),
    base = workspaceBase(publicScope(actor));
  const selected = member ? people.find((p) => p.id === member) : null;
  if (member && !selected) notFound();
  return (
    <PageContent >
      <p className="eyebrow">Better together</p>
      <h1>Your people</h1>
      <p>The teammates you can work with in this company.</p>
      <Card as="section" appearance="standard" className="ui-panel">
        {people.map((p) => (
          <Link
            className="ui-record-item"
            key={p.id}
            href={canonicalUiHref(`${base}/profiles/${encodeURIComponent(p.id)}`)}
          >
            <span className="avatar">{p.name.slice(0, 1)}</span>
            <strong>{p.name}</strong>
          </Link>
        ))}
      </Card>
      {selected && (
        <MemberProfile base={base} actor={actor} member={selected} />
      )}
    </PageContent>
  );
}
