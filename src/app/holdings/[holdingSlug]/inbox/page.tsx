import Link from "next/link";
import { holdingResources } from "@/server/runtime";
import { listStaffNotifications } from "@/server/staff-notifications";
import { ReadNotification } from "@/components/modules/staff-work-controls";
import { PageHeader, EmptyState } from "@/components/ui/primitives";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import styles from "@/components/workplace/pages.module.css";
import { MessagingInbox } from "@/components/messaging/inbox";
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
        initialTab={query.view || query.tab === "updates" ? "updates" : "chats"}
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
            <section className="card">
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
                visible.map((n) => (
                  <article className="row" key={n.id}>
                    <div className="row-main">
                      <Link href={`${base}${n.href}`}>
                        <strong>{n.message}</strong>
                      </Link>
                      <p className="muted">
                        {new Date(n.created_at).toLocaleString("en-GB", {
                          timeZone: "Asia/Bangkok",
                        })}
                      </p>
                    </div>
                    {!n.read_at && <ReadNotification base={base} id={n.id} />}
                  </article>
                ))
              )}
            </section>
          </>
        }
      />
    </main>
  );
}
