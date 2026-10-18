"use client";
import type { Field } from "./workspace-types";
import { RecordSurface } from "@/components/records/record-surface";
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
export function FrontPmsDailyCashierScreen({context}: {context: FrontPmsDailyContext}) {
  const {form, rows, tr, money} = context;
  return <FrontLocalizedTree>{(
              <Panel title="Cashier shifts">
                {form("Open cash shift", "shift.open", [
                  field("name", "Shift / till name"),
                  field("opening", "Opening float", "money", 0),
                ])}
                {rows("shift").map((row) => (
                  <RecordSurface as="article" recordId={row.id}
                    className={styles.record}
                    key={row.id}
                    data-pms-record={row.id}
                    tabIndex={-1}
                  >
                    <div className={styles.line}>
                      <h3>{row.data.name}</h3>
                      <Badge>{labelState[row.data.state]}</Badge>
                    </div>
                    <p>{row.data.date}</p>
                    <p>
                      {tr("System cash")} {money(row.data.cash)} · {tr("Counted cash")}{" "}
                      {money(row.data.counted)} · {tr("Variance")} {money(row.data.variance)}
                    </p>
                    {row.data.state === "open" && (
                      <>
                        {form(
                          "Move cash / deposit",
                          "cash.move",
                          [
                            {
                              ...field("amount", "Amount (+cash in / −bank deposit)", "money", 0),
                              min: -9999999,
                            },
                            field("reason", "Reason"),
                          ],
                          { shiftId: row.id },
                        )}
                        {form(
                          "Close cash shift",
                          "shift.close",
                          [
                            field("counted", "Counted cash", "money", row.data.cash / 100),
                            field("handover", "Handover record", "textarea", "", false),
                          ],
                          { id: row.id },
                        )}
                      </>
                    )}
                  </RecordSurface>
                ))}
                {!rows("shift").length && <Empty>{tr("No cash shifts yet")}</Empty>}
              </Panel>
            )}</FrontLocalizedTree>;
}
