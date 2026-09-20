import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { PageHeader } from "@/components/ui/primitives";
import { FlowPanel,documentFlow } from "@/components/workplace/flow-panel";
import { RecordNextAction } from "@/components/workplace/record-next-action";
import { RequestAi } from "@/components/workplace/request-ai";
import {
RequestActions,
RequestEditor,
RequestFiles,
RequestHistory,
RequestState,
} from "@/components/workplace/request-controls";
import styles from "@/components/workplace/services.module.css";
import { approvalAdminHref } from "@/lib/administration";
import { categoryLabel,requestKinds } from "@/lib/employee-services";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { commercePermissions } from "@/server/commerce";
import { employeeRequestDetail } from "@/server/employee-requests";
import { pageError } from "@/server/page-error";
import { requestPageOptions } from "@/server/request-page";
import { holdingResources } from "@/server/runtime";
import Link from "next/link";
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
    if (!["view", "q", "queue", "type", "responsibility"].includes(key))
      back.delete(key);
  const latestMessage = [...detail.events].reverse().find((e) => e.note);
  const flow = await documentFlow(actor,"request",id);
  const quotation = row.kind === "purchase" && commercePermissions(actor).purchase ? await write.prepare("SELECT id FROM procurement_rfqs WHERE request_id=? AND company_id=? AND status<>'cancelled'").bind(id,actor.companyId).first<{id:string}>() : null;
  return (
    <FeaturePage className={`content ${styles.stack}`} data-source="live">
      <Link href={`${base}/work${back.size ? `?${back}` : ""}`}>
        ← Back to Work
      </Link>
      <PageHeader
        title={row.title || "Untitled request"}
        description={`${requestKinds.find(([id]) => id === row.kind)?.[1]} · ${categoryLabel(row.category)}`}
        action={<RequestState state={row.state} />}
      /><FeatureTabs/><WidgetGrid pageKey="requests-detail">
      <FlowPanel flow={flow} actor={actor} kind="request" id={id} base={base}/>
      {row.kind === "purchase" && <FeatureWidget id="requests-detail-purchase-progress" title="Purchase progress"><section className="card"><ol><li>Approval · Spending authorization under Company purchasing policy</li><li>Purchasing · {row.purchase_order_id ? "Purchase order linked" : "Awaiting purchase order"}</li><li>Fulfilment · {row.state === "completed" ? "Completed" : "Follow receipt and handover with the assigned team"}</li></ol><p>Approval authorizes the request. Ordering, receipt and payment are recorded separately.</p></section></FeatureWidget>}
      <section data-critical-notice><RecordNextAction flow={flow}/></section><RequestActions
        base={base}
        detail={{...detail,actions:detail.actions.filter(a=>flow.canAct||["edit","cancel","repair","comment","resource-request"].includes(a))}}
        userId={actor.userId}
        companyId={actor.companyId}
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
                A company owner can resolve this under Projects & workflows →
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
        <FeatureWidget id="requests-detail-request-details" title="Request details"><section className="card">
          
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
        </section></FeatureWidget>
      )}
      {latestMessage && (
        <FeatureWidget id="requests-detail-latest-message" title="Latest message"><section className="card">
          
          <p className={styles.article}>{latestMessage.note}</p>
        </section></FeatureWidget>
      )}
      <RequestAi
        base={base}
        id={id}
        version={row.version}
        kind={row.kind}
        editable={detail.actions.includes("edit")}
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
          <FeatureWidget id="requests-detail-purchasing-handoff" title="Purchasing handoff"><section className="card">
            
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
                    : quotation ? `${base}/procurement-rfqs/${quotation.id}` : `${base}/commerce?view=purchase&request=${row.id}`
                }
              >
                {row.purchase_order_id
                  ? "Open linked purchase order"
                  : quotation ? "Continue awarded quotation handoff" : "Prepare purchase order in Purchasing"}
              </Link>
            ) : (
              <p>
                Your assigned team will coordinate the purchase order and
                fulfillment.
              </p>
            )}
            {!actor.beta && !row.purchase_order_id && commercePermissions(actor).purchase && detail.actions.includes("link-order") && <p><Link href={quotation?`${base}/procurement-rfqs/${quotation.id}`:`${base}/procurement-rfqs?request=${row.id}`}>{quotation?"Continue supplier quotation review":"Compare supplier quotations before ordering"}</Link></p>}
            <p className={styles.quiet}>
              Existing purchasing, stock, and finance permissions apply.
            </p>
          </section></FeatureWidget>
        )}
      {(detail.actions.includes("resource-request") ||
        row.parent_request_id ||
        detail.children.length > 0) && (
        <FeatureWidget id="requests-detail-related-requests" title="Related requests"><section className="card">
          
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
        </section></FeatureWidget>
      )}
      <RequestHistory detail={detail} />
    </WidgetGrid></FeaturePage>
  );
}
