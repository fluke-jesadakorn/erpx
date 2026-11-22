import {notificationWorkspace} from "@erpx/server-collaboration/public/notification-presentation";
import {nativeInboxAvailable,nativeInboxNotificationsSSR,nativeInboxSummarySSR} from "@erpx/server-collaboration/public/native-inbox-page";
import {holdingResources} from "@erpx/server-runtime/public/runtime";
import {readCompanyPurchaseFacts} from "@erpx/server-commercial/public/package-grants";
import {purchasedCapability} from "@erpx/server-commercial/public/rules/packages";
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
/** Native read behind the same availability gate as the feature-apps proxy:
 * the unread page plus the summary unread, with the presenter's
 * notificationsOnly gate computed on the same DB with the same functions.
 * The TS presenter remains the fallback when native is unavailable. */
async function nativeWorkspace(actor: Awaited<ReturnType<typeof requireWorkContext>>, holdingSlug: string, productHost: boolean) {
    // Same unread page the TS presenter reads: view=unread, default limit 30 server-side.
    const [page, summary, facts] = await Promise.all([
        nativeInboxNotificationsSSR(holdingSlug, "?view=unread"),
        nativeInboxSummarySSR(holdingSlug),
        (async () => {
            if (!productHost) return null;
            const { write } = await holdingResources(actor.holdingId);
            return readCompanyPurchaseFacts(write, actor.companyId);
        })(),
    ]);
    const notificationsOnly = !!facts && !purchasedCapability(facts.grants, actor.companyId, "work.today", "history", Date.now(), facts.history).allowed;
    return { initial: { items: page.items, nextCursor: page.nextCursor, unread: summary.unread }, notificationsOnly };
}
export default async function Inbox({ params, searchParams }: {
    params: Promise<{
        holdingSlug: string;
    }>;
    searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
    const actor = await requireWorkContext((await params).holdingSlug), base = workspaceBase(publicScope(actor)), query = await searchParams;
    const productHost=productHostForHost((await headers()).get("host"))==="apps";
    const data=nativeInboxAvailable()?await nativeWorkspace(actor,(await params).holdingSlug,productHost):await notificationWorkspace(actor,productHost);
    const tab="updates",key=JSON.stringify([actor.userId,actor.companyId,actor.assignmentId,actor.authority?.revision,actor.authority?.assignmentVersion]);
    const updates=<InboxUpdates key={key} base={base} canBroadcast={!data.notificationsOnly&&hasPermission(actor,"inbox.notice.broadcast")} notificationsOnly={data.notificationsOnly} initial={data.initial}/>;
    return <PageContent as={FeaturePage} data-source="live">
    <PageHeader appearance="standard" title={data.notificationsOnly?"Notifications":"Your inbox"} description={data.notificationsOnly?"Updates from the work you can currently access.":"Conversations, notices, and updates from your work."}/>
    {data.notificationsOnly?updates:<MessagingInbox key={tab} base={base} userId={actor.userId} initialTab={tab} initialPerson={typeof query.person === "string" ? query.person : undefined} initialRoom={typeof query.room === "string" ? query.room : undefined} notificationUnread={data.initial.unread} updates={updates}/>}
  </PageContent>;
}
