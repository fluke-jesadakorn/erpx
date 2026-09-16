import Link from "next/link";
import { holdingResources } from "@/server/runtime";
import { listStaffNotifications } from "@/server/staff-notifications";
import { ReadNotification } from "@/components/modules/staff-work-controls";
import { PageHeader, EmptyState } from "@/components/ui/primitives";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import styles from "@/components/workplace/pages.module.css";
import { MessagingInbox } from "@/components/messaging/inbox";
import { notificationDays } from "@/lib/notification-days";
import { CalendarDays, ClipboardList, MessageSquare, Receipt, FileText } from "lucide-react";
import updateStyles from "@/components/workplace/inbox-updates.module.css";
export default async function Inbox({
  params,
  searchParams,
}: PageProps<"/holdings/[holdingSlug]/inbox">) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor));
  const { write } = await holdingResources(actor.holdingId),
    notifications = await listStaffNotifications(write, actor);
  const query = await searchParams;
  const view = query.view === "all" ? "all" : "unread",
    visible = notifications.filter((n) => view === "all" || !n.read_at);
  return (
    <main className="content" data-source="live">
      <PageHeader
        title="Your inbox"
        description="Connect with people, teams, and the work you’re part of."
      />
      <MessagingInbox
        base={base}
        userId={actor.userId}
        initialTab={query.view || query.tab === "updates" ? "updates" : query.tab === "teams" ? "teams" : "chats"}
        initialPerson={typeof query.person === "string"?query.person:undefined}
        initialRoom={typeof query.room === "string"?query.room:undefined}
        updates={
          <>
            <nav className={styles.tabs} aria-label="Inbox views">
              <Link
                href="?tab=updates&view=unread"
                aria-current={view === "unread" ? "page" : undefined}
              >
                Unread{" "}
                <span>{notifications.filter((n) => !n.read_at).length}</span>
              </Link>
              <Link
                href="?tab=updates&view=all"
                aria-current={view === "all" ? "page" : undefined}
              >
                All
              </Link>
            </nav>
            <section className={updateStyles.updates} aria-label="Work updates">
              {!visible.length ? (
                <EmptyState
                  title={
                    view === "unread"
                      ? "You’re all caught up"
                      : "No notifications yet"
                  }
                  description="Updates will appear here when there is something to follow up."
                />
              ) : (
                notificationDays(visible).map(day => <section className={updateStyles.day} key={day.key} aria-label={day.label}>
                  <h2>{day.label}</h2><div className={updateStyles.group}>{day.items.map(n => {
                    const Icon = n.record_type === "expense" ? Receipt : n.record_type === "leave" ? CalendarDays : n.record_type === "post" ? MessageSquare : n.record_type === "task" ? ClipboardList : FileText;
                    return <article className={updateStyles.row} key={n.id} data-unread={!n.read_at}>
                      <Icon size={20} aria-hidden />
                      <div className={updateStyles.body}>
                        <Link href={`${base}${n.href}`}>{n.message}</Link>
                        <time dateTime={new Date(n.created_at).toISOString()}>{new Date(n.created_at).toLocaleTimeString("en-GB", { timeZone: "Asia/Bangkok", hour: "2-digit", minute: "2-digit" })}</time>
                      </div>
                      {!n.read_at && <ReadNotification base={base} id={n.id} />}
                    </article>;
                  })}</div>
                </section>)
              )}
            </section>
          </>
        }
      />
    </main>
  );
}
