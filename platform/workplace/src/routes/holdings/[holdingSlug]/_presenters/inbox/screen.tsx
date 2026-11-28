import { collaborationPresentationQueries } from "@erpx/collaboration-server/public/presentation-queries";

import { PageContent } from "@erpx/platform-ui/components/patterns";
import { FeaturePage } from "@erpx/platform-shared-ui/components/features/feature-page";
import { MessagingInbox } from "@erpx/collaboration-ui/components/messaging/inbox";
import { InboxUpdates } from "@erpx/collaboration-ui/components/messaging/inbox-updates";
import { PageHeader } from "@erpx/platform-ui/components/primitives";
import { hasPermission } from "@erpx/identity-access-ui/lib/permissions";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/identity-access-server/public/authorization";



export default async function Inbox({params,searchParams}:{ params: Promise<{ holdingSlug: string;  }>; searchParams: Promise<Record<string,string|string[]|undefined>> }){
  const actor=await requireWorkContext((await params).holdingSlug),base=workspaceBase(publicScope(actor)),query=await searchParams;
  const presentation=await collaborationPresentationQueries(actor.holdingId),summary=await presentation.inboxSummary(actor);
  const tab=query.view||query.tab==="updates"?"updates":query.tab==="teams"?"teams":"chats";
  return <PageContent as={FeaturePage} data-source="live">
    <PageHeader appearance="standard" title="Your inbox" description="Conversations, notices, and updates from your work."/>
    <MessagingInbox key={tab} base={base} userId={actor.userId} initialTab={tab}
      initialPerson={typeof query.person==="string"?query.person:undefined}
      initialRoom={typeof query.room==="string"?query.room:undefined}
      notificationUnread={summary.unread}
      updates={tab==="updates"?<InboxUpdates key={JSON.stringify([actor.userId,actor.companyId,actor.assignmentId,actor.authority?.revision,actor.authority?.assignmentVersion])} base={base} canBroadcast={hasPermission(actor,"inbox.notice.broadcast")}/>:null}/>
  </PageContent>;
}
