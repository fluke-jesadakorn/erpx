import {notificationWorkspace} from "@erpx/server-collaboration/public/notification-presentation";
import {headers} from "next/headers";
import {productHostForHost} from "@erpx/feature-runtime/lib/product-hosts";
import { PageContent } from "@erpx/ui/components/patterns";
import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import { MessagingInbox } from "@erpx/feature-collaboration/components/messaging/inbox";
import { InboxUpdates } from "@erpx/feature-collaboration/components/messaging/inbox-updates";
import { PageHeader } from "@erpx/ui/components/primitives";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
export default async function Inbox({ params, searchParams }: {
    params: Promise<{
        holdingSlug: string;
    }>;
    searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
    const actor = await requireWorkContext((await params).holdingSlug), base = workspaceBase(publicScope(actor)), query = await searchParams;
    const productHost=productHostForHost((await headers()).get("host"))==="apps";
    // Native-only inbox read: the TS read fallback and the availability gate
    // are retired. An absent or broken native transport fails closed (503) in
    // the native client rather than serving stale TS SQL.
    const data=await notificationWorkspace(actor,productHost);
    const tab="updates",key=JSON.stringify([actor.userId,actor.companyId,actor.assignmentId,actor.authority?.revision,actor.authority?.assignmentVersion]);
    const updates=<InboxUpdates key={key} base={base} canBroadcast={!data.notificationsOnly&&hasPermission(actor,"inbox.notice.broadcast")} notificationsOnly={data.notificationsOnly} initial={data.initial}/>;
    return <PageContent as={FeaturePage} data-source="live">
    <PageHeader appearance="standard" title={data.notificationsOnly?"Notifications":"Your inbox"} description={data.notificationsOnly?"Updates from the work you can currently access.":"Conversations, notices, and updates from your work."}/>
    {data.notificationsOnly?updates:<MessagingInbox key={tab} base={base} userId={actor.userId} initialTab={tab} initialPerson={typeof query.person === "string" ? query.person : undefined} initialRoom={typeof query.room === "string" ? query.room : undefined} notificationUnread={data.initial.unread} updates={updates}/>}
  </PageContent>;
}
