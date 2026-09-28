import { FeaturePage } from "@/components/features/feature-page";
import { MessagingInbox } from "@/components/messaging/inbox";
import { InboxUpdates } from "@/components/messaging/inbox-updates";
import { PageHeader } from "@/components/ui/primitives";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import {holdingResources} from "@/server/runtime";
import {inboxSummary} from "@/server/inbox";

export default async function Inbox({params,searchParams}:PageProps<"/holdings/[holdingSlug]/inbox">){
  const actor=await requireWorkContext((await params).holdingSlug),base=workspaceBase(publicScope(actor)),query=await searchParams;
  const {write}=await holdingResources(actor.holdingId),summary=await inboxSummary(write,actor);
  const tab=query.view||query.tab==="updates"?"updates":query.tab==="teams"?"teams":"chats";
  return <FeaturePage className="content" data-source="live">
    <PageHeader title="Your inbox" description="Conversations, notices, and updates from your work."/>
    <MessagingInbox key={tab} base={base} userId={actor.userId} initialTab={tab}
      initialPerson={typeof query.person==="string"?query.person:undefined}
      initialRoom={typeof query.room==="string"?query.room:undefined}
      notificationUnread={summary.unread}
      updates={tab==="updates"?<InboxUpdates base={base} canBroadcast={hasPermission(actor,"inbox.notice.broadcast")}/>:null}/>
  </FeaturePage>;
}
