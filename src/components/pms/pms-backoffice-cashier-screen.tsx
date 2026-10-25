"use client";
import type { Field } from "./workspace-types";
import { RecordSurface } from "@/components/records/record-surface";
import { FeatureWidget } from "@/components/features/feature-page";
import { type ReactNode } from "react";
import styles from "./workspace.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@/components/front/front-language";
import { frontText } from "@/lib/front-language";
import type {BackofficePmsWorkspaceContext} from "./pms-backoffice-workspace";


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
export function BackofficePmsWorkspaceCashierScreen({context}: {context: BackofficePmsWorkspaceContext}) {
  const {form, rows, data, money} = context;
  return <FrontLocalizedTree>{(
                <Panel title="Cashier shifts">
                  {form("Open shift", "shift.open", [
                    field("name", "Shift / till name"),
                    field("opening", "Opening float", "money", 0),
                  ])}
                  {rows("shift").map((r) => (
                    <RecordSurface as="article" recordId={r.id}
                      className={styles.record}
                      key={r.id}
                      data-pms-record={r.id}
                      tabIndex={-1}
                    >
                      <div className={styles.line}>
                        <h3>{r.data.name}</h3>
                        <Badge>{labelState[r.data.state]}</Badge>
                      </div>
                      <p>
                        {r.data.date} ·{" "}
                        {data.staff.find((x) => x.id === r.data.userId)?.name ?? r.data.userId}
                      </p>
                      <p>
                        System cash {money(r.data.cash)} · Counted cash {money(r.data.counted)} ·
                        Variance {money(r.data.variance)}
                      </p>
                      {r.data.state === "open" && (
                        <>
                          {form(
                            "Move cash / deposit to bank",
                            "cash.move",
                            [
                              {
                                ...field("amount", "Amount (+cash in / −bank deposit)", "money", 0),
                                min: -9999999,
                              },
                              field("reason", "Reason"),
                            ],
                            { shiftId: r.id },
                          )}
                          {form(
                            "Close shift",
                            "shift.close",
                            [
                              field("counted", "Counted cash", "money", r.data.cash / 100),
                              field("handover", "Handover record", "textarea", "", false),
                            ],
                            { id: r.id },
                          )}
                        </>
                      )}
                      {r.data.state === "closed" &&
                        form("Review shift", "shift.review", [], { id: r.id })}
                      <p className={styles.muted}>{r.data.handover}</p>
                    </RecordSurface>
                  ))}
                </Panel>
              )}</FrontLocalizedTree>;
}
