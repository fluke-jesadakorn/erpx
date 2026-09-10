import Link from "next/link";
import type { WorkContext } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { serviceOwner } from "@/server/service-access";
import {
  listEmployeeRequests,
  employeeRequestDetail,
} from "@/server/employee-requests";
import { RequestRepair } from "./service-teams";
import { approvalAdminHref } from "@/lib/administration";
import { resourceRequestRoute } from "@/server/expense-routing";
export async function RequestAdministration({
  actor,
  base,
  record,
}: {
  actor: WorkContext;
  base: string;
  record?: string;
}) {
  const { write } = await holdingResources(actor.holdingId);
  if (!(await serviceOwner(write, actor))) return null;
  const rows = record
    ? [(await employeeRequestDetail(write, actor, record)).request]
    : (await listEmployeeRequests(write, actor)).filter(
        (r) =>
          !["draft", "returned", "closed", "cancelled", "rejected"].includes(
            r.state,
          ),
      );
  const details = await Promise.all(
    rows.map((r) => employeeRequestDetail(write, actor, r.id)),
  );
  const issues = record
    ? details
    : details.filter(
        (d) => d.blockedReason || d.request.state === "awaiting_approval",
      );
  return (
    <section className="card">
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
      {await Promise.all(
        issues.map(async (d) => {
          const route =
            d.request.kind !== "service"
              ? await resourceRequestRoute(
                  write,
                  actor.companyId,
                  d.request.requester_user_id,
                  d.request.created_by,
                )
              : null;
          return (
            <article className="card" key={d.request.id}>
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
            </article>
          );
        }),
      )}
    </section>
  );
}
