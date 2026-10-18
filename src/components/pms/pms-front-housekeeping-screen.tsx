"use client";
import type { Option, Field } from "./workspace-types";
import { RecordSurface } from "@/components/records/record-surface";
import { Field as UiField, Select } from "@/components/ui/foundation";
import { FeatureWidget } from "@/components/shared/features/feature-page";
import { Hotel } from "lucide-react";
import { type ReactNode } from "react";
import styles from "./workspace.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@/components/front/front-language";
import { frontText } from "@/lib/shared/front-language";
import type {FrontPmsDailyContext} from "./standard-pms-front-workspace";


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
export function FrontPmsDailyHousekeepingScreen({context}: {context: FrontPmsDailyContext}) {
  const {floor, setFloor, roomRows, assignee, setAssignee, data, taskState, setTaskState, showHousekeepingForms, selectedRecord, tr, form, today, taskRows, roomName, showInspectionForms, base, language, rows, showMaintenanceForms} = context;
  return <FrontLocalizedTree>{(
              <>
                <Panel title="Housekeeping tasks"><div className={styles.filters}><UiField>Floor<Select value={floor} onChange={e=>setFloor(e.target.value)}><option value="">All floors</option>{[...new Set(roomRows.map(r=>r.data.floor))].filter(Boolean).map(f=><option key={f} value={f}>{f}</option>)}</Select></UiField><UiField>Assigned to<Select value={assignee} onChange={e=>setAssignee(e.target.value)}><option value="">All staff</option>{data.staff.map(st=><option key={st.id} value={st.id}>{st.name}</option>)}</Select></UiField><UiField>Status<Select value={taskState} onChange={e=>setTaskState(e.target.value)}><option value="">Active tasks</option>{["pending","working","clean","done"].map(v=><option key={v} value={v}>{v}</option>)}</Select></UiField></div>
                  {showHousekeepingForms && (
                    <details
                      className={styles.taskCreation}
                      open={roomRows.some((row) => row.id === selectedRecord)}
                    >
                      <summary>{tr("Create housekeeping task")}</summary>
                      {roomRows.map(
                        (row) =>
                          showHousekeepingForms && (
                            <RecordSurface recordId={row.id} key={row.id} data-pms-record={row.id} tabIndex={-1}>
                              {form(
                                `${tr("Create housekeeping task")} · ${tr("Room")} ${row.data.number}`,
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
                                { roomId: row.id },
                              )}
                            </RecordSurface>
                          ),
                      )}
                    </details>
                  )}
                  {taskRows.filter(row=>(!floor||roomRows.find(r=>r.id===row.data.roomId)?.data.floor===floor)&&(!assignee||row.data.assigneeId===assignee)&&(!taskState||row.data.state===taskState))
                    .filter(
                      (row) =>
                        row.id === selectedRecord || Boolean(taskState) ||
                        !["done", "superseded"].includes(row.data.state),
                    )
                    .map((row) => (
                      <RecordSurface as="article" recordId={row.id}
                        className={styles.record}
                        key={row.id}
                        data-pms-record={row.id}
                        tabIndex={-1}
                      >
                        <div className={styles.line}>
                          <h3>
                            {tr("Room")} {roomName(row.data.roomId)} · {labelState[row.data.type]}
                          </h3>
                          <Badge>{labelState[row.data.state]}</Badge>
                        </div>
                        <p>
                          {row.data.date} ·{" "}
                          {row.data.priority === "urgent" ? tr("Urgent") : tr("Normal")}
                        </p>
                        {showInspectionForms &&
                          form(
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
                                row.data.priority,
                              ),
                            ],
                            { id: row.id },
                          )}
                        {showHousekeepingForms &&
                          row.data.state === "pending" &&
                          form("Start task", "task.start", [], { id: row.id })}
                        {showHousekeepingForms &&
                          ["pending", "working"].includes(row.data.state) &&
                          form(
                            "Mark task complete",
                            "task.complete",
                            [
                              ...row.data.checklist.map((name, index) =>
                                field(`check_${index}`, name, "checkbox", false),
                              ),
                              field("notes", "Notes", "textarea", "", false),
                            ],
                            { id: row.id },
                            (value) => ({
                              ...value,
                              checked: row.data.checklist.filter(
                                (_, index) => value[`check_${index}`],
                              ),
                            }),
                            [
                              {
                                base,
                                targetType: "pms_task",
                                targetId: row.id,
                                expectedTargetVersion: row.version,
                                purposeKey: "room_photo",
                                title: "Room completion photos",
                                description:
                                  "Attach room-condition photos taken while completing this task.",
                              },
                              {
                                base,
                                targetType: "pms_task",
                                targetId: row.id,
                                expectedTargetVersion: row.version,
                                purposeKey: "damage_photo",
                                title: "Damage evidence",
                                description:
                                  "Attach damage photos when the completed task found an exception.",
                              },
                            ],
                          )}
                        {showInspectionForms &&
                          row.data.state === "clean" &&
                          (row.data.completedBy === data.userId ? (
                            <p className={styles.notice}>
                              {language === "th"
                                ? "ส่งตรวจแล้ว ให้พนักงานอีกคนตรวจรับห้องนี้"
                                : "Ready for review. Another staff member must inspect this room."}
                            </p>
                          ) : (
                            form("Inspect task", "task.inspect", [], { id: row.id })
                          ))}
                      </RecordSurface>
                    ))}
                  {!taskRows.some((row) => !["done", "superseded"].includes(row.data.state)) && (
                    <Empty>{tr("No open room tasks")}</Empty>
                  )}
                </Panel>
                <Panel title="Rooms under maintenance">
                  {rows("block")
                    .filter((row) => row.data.kind === "maintenance" && row.data.state === "active")
                    .map((row) => (
                      <RecordSurface recordId={row.id}
                        className={styles.record}
                        key={row.id}
                        data-pms-record={row.id}
                        tabIndex={-1}
                      >
                        <strong>
                          {tr("Room")} {roomName(row.data.roomId)} · {row.data.name}
                        </strong>
                        <p>
                          {row.data.from} → {row.data.to}
                        </p>
                        {showMaintenanceForms &&
                          form("Reopen room after inspection", "maintenance.close", [], {
                            id: row.id,
                          })}
                      </RecordSurface>
                    ))}
                </Panel>
              </>
            )}</FrontLocalizedTree>;
}
