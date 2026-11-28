import { collaborationPresentationQueries } from "@erpx/collaboration-server/public/presentation-queries";
import { PageContent } from "@erpx/platform-ui/components/patterns";
import { FeaturePage } from "@erpx/platform-shared-ui/components/features/feature-page";
import { MessagingInbox } from "@erpx/collaboration-ui/components/messaging/inbox";
import { PageHeader } from "@erpx/platform-ui/components/primitives";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/identity-access-server/public/authorization";
export default async function Inbox({ params, searchParams }: {
    params: Promise<{
        holdingSlug: string;
    }>;
    searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
    const actor = await requireWorkContext((await params).holdingSlug), base = workspaceBase(publicScope(actor)), query = await searchParams;
    const presentation = await collaborationPresentationQueries(actor.holdingId), summary = await presentation.inboxSummary(actor);
    const tab = query.view || false ? "updates" : "teams";
    const workspace=await presentation.messagingWorkspace(actor,typeof query.room==='string'?query.room:undefined);
    return <PageContent as={FeaturePage} data-source="live">
    <PageHeader appearance="standard" title="Your inbox" description="Conversations, notices, and updates from your work."/>
    <MessagingInbox key={workspace.context+tab} base={base} userId={actor.userId} contextToken={workspace.context} initialOverview={workspace.overview} initialHistory={workspace.history} initialTab={tab} initialPerson={typeof query.person === "string" ? query.person : undefined} initialRoom={typeof query.room === "string" ? query.room : undefined} notificationUnread={summary.unread} updates={null}/>
  </PageContent>;
}
