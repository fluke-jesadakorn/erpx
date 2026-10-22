"use client";
import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";
import { FeatureWidget } from "@erpx/feature-shared/components/features/feature-page";
import { BedDouble, CalendarDays, ClipboardCheck, Hotel, Wallet } from "lucide-react";
import Link from "@erpx/workplace/components/workspace-link";
import { type ReactNode } from "react";
import styles from "@erpx/feature-pms/components/workspace.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@erpx/front/components/front-language";
import { frontText } from "@erpx/feature-shared/lib/front-language";
import type {BackofficePmsWorkspaceDashboardScreenContext as BackofficePmsWorkspaceContext} from "@erpx/feature-pms/components/backoffice-screen-context";


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
export function BackofficePmsWorkspaceDashboardScreen({context}: {context: BackofficePmsWorkspaceContext & {p: NonNullable<BackofficePmsWorkspaceContext["p"]>}}) {
  const {stays, today, rows, tr, dailyPmsHref, p, guestName, roomName, s} = context;
  return <FrontLocalizedTree>{(
                <>
                  <div className={styles.stats}>
                    {[
                      [
                        BedDouble,
                        "Guests",
                        stays.filter((r) => r.data.state === "in_house").length,
                      ],
                      [
                        CalendarDays,
                        "Arriving today",
                        stays.filter((r) => r.data.arrival === today && r.data.state === "reserved")
                          .length,
                      ],
                      [
                        Wallet,
                        "Departing today",
                        stays.filter(
                          (r) => r.data.departure === today && r.data.state === "in_house",
                        ).length,
                      ],
                      [
                        ClipboardCheck,
                        "Open room tasks",
                        rows("task").filter((r) => !["done", "superseded"].includes(r.data.state))
                          .length,
                      ],
                    ].map(([Icon, label, value], i) => {
                      const I = Icon as typeof Hotel;
                      return (
                        <div className={styles.stat} key={i}>
                          <I size={22} />
                          <span>{tr(String(label))}</span>
                          <strong>{String(value)}</strong>
                        </div>
                      );
                    })}
                  </div>
                  <>
                    <Panel title="Room status">
                      <div className={styles.roomGrid}>
                        {rows("room").map((r) => {
                          const stay = stays.find(
                            (x) => x.data.state === "in_house" && x.data.roomId === r.id,
                          );
                          return (
                            <Link
                              className={`${styles.room} ${r.data.cleanliness === "inspected" ? styles.ready : ""}`}
                              href={dailyPmsHref("rooms", { property: p.id })}
                              key={r.id}
                            >
                              <strong>{r.data.number}</strong>
                              <span>{tr(stay ? "Occupied" : "Vacant")}</span>
                              <Badge>{labelState[r.data.cleanliness]}</Badge>
                              {r.data.dnd && <small>DND</small>}
                            </Link>
                          );
                        })}
                      </div>
                      {!rows("room").length && <Empty>Add rooms from Settings.</Empty>}
                    </Panel>
                    <Panel title="Today's work">
                      {stays
                        .filter(
                          (r) =>
                            (r.data.arrival === today && r.data.state === "reserved") ||
                            (r.data.departure === today && r.data.state === "in_house"),
                        )
                        .map((r) => (
                          <Link
                            className={styles.line}
                            href={dailyPmsHref("front-desk", { property: p.id })}
                            key={r.id}
                          >
                            <strong>{guestName(r.data.guestId)}</strong>
                            <span>{roomName(r.data.roomId)}</span>
                            <Badge>{labelState[r.data.state]}</Badge>
                          </Link>
                        ))}
                      {!(s?.dashboardHasStays ?? Boolean(stays.length)) && (
                        <Empty>
                          No reservations yet. Open the hotel to start accepting bookings.
                        </Empty>
                      )}
                      <UiActionLink
                        variant="secondary"

                        href={dailyPmsHref("reservations", { property: p.id })}
                      >
                        View all reservations
                      </UiActionLink>
                    </Panel>
                  </>
                </>
              )}</FrontLocalizedTree>;
}
