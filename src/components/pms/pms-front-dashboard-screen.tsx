"use client";
import { FeatureWidget } from "@/components/shared/features/feature-page";
import { BedDouble, CalendarDays, ClipboardCheck, Hotel, Wallet } from "lucide-react";
import Link from "@/components/workplace/workspace-link";
import { type ReactNode } from "react";
import styles from "./workspace.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@/components/front/front-language";
import { frontText } from "@/lib/shared/front-language";
import type {FrontPmsDailyContext} from "./standard-pms-front-workspace";


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
export function FrontPmsDailyDashboardScreen({context}: {context: FrontPmsDailyContext}) {
  const {tr, language, stays, today, showFrontDeskForms, taskRows, showHousekeepingForms, showInspectionForms, rows, showCashierForms, frontHref, property, roomRows, snapshot, selectedRecord, guestName, roomName} = context;
  return <FrontLocalizedTree>{(
              <>
                <section className={styles.frontQueues} aria-label={tr("Today's work")}>
                  {[
                    {
                      label: language === "th" ? "รอเช็กอิน" : "Arrivals",
                      count: stays.filter(
                        (row) => row.data.state === "reserved" && row.data.arrival <= today,
                      ).length,
                      action: language === "th" ? "เปิดคิวเช็กอิน" : "Open check-in queue",
                      href: "front-desk?queue=arrivals",
                      show: showFrontDeskForms,
                    },
                    {
                      label: language === "th" ? "รอเช็กเอาต์" : "Departures",
                      count: stays.filter(
                        (row) => row.data.state === "in_house" && row.data.departure <= today,
                      ).length,
                      action: language === "th" ? "เปิดคิวเช็กเอาต์" : "Open checkout queue",
                      href: "front-desk?queue=departures",
                      show: showFrontDeskForms,
                    },
                    {
                      label: language === "th" ? "ห้องต้องเตรียม" : "Rooms to prepare",
                      count: taskRows.filter(
                        (row) => !["done", "superseded"].includes(row.data.state),
                      ).length,
                      action: language === "th" ? "เปิดงานแม่บ้าน" : "Open housekeeping",
                      href: "housekeeping",
                      show: showHousekeepingForms || showInspectionForms,
                    },
                    {
                      label: language === "th" ? "ยอดค้างรับ" : "Balances due",
                      count: rows("guest_account").filter(
                        (row) => row.data.state !== "closed" && row.data.due > 0,
                      ).length,
                      action: language === "th" ? "เปิดบัญชีผู้เข้าพัก" : "Open guest accounts",
                      href: "guest-accounts",
                      show: showCashierForms,
                    },
                  ]
                    .filter((item) => item.show)
                    .map((item) => (
                      <Link
                        className={styles.frontQueue}
                        key={item.href + item.label}
                        href={frontHref + item.href + (property ? `${item.href.includes("?") ? "&" : "?"}property=${encodeURIComponent(property.id)}` : "")}
                      >
                        <span>{item.label}</span>
                        <strong>{item.count}</strong>
                        <small>{item.action} →</small>
                      </Link>
                    ))}
                </section>
                <div className={styles.stats}>
                  {[
                    [
                      BedDouble,
                      "Guests",
                      stays.filter((row) => row.data.state === "in_house").length,
                    ],
                    [
                      CalendarDays,
                      "Arriving today",
                      stays.filter(
                        (row) => row.data.arrival === today && row.data.state === "reserved",
                      ).length,
                    ],
                    [
                      Wallet,
                      "Departing today",
                      stays.filter(
                        (row) => row.data.departure === today && row.data.state === "in_house",
                      ).length,
                    ],
                    [
                      ClipboardCheck,
                      "Open room tasks",
                      taskRows.filter((row) => !["done", "superseded"].includes(row.data.state))
                        .length,
                    ],
                  ].map(([Icon, label, value], index) => {
                    const StatIcon = Icon as typeof Hotel;
                    return (
                      <div className={styles.stat} key={index}>
                        <StatIcon size={22} />
                        <span>{tr(String(label))}</span>
                        <strong>{String(value)}</strong>
                      </div>
                    );
                  })}
                </div>
                <Panel title="Room status">
                  <div className={styles.roomGrid}>
                    {roomRows.map((row) => {
                      const occupied = stays.some(
                        (stay) => stay.data.state === "in_house" && stay.data.roomId === row.id,
                      );
                      return (
                        <article
                          className={`${styles.room} ${row.data.cleanliness === "inspected" ? styles.ready : ""}`}
                          key={row.id}
                        >
                          <strong>{row.data.number}</strong>
                          <span>{tr(occupied ? "Occupied" : "Vacant")}</span>
                          <Badge>{labelState[row.data.cleanliness]}</Badge>
                          {row.data.dnd && <small>{tr("Do not disturb")}</small>}
                        </article>
                      );
                    })}
                  </div>
                  {!roomRows.length && <Empty>{tr("No rooms are available for daily work")}</Empty>}
                </Panel>
                <Panel title="Today's work">
                  {stays
                    .filter(r=>!snapshot?.listMeta||snapshot.listMeta.section!=="front-desk"||snapshot.listMeta.ids.includes(r.id)||r.id===selectedRecord)
                    .filter(
                      (row) =>
                        (row.data.arrival === today && row.data.state === "reserved") ||
                        (row.data.departure === today && row.data.state === "in_house"),
                    )
                    .map((row) => (
                      <div className={styles.line} key={row.id}>
                        <strong>{guestName(row.data.guestId)}</strong>
                        <span>
                          {roomName(row.data.roomId)} · {labelState[row.data.state]}
                        </span>
                      </div>
                    ))}
                  {!stays.some(
                    (row) => row.data.arrival === today || row.data.departure === today,
                  ) && <Empty>{tr("No arrivals or departures today")}</Empty>}
                </Panel>
              </>
            )}</FrontLocalizedTree>;
}
