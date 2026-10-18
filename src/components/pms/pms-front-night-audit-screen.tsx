"use client";
import { RecordSurface } from "@/components/records/record-surface";
import { FeatureWidget } from "@/components/shared/features/feature-page";
import type { PmsWorkspace } from "@/server/modules/pms/public/pms";
import { MoonStar } from "lucide-react";
import { type ReactNode } from "react";
import styles from "./workspace.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@/components/front/front-language";
import { frontText } from "@/lib/shared/front-language";
import type {FrontPmsDailyContext} from "./standard-pms-front-workspace";




function nightAuditMessage(message: string) {
  return message === "PMS_PERIOD_CLOSED"
    ? "Finance needs to check the accounting period for this business date before day close can continue."
    : message;
}


function RestaurantDayControls({controls,money,tr=(text:string)=>text,children}:{
  controls:NonNullable<PmsWorkspace['dayControls']>;money:(value:number)=>string;tr?:(text:string)=>string;children?:ReactNode;
}) {
  return <section aria-label={tr('Restaurant day controls')} className={styles.record}>
    <h3>{tr('Restaurant day controls')}</h3>
    <p>{tr('Business date')} {controls.businessDate} · {controls.openBills} {tr('open bills')} · {controls.pendingPayments} {tr('pending payments')} · {controls.openShifts} {tr('open cash sessions')}</p>
    <p>{tr('Direct sales')} {money(controls.directSalesMinor)} · {tr('Room charges')} {money(controls.roomChargesMinor)} · {tr('Cash received')} {money(controls.cashReceivedMinor)} · {tr('Verified transfers')} {money(controls.verifiedTransferMinor)}</p>
    <p>{tr('Restaurant stock movement')} {money(controls.stockValueMinor)} · {tr('Inventory ledger movement')} {money(controls.stockLedgerMinor)}</p>
    {controls.blockers.length ? <><ul className={styles.blockers}>{controls.blockers.map(blocker=><li key={blocker}>{blocker}</li>)}</ul>{children}</> : <p className={styles.notice}>{tr('Restaurant controls are ready for day close.')}</p>}
  </section>;
}


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
export function FrontPmsDailyNightAuditScreen({context}: {context: FrontPmsDailyContext}) {
  const {tr, today, data, money, property, operationalNightBlockers, form, rows} = context;
  return <FrontLocalizedTree>{(
              <Panel title="Night audit">
                <div className={styles.auditHero}>
                  <MoonStar size={38} />
                  <div>
                    <h3>
                      {tr("Close business day")} {today}
                    </h3>
                    <p>
                      {tr(
                        "Post room charges and start the next day after today's work is complete",
                      )}
                    </p>
                  </div>
                </div>
                {data.dayControls && <RestaurantDayControls controls={data.dayControls} money={money} tr={tr} />}
                {property?.state === "open" && (
                  <>
                    {operationalNightBlockers.length > 0 ? (
                      <ul className={styles.blockers}>
                        {operationalNightBlockers.map((item) => (
                          <li key={item}>{item}</li>
                        ))}
                      </ul>
                    ) : (
                      !data.dayControls?.blockers.length && (
                        <p className={styles.notice}>{tr("Ready to close the day")}</p>
                      )
                    )}
                    {!operationalNightBlockers.length &&
                      !data.dayControls?.blockers.length &&
                      form("Start day close", "night.start")}
                  </>
                )}
                {rows("night")
                  .slice()
                  .reverse()
                  .map((row) => (
                    <RecordSurface as="article" recordId={row.id}
                      className={styles.record}
                      key={row.id}
                      data-pms-record={row.id}
                      tabIndex={-1}
                    >
                      <strong>{row.data.date}</strong>{" "}
                      <Badge tone={row.data.state === "completed" ? "good" : "warn"}>
                        {labelState[row.data.state]}
                      </Badge>
                      <p>{tr(nightAuditMessage(row.data.message))}</p>
                      {row.data.state === "blocked" &&
                        form("Resolve and continue", "night.resume", [], { id: row.id })}
                    </RecordSurface>
                  ))}
              </Panel>
            )}</FrontLocalizedTree>;
}
