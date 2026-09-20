import { FeaturePage, FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { MessagingInbox } from "@/components/messaging/inbox";
import { ReadNotification } from "@/components/modules/staff-work-controls";
import { ListPagination } from "@/components/ui/list-pagination";
import { EmptyState,PageHeader } from "@/components/ui/primitives";
import updateStyles from "@/components/workplace/inbox-updates.module.css";
import { notificationDays } from "@/lib/notification-days";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { notificationsPage } from "@/server/staff-notifications";
import { MessageSquare } from "lucide-react";
import Link from "next/link";
export default async function Inbox({
  params,
  searchParams,
}: PageProps<"/holdings/[holdingSlug]/inbox">) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor));
  const query = await searchParams;
  const tab = query.view || query.tab === "updates" ? "updates" : query.tab === "teams" ? "teams" : "chats";
  return (
    <FeaturePage className="content" data-source="live">
      <PageHeader
        title="Your inbox"
        description="Connect with people, teams, and the work you’re part of."
      />
      <MessagingInbox
        key={tab}
        base={base}
        userId={actor.userId}
        initialTab={tab}
        initialPerson={typeof query.person === "string"?query.person:undefined}
        initialRoom={typeof query.room === "string"?query.room:undefined}
        updates={tab==="updates"?<WidgetGrid pageKey="inbox-updates">{await Promise.all((["unread","all"] as const).map(async view=>{
          const page=await notificationsPage((await holdingResources(actor.holdingId)).write,actor,{view,cursor:typeof query[`${view}.cursor`]==="string"?query[`${view}.cursor`] as string:query.view===view&&typeof query.cursor==="string"?query.cursor:undefined});
          return <FeatureWidget key={view} id={`inbox-${view}`} title={view==="unread"?"Unread notifications":"All notifications"}>
            {!page.items.length?<EmptyState title={view==="unread"?"You’re all caught up":"No notifications yet"} description="Updates appear here when there is something to follow up."/>:notificationDays(page.items).map(day=><section className={updateStyles.day} key={day.key} aria-label={day.label}><h3>{day.label}</h3>{day.items.map(n=><article className={updateStyles.row} key={n.id} data-unread={!n.read_at}><MessageSquare size={20}/><div className={updateStyles.body}><Link href={`${base}${n.href}`}>{n.message}</Link><time dateTime={new Date(n.created_at).toISOString()}>{new Date(n.created_at).toLocaleString("en-GB",{timeZone:"Asia/Bangkok"})}</time></div>{!n.read_at&&<ReadNotification base={base} id={n.id}/>}</article>)}</section>)}
            <ListPagination {...page} query={query} cursorName={`${view}.cursor`} count={page.items.length}/>
          </FeatureWidget>;
        }))}</WidgetGrid>:null}
      />
    </FeaturePage>
  );
}
