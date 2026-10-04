import { RecordSections } from "@/components/workplace/record-sections";
import { InlineNotice } from "@/components/ui/foundation";

import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { FeaturePage, FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { PageHeader } from "@/components/ui/primitives";
import { FlowPanel,documentFlow } from "@/components/workplace/flow-panel";
import { RecordNextAction } from "@/components/workplace/record-next-action";
import { PurchaseRequestProgress } from "@/components/workplace/purchase-request-progress";
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
import { purchaseRequestProgress } from "@/server/purchase-request-progress";
import { pageError } from "@/server/page-error";
import { requestPageOptions } from "@/server/request-page";
import { holdingResources } from "@/server/runtime";
import Link from "next/link";
import { Card } from "@/components/ui/foundation";
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
  const purchaseProgress = row.kind === "purchase"
    ? await purchaseRequestProgress(write, actor, row.id)
    : null;
  const back = new URLSearchParams(query.return ?? "");
  for (const key of [...back.keys()])
    if (!["view", "q", "queue", "type", "responsibility", "offset", "tracking"].includes(key))
      back.delete(key);
  const latestMessage = [...detail.events].reverse().find((e) => e.note);
  const flow = await documentFlow(actor,"request",id);
  const savedApproval = JSON.parse(row.approval_json) as {route?:{steps?:{userId:string}[]};decisions?:{userId?:string;originalApproverUserId?:string;actualApproverUserId?:string}[]};
  const approverIds = [...new Set([...(savedApproval.route?.steps??[]).map(step=>step.userId),...(savedApproval.decisions??[]).flatMap(decision=>[decision.userId,decision.originalApproverUserId,decision.actualApproverUserId].filter((id):id is string=>!!id))])];
  const approverNames = Object.fromEntries(await Promise.all(approverIds.map(async userId=>{
    const person=await write.prepare("SELECT legal_name FROM hr_people WHERE company_id=? AND iam_same_person(user_id,?,company_id) AND merged_into_id IS NULL LIMIT 1").bind(actor.companyId,userId).first<{legal_name:string}>();
    return [userId,person?.legal_name??userId] as const;
  })));
  const quotation = row.kind === "purchase" && commercePermissions(actor).purchase ? await write.prepare("SELECT id FROM procurement_rfqs WHERE request_id=? AND company_id=? AND status<>'cancelled'").bind(id,actor.companyId).first<{id:string}>() : null;
  return (
    <FeaturePage className={`content ${styles.stack}`} data-source="live">
      <Link href={`${base}/work${back.size ? `?${back}` : ""}`}>
        ← Back to Work
      </Link>
      <PageHeader appearance="standard"
        title={row.title || "Untitled request"}
        description={`${requestKinds.find(([id]) => id === row.kind)?.[1]} · ${categoryLabel(row.category)}`}
        action={<RequestState state={row.state} />}
      /><RecordSections evidence={<RequestFiles base={base} detail={detail} />} history={<RequestHistory detail={detail} approverNames={approverNames} />}><WidgetGrid pageKey="requests-detail">
      <FlowPanel flow={flow} actor={actor} kind="request" id={id} base={base}/>
      {purchaseProgress && <FeatureWidget appearance="standard" id="requests-detail-purchase-progress" title="Purchase-to-payment progress"><PurchaseRequestProgress base={base} progress={purchaseProgress}/></FeatureWidget>}
      <section data-critical-notice><RecordNextAction flow={flow}/></section><RequestActions
        base={base}
        detail={{...detail,actions:detail.actions.filter(a=>flow.canAct||["edit","cancel","repair","comment","resource-request"].includes(a))}}
        userId={actor.userId}
        companyId={actor.companyId}
      />
      <Card as="section" appearance="standard" className="ui-panel">
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
          <InlineNotice tone="info" role="status">
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
          </InlineNotice>
        )}
      </Card>
      {detail.actions.includes("edit") ? (
        <RequestEditor
          base={base}
          kind={row.kind}
          options={await requestPageOptions(write, actor)}
          request={row}
          returnTo={back.toString()}
        />
      ) : (
        <FeatureWidget appearance="standard" id="requests-detail-request-details" title="Request details"><Card as="section" appearance="standard" className="ui-panel">

          <p className={styles.article}>{row.description}</p>
          {detail.items.map((item, i) => (
            <p key={i}>
              {item.description} · {item.quantity} {item.unit ?? "each"}
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
        </Card></FeatureWidget>
      )}
      {latestMessage && (
        <FeatureWidget appearance="standard" id="requests-detail-latest-message" title="Latest message"><Card as="section" appearance="standard" className="ui-panel">

          <p className={styles.article}>{latestMessage.note}</p>
        </Card></FeatureWidget>
      )}
      <details><summary>Ask AI about this request</summary><RequestAi
        base={base}
        id={id}
        version={row.version}
        kind={row.kind}
        editable={detail.actions.includes("edit")}
      /></details>

      {row.kind === "purchase" &&
        ![
          "draft",
          "returned",
          "awaiting_approval",
          "blocked",
          "rejected",
          "cancelled",
        ].includes(row.state) && (
          <FeatureWidget appearance="standard" id="requests-detail-purchasing-handoff" title="Purchasing handoff"><Card as="section" appearance="standard" className="ui-panel">

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
          </Card></FeatureWidget>
        )}
      {(detail.actions.includes("resource-request") ||
        row.parent_request_id ||
        detail.children.length > 0) && (
        <FeatureWidget appearance="standard" id="requests-detail-related-requests" title="Related requests"><Card as="section" appearance="standard" className="ui-panel">

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
              <UiActionLink appearance="standard" variant="secondary"

                href={`${base}/work?new=equipment_request&from=${row.id}`}
              >
                Request equipment
              </UiActionLink>
              <UiActionLink appearance="standard" variant="secondary"

                href={`${base}/work?new=purchase_request&from=${row.id}`}
              >
                Request purchase
              </UiActionLink>
            </div>
          )}
        </Card></FeatureWidget>
      )}
    </WidgetGrid></RecordSections></FeaturePage>
  );
}
