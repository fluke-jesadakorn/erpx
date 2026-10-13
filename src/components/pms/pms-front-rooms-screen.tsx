"use client";
import type { Field } from "./workspace-types";
import { FeatureWidget } from "@/components/shared/features/feature-page";
import { plusDays } from "@/server/modules/pms/public/rules/pms-model";
import { Hotel } from "lucide-react";
import { type ReactNode } from "react";
import styles from "./workspace.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@/components/front/front-language";
import { frontText } from "@/lib/shared/front-language";
import type {FrontPmsDailyContext} from "./pms-front-workspace";


const field = (
  name: string,
  label: string,
  type: Field["type"] = "text",
  value?: Field["value"],
  required = true,
): Field => ({ name, label, type, value, required });


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
export function FrontPmsDailyRoomsScreen({context}: {context: FrontPmsDailyContext}) {
  const {roomRows, tr, stays, showHousekeepingForms, form, showMaintenanceForms, today} = context;
  return <FrontLocalizedTree>{(
              <Panel title="Rooms">
                <div className={styles.roomGrid}>
                  {roomRows.map((row) => (
                    <article
                      className={`${styles.room} ${row.data.cleanliness === "inspected" ? styles.ready : ""}`}
                      key={row.id}
                    >
                      <strong>{row.data.number}</strong>
                      <small>
                        {row.data.building} · {tr("Floor")} {row.data.floor}
                      </small>
                      <Badge>{labelState[row.data.cleanliness]}</Badge>
                      <span>
                        {tr(
                          stays.some(
                            (stay) => stay.data.state === "in_house" && stay.data.roomId === row.id,
                          )
                            ? "Occupied"
                            : "Vacant",
                        )}{" "}
                        {row.data.dnd ? "· DND" : ""}
                      </span>
                      {showHousekeepingForms &&
                        form(
                          "Do-not-disturb request",
                          "room.dnd",
                          [field("dnd", "DND", "checkbox", row.data.dnd)],
                          { id: row.id },
                        )}
                      {showHousekeepingForms &&
                        form(
                          "Verify occupancy",
                          "room.observe",
                          [field("occupied", "Guest present", "checkbox", false)],
                          { id: row.id },
                        )}
                      {showMaintenanceForms &&
                        form(
                          "Close room for maintenance",
                          "maintenance.open",
                          [
                            field("from", "Start date", "date", today),
                            field("to", "Expected completion", "date", plusDays(today, 1)),
                            field("reason", "Details"),
                          ],
                          { roomId: row.id },
                        )}
                    </article>
                  ))}
                </div>
                {!roomRows.length && <Empty>{tr("No rooms are available for daily work")}</Empty>}
              </Panel>
            )}</FrontLocalizedTree>;
}
