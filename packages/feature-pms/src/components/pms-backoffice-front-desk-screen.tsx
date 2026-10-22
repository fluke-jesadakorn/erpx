"use client";
import type { Option, Field } from "@erpx/feature-pms/components/workspace-types";
import { RecordSurface } from "@erpx/feature-records/components/record-surface";
import { Button } from "@erpx/ui/components/foundation";
import { FeatureWidget } from "@erpx/feature-shared/components/features/feature-page";
import { AttachmentFeatureSurface } from "@erpx/feature-files/components/attachment-feature-context";
import { type RecordDocumentPurpose } from "@erpx/feature-files/components/record-document-panel";
import { Hotel } from "lucide-react";
import Link from "@erpx/workplace/components/workspace-link";
import { useState, type ReactNode } from "react";
import dynamic from "next/dynamic";
import styles from "@erpx/feature-pms/components/workspace.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@erpx/front/components/front-language";
import { frontText } from "@erpx/feature-shared/lib/front-language";
import type {BackofficePmsWorkspaceFrontDeskScreenContext as BackofficePmsWorkspaceContext} from "@erpx/feature-pms/components/backoffice-screen-context";


const PmsOperationsCalendar = dynamic(() => import("@erpx/feature-pms/components/operations-calendar").then(module => module.PmsOperationsCalendar));


const RecordDocumentPanel = dynamic(() => import("@erpx/feature-files/components/record-document-panel").then(module => module.RecordDocumentPanel));


const StaySummary = dynamic(() => import("@erpx/feature-pms/components/stay-summary").then(module => module.StaySummary));


const StaffBookingChange = dynamic(() => import("@erpx/feature-pms/components/staff-booking-change").then(module => module.StaffBookingChange));





const pmsFileBase=(base:string)=>base.startsWith('/front/')?`${base.replace(/\/$/,'')}/pms`:base;


const guestDocumentPurposes: readonly RecordDocumentPurpose[] = [
  {
    key: "identity_document",
    label: "Identity document",
    description:
      "Attach the verified guest identity document. The original remains restricted and download-only.",
  },
  {
    key: "consent",
    label: "Guest consent",
    description:
      "Attach the signed privacy, service, or marketing consent reviewed for this guest.",
  },
];


const stayDocumentPurposes: readonly RecordDocumentPurpose[] = [
  {
    key: "signed_registration",
    label: "Signed registration",
    description: "Attach the signed registration card for this stay.",
  },
  {
    key: "transfer_slip",
    label: "Transfer slip",
    description: "Attach the bank-transfer slip associated with this stay.",
  },
];



function PmsRecordDocuments({
  base,
  targetType,
  targetId,
  expectedTargetVersion,
  title,
  purposes,
  editable,
}: {
  base: string;
  targetType: "pms_guest" | "pms_reservation" | "pms_stay" | "pms_lost_property";
  targetId: string;
  expectedTargetVersion: number;
  title: string;
  purposes: readonly RecordDocumentPurpose[];
  editable: boolean;
}) {
  const [open, setOpen] = useState(false);
  return (
    <AttachmentFeatureSurface>
      <details className={styles.action} onToggle={(event) => setOpen(event.currentTarget.open)}>
        <summary>{title}</summary>
        {open && (
          <RecordDocumentPanel
            base={pmsFileBase(base)}
            targetType={targetType}
            targetId={targetId}
            expectedTargetVersion={expectedTargetVersion}
            title={title}
            purposes={purposes}
            editable={editable}
          />
        )}
      </details>
    </AttachmentFeatureSurface>
  );
}


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
export function BackofficePmsWorkspaceFrontDeskScreen({context}: {context: BackofficePmsWorkspaceContext & {p: NonNullable<BackofficePmsWorkspaceContext["p"]>}}) {
  const {base, p, today, query, s, recordSelection, router, setQuery, stays, selectedRecord, guestName, roomName, can, data, rows, dailyPmsHref, form, rooms, guests, guestAccounts, rates} = context;
  return <FrontLocalizedTree>{(
                <Panel title="Front Office"><PmsOperationsCalendar base={base} propertyId={p.id} today={today}/>
                  <div className={styles.filters}>
                    {["", "arrivals", "departures", "reserved", "in_house", "checked_out"].map(
                      (st) => (
                        <Button
                          type="submit"
                          variant="plain"
                          key={st}
                          className={`button ${query === st ? "" : "secondary"}`}
                          onClick={() => {if(s?.listMeta){const params=new URLSearchParams(recordSelection);params.set("status",st);params.set("page","0");params.delete("queue");router.push(`?${params}`);}else setQuery(st);}}
                        >
                          {labelState[st] ?? "All"}
                        </Button>
                      ),
                    )}
                  </div>
                  {stays
                    .filter(r=>!s?.listMeta||s.listMeta.section!=="front-desk"||s.listMeta.ids.includes(r.id)||r.id===selectedRecord)
                    .filter((r) =>
                      query === "arrivals"
                        ? r.data.state === "reserved" && r.data.arrival === today
                        : query === "departures"
                          ? r.data.state === "in_house" && r.data.departure <= today
                          : !query || r.data.state === query,
                    )
                    .map((r) => (
                      <RecordSurface as="article" recordId={r.id}
                        key={r.id}
                        className={styles.record}
                        data-pms-record={r.id}
                        tabIndex={-1}
                      >
                        <div className={styles.line}>
                          <h3>{guestName(r.data.guestId)}</h3>
                          <Badge>{labelState[r.data.state]}</Badge>
                        </div>
                        <p>
                          {roomName(r.data.roomId)} · {r.data.arrival} → {r.data.departure} ·{" "}
                          {r.data.adults} guests
                        </p>
                        <StaySummary snapshot={s!} stay={r} financial={can("pms.cashier")||can("pms.reports")} base={base} documents={data.documents}/>
                        <PmsRecordDocuments
                          base={base}
                          targetType="pms_stay"
                          targetId={r.id}
                          expectedTargetVersion={r.version}
                          title="Registration and transfer documents"
                          purposes={stayDocumentPurposes}
                          editable={can("pms.frontdesk")}
                        />
                        {rows("guest")
                          .filter((guest) => guest.id === r.data.guestId)
                          .map((guest) => (
                            <PmsRecordDocuments
                              key={guest.id}
                              base={base}
                              targetType="pms_guest"
                              targetId={guest.id}
                              expectedTargetVersion={guest.version}
                              title="Guest identity and consent"
                              purposes={guestDocumentPurposes}
                              editable={can("pms.reservations")}
                            />
                          ))}
                        <Link
                          href={dailyPmsHref("guest-accounts", { property: p.id, guestAccount: r.data.guestAccountId })}
                        >
                          Open this guest account
                        </Link>
                        {["reserved", "in_house"].includes(r.data.state) && (
                          <>
                            {form(
                              r.data.state === "in_house" ? "Move room" : "Assign room",
                              r.data.state === "in_house" ? "stay.move" : "stay.assign",
                              [
                                select("roomId", "Room", rooms),
                                field("reason", "Notes", "text", "", false),
                              ],
                              { id: r.id },
                            )}
                            {r.data.state === "in_house" && form(
                          "Upgrade room", "stay.upgrade", [
                            select("roomId", "Room", rows("room").filter(room => room.data.active && room.data.typeId !== r.data.typeId && room.data.cleanliness === "inspected").map(room => ({ id: room.id, name: `${rows("room_type").find(type => type.id === room.data.typeId)?.data.name ?? ""} · ${room.data.number}` }))),
                            select("rateId", "Rate plan", rows("rate").filter(rate => rate.data.active && rate.data.typeId !== r.data.typeId).map(rate => ({ id: rate.id, name: `${rows("room_type").find(type => type.id === rate.data.typeId)?.data.name ?? ""} · ${rate.data.name}` }))),
                            field("reason", "Reason"),
                          ], { id: r.id },
                        )}
                        {r.data.state === "reserved" ? (
                              <>
                                {form("Check in", "stay.check_in", [], {
                                  id: r.id,
                                })}
                                {(!r.data.bookingPolicy) && form(
                                  "Cancel this stay",
                                  "stay.cancel",
                                  [field("reason", "Reason"), field("fee", "Fee", "money", 0)],
                                  { id: r.id },
                                )}
                                {form(
                                  "No-show this stay",
                                  "stay.no_show",
                                  [field("reason", "Reason"), field("fee", "Fee", "money", 0)],
                                  { id: r.id },
                                )}
                              </>
                            ) : (
                              <>
                                {form(
                                  "Post room charge before check-out",
                                  "stay.preview_checkout",
                                  [],
                                  { id: r.id },
                                )}
                                {form(
                                  "Check out",
                                  "stay.check_out",
                                  [
                                    field("lateFee", "Late check-out fee", "money", 0),
                                    field("reason", "Notes", "text", "", false),
                                  ],
                                  { id: r.id },
                                )}
                              </>
                            )}
                            {form(
                              "Assign guest to this room",
                              "stay.guest",
                              [select("guestId", "Guest", guests, r.data.guestId)],
                              { id: r.id },
                            )}
                            {form(
                              "Route the next room charge to a guest account",
                              "stay.route_guest_account",
                              [select("guestAccountId", "Destination guest account", guestAccounts)],
                              { id: r.id },
                            )}
                            {r.data.state === "reserved" && r.data.bookingPolicy && can("pms.reservations") && <StaffBookingChange base={base} snapshot={s!} stay={r} canOverride={can("pms.approve")} onSaved={()=>router.refresh()}/>}
                            {(r.data.state === "in_house" || !r.data.bookingPolicy) && form(
                              "Change dates / extend stay",
                              "stay.change_dates",
                              [
                                field("arrival", "Arrival date", "date", r.data.arrival),
                                field("departure", "New departure date", "date", r.data.departure),
                                select("rateId", "Rate plan", rates, r.data.rateId),
                                field("reason", "Reason"),
                              ],
                              { id: r.id },
                            )}
                          </>
                        )}
                      </RecordSurface>
                    ))}
                  {!stays.length && <Empty>No guests are currently in house.</Empty>}
                </Panel>
              )}</FrontLocalizedTree>;
}
