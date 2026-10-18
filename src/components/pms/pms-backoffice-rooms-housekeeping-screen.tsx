"use client";
import type { Option, Field } from "./workspace-types";
import { RecordSurface } from "@/components/records/record-surface";
import { FeatureWidget } from "@/components/shared/features/feature-page";
import { plusDays } from "@/server/modules/pms/public/rules/pms-model";
import { Hotel } from "lucide-react";
import { type ReactNode } from "react";
import styles from "./workspace.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@/components/front/front-language";
import { frontText } from "@/lib/shared/front-language";
import type {BackofficePmsWorkspaceRoomsHousekeepingScreenContext as BackofficePmsWorkspaceContext} from "./backoffice-screen-context";


const field = (
  name: string,
  label: string,
  type: Field["type"] = "text",
  value?: Field["value"],
  required = true,
): Field => ({ name, label, type, value, required });


const select = (
  name: string,
  label: string,
  options: Option[],
  value?: string,
  required = true,
): Field => ({ name, label, type: "select", options, value, required });


const Badge = ({ children, tone = "" }: { children: ReactNode; tone?: string }) => {
  const language = useFrontLanguage();
  return (
    <span
      className={`${styles.badge} ${tone === "good" ? styles.good : tone === "warn" ? styles.warn : ""}`}
    >
      {typeof children === "string" ? frontText(children, language) : children}
    </span>
  );
};


const Empty = ({ children }: { children: ReactNode }) => {
  const language = useFrontLanguage();
  return (
    <div className={styles.empty}>
      <Hotel size={24} />
      <p>{typeof children === "string" ? frontText(children, language) : children}</p>
    </div>
  );
};


const Panel = ({ title, children }: { title: string; children: ReactNode }) => {
  const language = useFrontLanguage();
  return (
    <FeatureWidget
      id={
        "pms-" +
        title
          .toLowerCase()
          .replace(/[^a-z0-9]+/g, "-")
          .replace(/^-|-$/g, "")
      }
      title={frontText(title, language)}
    >
      {children}
    </FeatureWidget>
  );
};


const labelState: Record<string, string> = {
  arrivals: "Arriving today",
  departures: "Departing today",
  setup: "Setup",
  open: "Open",
  closing: "Closing day",
  confirmed: "Confirmed",
  cancelled: "Cancelled",
  no_show: "No-show",
  completed: "Completed",
  reserved: "Awaiting check-in",
  in_house: "In house",
  checked_out: "Checked out",
  dirty: "Needs cleaning",
  clean: "Needs inspection",
  inspected: "Inspected",
  pending: "Pending",
  verified: "Verified",
  working: "In progress",
  done: "Inspected",
  superseded: "Replaced by newer work",
  requested: "Awaiting approval",
  approved: "Approved",
  rejected: "Rejected",
  paid: "Paid",
  credit: "Credit receivable",
  closed: "Closed",
  reviewed: "Reviewed",
  blocked: "Needs correction",
  departure: "After check-out",
  arrival: "Before arrival",
  stayover: "During stay",
  maintenance: "Maintenance",
  discrepancy: "Status mismatch",
  stored: "Stored",
  returned: "Returned",
  disposed: "Disposed",
};
export function BackofficePmsWorkspaceRoomsHousekeepingScreen({context}: {context: BackofficePmsWorkspaceContext}) {
  const {section, rows, stays, form, today, selectedRecord, roomName, data, base} = context;
  return <FrontLocalizedTree>{(
                <>
                  <Panel title={section === "rooms" ? "Rooms & readiness" : "Housekeeping"}>
                    <div className={styles.roomGrid}>
                      {rows("room").map((r) => (
                        <article
                          className={`${styles.room} ${r.data.cleanliness === "inspected" ? styles.ready : ""}`}
                          key={r.id}
                        >
                          <strong>{r.data.number}</strong>
                          <small>
                            {r.data.building} · Floor {r.data.floor}
                          </small>
                          <Badge tone={r.data.cleanliness === "inspected" ? "good" : "warn"}>
                            {labelState[r.data.cleanliness]}
                          </Badge>
                          <span>
                            {stays.some(
                              (st) => st.data.state === "in_house" && st.data.roomId === r.id,
                            )
                              ? "Occupied"
                              : "Vacant"}{" "}
                            {r.data.dnd ? "· DND" : ""}
                          </span>
                          {form(
                            "Do-not-disturb request",
                            "room.dnd",
                            [field("dnd", "DND", "checkbox", r.data.dnd)],
                            { id: r.id },
                          )}
                          {form(
                            "Verify occupancy",
                            "room.observe",
                            [field("occupied", "Guest present", "checkbox", false)],
                            { id: r.id },
                          )}
                          {form(
                            "Create housekeeping task",
                            "task.create",
                            [
                              select(
                                "type",
                                "Task type",
                                [
                                  { id: "arrival", name: "Before arrival" },
                                  { id: "stayover", name: "During stay" },
                                  { id: "departure", name: "After check-out" },
                                ],
                                "arrival",
                              ),
                              field("date", "Work date", "date", today),
                            ],
                            { roomId: r.id },
                          )}
                          {form(
                            "Close room for maintenance",
                            "maintenance.open",
                            [
                              field("from", "Start date", "date", today),
                              field("to", "Expected completion", "date", plusDays(today, 1)),
                              field("reason", "Details"),
                            ],
                            { roomId: r.id },
                          )}
                        </article>
                      ))}
                    </div>
                  </Panel>
                  <Panel title="Tasks">
                    {rows("task")
                      .filter(
                        (r) =>
                          r.id === selectedRecord || !["done", "superseded"].includes(r.data.state),
                      )
                      .sort(
                        (a, b) =>
                          Number(b.data.priority === "urgent") -
                          Number(a.data.priority === "urgent"),
                      )
                      .map((r) => (
                        <RecordSurface as="article" recordId={r.id}
                          key={r.id}
                          className={styles.record}
                          data-pms-record={r.id}
                          tabIndex={-1}
                        >
                          <div className={styles.line}>
                            <h3>
                              Room {roomName(r.data.roomId)} · {labelState[r.data.type]}
                            </h3>
                            <Badge>{labelState[r.data.state]}</Badge>
                          </div>
                          <p>
                            {r.data.date} ·{" "}
                            {data.staff.find((x) => x.id === r.data.assigneeId)?.name ??
                              "Unassigned"}{" "}
                            · {r.data.priority === "urgent" ? "Urgent" : "Normal"}
                          </p>
                          {form(
                            "Assign task",
                            "task.assign",
                            [
                              select("assigneeId", "Staff member", data.staff),
                              select(
                                "priority",
                                "Priority",
                                [
                                  { id: "normal", name: "Normal" },
                                  { id: "urgent", name: "Urgent" },
                                ],
                                r.data.priority,
                              ),
                            ],
                            { id: r.id },
                          )}
                          {r.data.state === "pending" &&
                            form("Start task", "task.start", [], { id: r.id })}
                          {["pending", "working"].includes(r.data.state) &&
                            form(
                              "Mark task complete",
                              "task.complete",
                              [
                                ...r.data.checklist.map((name, i) =>
                                  field(`check_${i}`, name, "checkbox", false),
                                ),
                                field("notes", "Notes", "textarea", "", false),
                              ],
                              { id: r.id },
                              (v) => ({
                                ...v,
                                checked: r.data.checklist.filter((_, i) => v[`check_${i}`]),
                              }),
                              [
                                {
                                  base,
                                  targetType: "pms_task",
                                  targetId: r.id,
                                  expectedTargetVersion: r.version,
                                  purposeKey: "room_photo",
                                  title: "Room completion photos",
                                  description:
                                    "Attach room-condition photos taken while completing this task.",
                                },
                                {
                                  base,
                                  targetType: "pms_task",
                                  targetId: r.id,
                                  expectedTargetVersion: r.version,
                                  purposeKey: "damage_photo",
                                  title: "Damage evidence",
                                  description:
                                    "Attach damage photos when the completed task found an exception.",
                                },
                              ],
                            )}
                          {r.data.state === "clean" &&
                            form("Inspect task", "task.inspect", [], { id: r.id })}
                        </RecordSurface>
                      ))}
                    {!rows("task").some((t) => !["done", "superseded"].includes(t.data.state)) && (
                      <Empty>No open room tasks.</Empty>
                    )}
                  </Panel>
                  <Panel title="Rooms under maintenance">
                    {rows("block")
                      .filter((r) => r.data.kind === "maintenance" && r.data.state === "active")
                      .map((r) => (
                        <RecordSurface recordId={r.id}
                          className={styles.record}
                          key={r.id}
                          data-pms-record={r.id}
                          tabIndex={-1}
                        >
                          <strong>
                            Room {roomName(r.data.roomId)} · {r.data.name}
                          </strong>
                          <p>
                            {r.data.from} → {r.data.to}
                          </p>
                          {form("Reopen room after inspection", "maintenance.close", [], {
                            id: r.id,
                          })}
                        </RecordSurface>
                      ))}
                  </Panel>
                </>
              )}</FrontLocalizedTree>;
}
