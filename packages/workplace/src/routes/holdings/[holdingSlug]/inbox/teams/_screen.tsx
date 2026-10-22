import { collaborationPresentationQueries } from "@erpx/server-collaboration/public/presentation-queries";
import { PageContent } from "@erpx/ui/components/patterns";
import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import { MessagingInbox } from "@erpx/feature-collaboration/components/messaging/inbox";
import { PageHeader } from "@erpx/ui/components/primitives";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
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
