import Link from "next/link";
import { StatusBadge, EmptyState } from "@/components/ui/primitives";
import { workTypes, workItemHref, type WorkItem } from "@/lib/work-item";
import { workBlocker } from "@/lib/work-attention";
import { WorkFlow } from "./work-flow";
import styles from "./work-list.module.css";
export function WorkList({
  base,
  items,
  returnTo,
  queue = "all",
  filtered = false,
  waiting = 0,
  incomplete = false,
}: {
  base: string;
  items: WorkItem[];
  returnTo: string;
  queue?: string;
  filtered?: boolean;
  waiting?: number;
  incomplete?: boolean;
}) {
  return (
    <section className={styles.list} aria-label="Work results">
      {!items.length ? (
        <EmptyState
          title={incomplete ? "Work is temporarily unavailable" : filtered ? "No matching work" : queue === "todo" ? "No actions for you right now" : queue === "waiting" ? "Nothing waiting" : queue === "completed" ? "No completed work yet" : "Your work starts here"}
          description={incomplete ? "Retry above to check for missing work." : filtered ? "Try another search or clear the filters." : queue === "todo" && waiting > 0 ? `${waiting} ${waiting === 1 ? "item is" : "items are"} with others. You can follow their progress in Waiting.` : queue === "todo" ? "You’re all caught up." : queue === "completed" ? "Finished work will appear here." : queue === "waiting" ? "Work with other people will appear here." : "Use New to start a request or task."}
          action={!filtered && !incomplete && queue === "todo" && waiting > 0 ? <Link className="button secondary" href={`${base}/work?view=waiting`}>View waiting work</Link> : undefined}
        />
      ) : (
        items.map((item) => (
          <Link
            className={styles.row}
            key={`${item.type}:${item.id}`}
            href={workItemHref(base, item, returnTo)}
          >
            <div className={styles.body}>
              <small className={styles.type}>
                {workTypes.find(([id]) => id === item.type)?.[1]}
              </small>
              <strong className={styles.title}>{item.title}</strong>
              <p className={styles.action}>{item.nextAction ?? item.detail}</p>
              <WorkFlow item={item} />
              {item.dueAt && (
                <small className={styles.due}>
                  Due{" "}
                  {new Date(item.dueAt).toLocaleDateString("en-GB", {
                    timeZone: "Asia/Bangkok",
                  })}
                </small>
              )}
              {workBlocker(item) && item.queue !== "completed" && <p className={styles.blocker}>Needs attention: {workBlocker(item)}</p>}
            </div>
            <StatusBadge status={item.status} />
          </Link>
        ))
      )}
    </section>
  );
}
