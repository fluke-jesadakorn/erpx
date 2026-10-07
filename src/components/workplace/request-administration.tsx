import Link from "next/link";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { requestAdministrationData } from "@/server/modules/records/public/request-administration";
import { RequestRepair } from "./service-teams";
import { approvalAdminHref } from "@/lib/administration";
import { Card } from "@/components/ui/foundation";
export async function RequestAdministration({
  actor,
  base,
  record,
}: {
  actor: WorkContext;
  base: string;
  record?: string;
}) {
  const issues = await requestAdministrationData(actor, record);
  if (!issues) return null;
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>Employee request routing</h2>
      <p>
        Resolve missing service coverage, unavailable handlers, or independent
        department approval.
      </p>
      <p>
        <Link href={`${base}/settings?section=organization#service-teams`}>
          Configure service teams and coverage
        </Link>
      </p>
      {!issues.length && (
        <p>No employee request routing issues in this scope.</p>
      )}
      {issues.map((d) => {
        const route = d.route;
        return (
          <Card as="article" appearance="standard" className="ui-panel" key={d.request.id}>
            <h3>
              <Link href={`${base}/requests/${d.request.id}`}>
                {d.request.title || "Untitled request"}
              </Link>
            </h3>
            <p>{d.blockedReason ?? d.nextAction}</p>
            {record ? (
              <RequestRepair
                base={base}
                id={d.request.id}
                version={d.request.version}
                approvers={route?.steps[0]?.candidateUserIds ?? []}
                companyId={actor.companyId}
                userId={actor.userId}
              />
            ) : (
              <Link href={approvalAdminHref(base, "request", d.request.id)}>
                Resolve routing issue
              </Link>
            )}
          </Card>
        );
      })}
    </Card>
  );
}
