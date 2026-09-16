import { FlowPanel, documentFlow } from "@/components/workplace/flow-panel";
import { commercePermissions } from "@/server/commerce";
import { pageError } from "@/server/page-error";
import { RequestAi } from "@/components/workplace/request-ai";
import Link from "next/link";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { workspaceBase } from "@/lib/workspace";
import { employeeRequestDetail } from "@/server/employee-requests";
import { requestPageOptions } from "@/server/request-page";
import {
  RequestActions,
  RequestEditor,
  RequestFiles,
  RequestHistory,
  RequestState,
} from "@/components/workplace/request-controls";
import { PageHeader } from "@/components/ui/primitives";
import { approvalAdminHref } from "@/lib/administration";
import { categoryLabel, requestKinds } from "@/lib/employee-services";
import styles from "@/components/workplace/services.module.css";
export default async function RequestPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
  searchParams: Promise<{ return?: string }>;
}) {
  const { holdingSlug, id } = await params,
    actor = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(actor)),
    { write } = await holdingResources(actor.holdingId),
    detail = await employeeRequestDetail(write, actor, id).catch(
      (e: unknown) => {
        return pageError(e, actor.holdingSlug);
      },
    ),
    row = detail.request,
    query = await searchParams;
  const back = new URLSearchParams(query.return ?? "");
  for (const key of [...back.keys()])
    if (!["q", "queue", "type", "responsibility"].includes(key))
      back.delete(key);
  const latestMessage = [...detail.events].reverse().find((e) => e.note);
  const flow = await documentFlow(actor,"request",id);
  return (
    <main className={`content ${styles.stack}`} data-source="live">
      <Link href={`${base}/work${back.size ? `?${back}` : ""}`}>
        ← Back to Work
      </Link>
      <PageHeader
        title={row.title || "Untitled request"}
        description={`${requestKinds.find(([id]) => id === row.kind)?.[1]} · ${categoryLabel(row.category)}`}
        action={<RequestState state={row.state} />}
      />
      <section className="card">
        <div className={styles.status}>
          <div>
            <small>Responsible queue</small>
            <strong>{detail.responsibleQueue}</strong>
            {detail.assigneeName && <p>{detail.assigneeName}</p>}
          </div>
          <div>
            <small>Next action</small>
            <strong>{detail.nextAction}</strong>
          </div>
          <div>
            <small>Location</small>
            <strong>{row.location || "Not set"}</strong>
          </div>
        </div>
        {detail.blockedReason && (
          <div role="status">
            <p>{detail.blockedReason}</p>
            {detail.actions.includes("repair") ? (
              <Link href={approvalAdminHref(base, "request", id)}>
                Resolve routing issue
              </Link>
            ) : (
              <p className={styles.quiet}>
                A company owner can resolve this under Administration →
                Approvals.
              </p>
            )}
            <p>
              <Link href={`${base}/people?member=${row.requester_user_id}`}>
                Review requester’s People setup
              </Link>
            </p>
          </div>
        )}
      </section>
      <RequestFiles base={base} detail={detail} />
      {detail.actions.includes("edit") ? (
        <RequestEditor
          base={base}
          kind={row.kind}
          options={await requestPageOptions(write, actor)}
          request={row}
          returnTo={back.toString()}
        />
      ) : (
        <section className="card">
          <h2>Request details</h2>
          <p className={styles.article}>{row.description}</p>
          {detail.items.map((item, i) => (
            <p key={i}>
              {item.description} · {item.quantity}
              {item.estimatedCost
                ? ` · Estimated ${row.currency} ${item.estimatedCost}`
                : ""}
            </p>
          ))}
          {row.needed_date && <p>Needed by {row.needed_date}</p>}
          {row.resolution && (
            <p className={styles.article}>
              <strong>Resolution / handover</strong>
              <br />
              {row.resolution}
            </p>
          )}
        </section>
      )}
      {latestMessage && (
        <section className="card">
          <h2>Latest message</h2>
          <p className={styles.article}>{latestMessage.note}</p>
        </section>
      )}
      <RequestAi
        base={base}
        id={id}
        version={row.version}
        kind={row.kind}
        editable={detail.actions.includes("edit")}
      />
      <RequestActions
        base={base}
        detail={{...detail,actions:detail.actions.filter(a=>flow.canAct||["edit","cancel","repair","comment","resource-request"].includes(a))}}
        userId={actor.userId}
        companyId={actor.companyId}
      />
      {row.kind === "purchase" &&
        ![
          "draft",
          "returned",
          "awaiting_approval",
          "blocked",
          "rejected",
          "cancelled",
        ].includes(row.state) && (
          <section className="card">
            <h2>Purchasing handoff</h2>
            {actor.beta ? (
              <p>
                Purchasing execution is unavailable in this beta workspace. Your
                approved request remains saved.
              </p>
            ) : commercePermissions(actor).purchase &&
              (row.purchase_order_id ||
                detail.actions.includes("link-order")) ? (
              <Link
                href={
                  row.purchase_order_id
                    ? `${base}/commerce/orders/${row.purchase_order_id}`
                    : `${base}/commerce?view=purchase&request=${row.id}`
                }
              >
                {row.purchase_order_id
                  ? "Open linked purchase order"
                  : "Prepare purchase order in Purchasing"}
              </Link>
            ) : (
              <p>
                Your assigned team will coordinate the purchase order and
                fulfillment.
              </p>
            )}
            <p className={styles.quiet}>
              Existing purchasing, stock, and finance permissions apply.
            </p>
          </section>
        )}
      {(detail.actions.includes("resource-request") ||
        row.parent_request_id ||
        detail.children.length > 0) && (
        <section className="card">
          <h2>Related requests</h2>
          {row.parent_request_id && (
            <Link href={`${base}/requests/${row.parent_request_id}`}>
              Original service request
            </Link>
          )}
          {detail.children.map((c) => (
            <p key={c.id}>
              <Link href={`${base}/requests/${c.id}`}>{c.title || c.kind}</Link>{" "}
              · <RequestState state={c.state} />
            </p>
          ))}
          {detail.actions.includes("resource-request") && (
            <div className={styles.actions}>
              <Link
                className="button secondary"
                href={`${base}/work?new=equipment_request&from=${row.id}`}
              >
                Request equipment
              </Link>
              <Link
                className="button secondary"
                href={`${base}/work?new=purchase_request&from=${row.id}`}
              >
                Request purchase
              </Link>
            </div>
          )}
        </section>
      )}
      <RequestHistory detail={detail} />
    <FlowPanel flow={flow} actor={actor} kind="request" id={id} base={base}/></main>
  );
}
