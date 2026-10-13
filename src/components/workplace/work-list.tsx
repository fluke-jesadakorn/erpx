import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { Button, RecordList, RecordListItem } from "@/components/ui/foundation";
import { RecordLink as Link } from "@/components/records/record-link";
import { StatusBadge, EmptyState } from "@/components/ui/primitives";
import { workTypes, workItemHref, isApprovalAction, type WorkItem } from "@/lib/work_projection/work-item";
import { supportsWorkReview } from "@/lib/work_projection/work-review";
import { workBlocker } from "@/lib/work_projection/work-attention";
import { WorkFlow } from "./work-flow";
import { operationFromWork } from "@/lib/shared/operation-presentation";
import { workTypeTone } from "@/lib/work_projection/work-category-tone";
import styles from "./work-list.module.css";
import { TimeText } from "@/components/people_payroll/time/time-preference-context";
export function WorkList({
  base,
  items,
  returnTo,
  queue = "all",
  filtered = false,
  waiting = 0,
  incomplete = false,
  onReview,
}: {
  base: string;
  items: WorkItem[];
  returnTo: string;
  queue?: string;
  filtered?: boolean;
  waiting?: number;
  incomplete?: boolean;
  onReview?: (item: WorkItem) => void;
}) {
  return (
    <RecordList as="section" className={styles.list} aria-label="Work results">
      {!items.length ? (
        <EmptyState
          title={
            incomplete
              ? "Work is temporarily unavailable"
              : filtered
                ? "No matching work"
                : queue === "todo"
                  ? "No actions for you right now"
                  : queue === "waiting"
                    ? "Nothing waiting"
                    : queue === "completed"
                      ? "No completed work yet"
                      : "Your work starts here"
          }
          description={
            incomplete
              ? "Retry above to check for missing work."
              : filtered
                ? "Try another search or clear the filters."
                : queue === "todo" && waiting > 0
                  ? `${waiting} ${waiting === 1 ? "item is" : "items are"} with others. You can follow their progress in Waiting.`
                  : queue === "todo"
                    ? "You’re all caught up."
                    : queue === "completed"
                      ? "Finished work will appear here."
                      : queue === "waiting"
                        ? "Work with other people will appear here."
                        : "Use New to start a request or task."
          }
          action={
            !filtered && !incomplete && queue === "todo" && waiting > 0 ? (
              <UiActionLink appearance="standard" variant="secondary" href={`${base}/work?view=waiting`}>
                View waiting work
              </UiActionLink>
            ) : undefined
          }
        />
      ) : (
        items.map((item) => (
          <RecordListItem
            className={styles.row}
            data-tone={workTypeTone[item.type]}
            key={`${item.type}:${item.id}:${item.actionKind ?? "record"}`}
          >
            <Link href={workItemHref(base, item, returnTo)} className={styles.body}>
              <small className={styles.type}>
                {workTypes.find(([id]) => id === item.type)?.[1]} ·{" "}
                {operationFromWork(item).actionKind === "override"
                  ? "Decision on behalf of approver"
                  : operationFromWork(item).actionKind === "approval"
                    ? "Approval"
                    : "Work"}
              </small>
              <strong className={styles.title}>{item.title}</strong>
              <p className={styles.action}>{item.nextAction ?? item.detail}</p>
              <WorkFlow item={item} />
              {item.dueAt && (
                <small className={styles.due}>
                  Due <TimeText appearance="standard" value={item.dueAt} />
                </small>
              )}
              {workBlocker(item) && item.queue !== "completed" && (
                <p className={styles.blocker}>Needs attention: {workBlocker(item)}</p>
              )}
              {!!item.relatedAlerts?.length && (
                <span className={styles.followups}>
                  {item.relatedAlerts.length} related {item.relatedAlerts.length === 1 ? "follow-up" : "follow-ups"}
                </span>
              )}
            </Link>
            {!!item.relatedAlerts?.length && <details className={styles.followupDetails}><summary>Related follow-ups ({item.relatedAlerts.length})</summary><ul>{item.relatedAlerts.map((alert, index) => <li key={index}><strong>{alert.title}</strong><span>{alert.nextAction}</span></li>)}</ul></details>}
            <div className={styles.commands}>
              <StatusBadge appearance="standard" status={item.status} />
              {onReview && isApprovalAction(item) && supportsWorkReview(item) && (
                <Button appearance="standard"
                  type="submit"
                  variant="secondary"
                  onClick={() => onReview(item)}
                  aria-label={`Review ${item.title}`}
                >
                  Review
                </Button>
              )}
            </div>
          </RecordListItem>
        ))
      )}
    </RecordList>
  );
}
