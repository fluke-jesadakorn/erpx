import { collaborationPresentationQueries } from "@/server/modules/collaboration/public/presentation-queries";

import { PageContent } from "@/components/ui/patterns";
import { FeaturePage } from "@/components/shared/features/feature-page";
import { MessagingInbox } from "@/components/collaboration/messaging/inbox";
import { InboxUpdates } from "@/components/collaboration/messaging/inbox-updates";
import { PageHeader } from "@/components/ui/primitives";
import { hasPermission } from "@/lib/identity_access/permissions";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";



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
      updates={tab==="updates"?<InboxUpdates base={base} canBroadcast={hasPermission(actor,"inbox.notice.broadcast")}/>:null}/>
  </PageContent>;
}
