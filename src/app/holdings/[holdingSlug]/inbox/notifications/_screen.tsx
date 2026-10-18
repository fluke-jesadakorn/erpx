import {notificationWorkspace} from "@/server/modules/collaboration/public/notification-presentation";
import {headers} from "next/headers";
import {productHostForHost} from "@/lib/runtime/product-hosts";
import { PageContent } from "@/components/ui/patterns";
import { FeaturePage } from "@/components/shared/features/feature-page";
import { MessagingInbox } from "@/components/collaboration/messaging/inbox";
import { InboxUpdates } from "@/components/collaboration/messaging/inbox-updates";
import { PageHeader } from "@/components/ui/primitives";
import { hasPermission } from "@/lib/identity_access/permissions";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
export default async function Inbox({ params, searchParams }: {
    params: Promise<{
        holdingSlug: string;
    }>;
    searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
    const actor = await requireWorkContext((await params).holdingSlug), base = workspaceBase(publicScope(actor)), query = await searchParams;
    const data=await notificationWorkspace(actor,productHostForHost((await headers()).get("host"))==="work");
    const tab="updates",key=`${actor.companyId}:${actor.assignmentId}:${actor.authority?.revision??0}`;
    const updates=<InboxUpdates key={key} base={base} canBroadcast={!data.notificationsOnly&&hasPermission(actor,"inbox.notice.broadcast")} notificationsOnly={data.notificationsOnly} initial={data.initial}/>;
    return <PageContent as={FeaturePage} data-source="live">
    <PageHeader appearance="standard" title={data.notificationsOnly?"Notifications":"Your inbox"} description={data.notificationsOnly?"Updates from the work you can currently access.":"Conversations, notices, and updates from your work."}/>
    {data.notificationsOnly?updates:<MessagingInbox key={tab} base={base} userId={actor.userId} initialTab={tab} initialPerson={typeof query.person === "string" ? query.person : undefined} initialRoom={typeof query.room === "string" ? query.room : undefined} notificationUnread={data.initial.unread} updates={updates}/>}
  </PageContent>;
}
