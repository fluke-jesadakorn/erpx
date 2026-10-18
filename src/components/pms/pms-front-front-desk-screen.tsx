"use client";
import type { Option, Field } from "./workspace-types";
import { RecordSurface } from "@/components/records/record-surface";
import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { Button, Field as UiField, TextInput } from "@/components/ui/foundation";
import { FeatureWidget } from "@/components/shared/features/feature-page";
import { AttachmentFeatureSurface } from "@/components/files/attachment-feature-context";
import { type RecordDocumentPurpose } from "@/components/files/record-document-panel";
import { Hotel } from "lucide-react";
import { useState, type ReactNode } from "react";
import dynamic from "next/dynamic";
import styles from "./workspace.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@/components/front/front-language";
import { frontText } from "@/lib/shared/front-language";
import type {FrontPmsDailyContext} from "./standard-pms-front-workspace";


const PmsOperationsCalendar = dynamic(() => import("./operations-calendar").then(module => module.PmsOperationsCalendar));


const RecordDocumentPanel = dynamic(() => import("@/components/files/record-document-panel").then(module => module.RecordDocumentPanel));


const StaySummary = dynamic(() => import("./stay-summary").then(module => module.StaySummary));


const StaffBookingChange = dynamic(() => import("./staff-booking-change").then(module => module.StaffBookingChange));





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
export function FrontPmsDailyFrontDeskScreen({context}: {context: FrontPmsDailyContext}) {
  const {staySearch, setStaySearch, base, propertyId, today, query, snapshot, listParams, listRouter, setQuery, tr, visibleStays, guestName, roomName, can, data, showCashierForms, frontHref, showFrontDeskForms, rows, showReservationForms, form, rooms, guests, guestAccounts, onRefresh, rates, language} = context;
  return <FrontLocalizedTree>{(
              <Panel title="Front desk"><UiField>Search guest or booking<TextInput type="search" value={staySearch} onChange={e=>setStaySearch(e.target.value)}/></UiField><PmsOperationsCalendar base={base} propertyId={propertyId!} today={today}/>
                <div className={styles.filters}>
                  {["", "arrivals", "departures", "reserved", "in_house"].map((value) => (
                    <Button
                      variant="plain"
                      type="button"
                      key={value}
                      aria-pressed={query === value}
                      className={`button ${query === value ? "" : "secondary"}`}
                      onClick={() => {if(snapshot?.listMeta){const params=new URLSearchParams(listParams);params.set("status",value);params.set("page","0");params.delete("queue");listRouter.push(`?${params}`);}else setQuery(value);}}
                    >
                      {tr(labelState[value] ?? "All")}
                    </Button>
                  ))}
                </div>
                {visibleStays.map((row) => (
                  <RecordSurface as="article" recordId={row.id}
                    className={styles.record}
                    key={row.id}
                    data-pms-record={row.id}
                    tabIndex={-1}
                  >
                    <div className={styles.line}>
                      <h3>{guestName(row.data.guestId)}</h3>
                      <Badge>{labelState[row.data.state]}</Badge>
                    </div>
                    <p>
                      {roomName(row.data.roomId)} · {row.data.arrival} → {row.data.departure} ·{" "}
                      {row.data.adults} guests
                    </p>
                    <StaySummary snapshot={snapshot!} stay={row} financial={can("pms.cashier")||can("pms.reports")} base={base} documents={data.documents}/>
                    {showCashierForms && row.data.guestAccountId && (
                      <UiActionLink
                        variant="secondary"
                        href={`${frontHref.replace(/\/$/, "")}/guest-accounts?${new URLSearchParams({ property: propertyId ?? "", guestAccount: row.data.guestAccountId })}`}
                      >
                        {tr("Guest Account")}
                      </UiActionLink>
                    )}
                    <PmsRecordDocuments
                      base={base}
                      targetType="pms_stay"
                      targetId={row.id}
                      expectedTargetVersion={row.version}
                      title="Registration and transfer documents"
                      purposes={stayDocumentPurposes}
                      editable={showFrontDeskForms}
                    />
                    {rows("guest")
                      .filter((guest) => guest.id === row.data.guestId)
                      .map((guest) => (
                        <PmsRecordDocuments
                          key={guest.id}
                          base={base}
                          targetType="pms_guest"
                          targetId={guest.id}
                          expectedTargetVersion={guest.version}
                          title="Guest identity and consent"
                          purposes={guestDocumentPurposes}
                          editable={showReservationForms}
                        />
                      ))}
                    {showFrontDeskForms && ["reserved", "in_house"].includes(row.data.state) && (
                      <>
                        {form(
                          row.data.state === "in_house" ? "Move room" : "Assign room",
                          row.data.state === "in_house" ? "stay.move" : "stay.assign",
                          [
                            select("roomId", "Room", rooms),
                            field("reason", "Notes", "text", "", false),
                          ],
                          { id: row.id },
                        )}
                        {row.data.state === "in_house" && form(
                          "Upgrade room", "stay.upgrade", [
                            select("roomId", "Room", rows("room").filter(room => room.data.active && room.data.typeId !== row.data.typeId && room.data.cleanliness === "inspected").map(room => ({ id: room.id, name: `${rows("room_type").find(type => type.id === room.data.typeId)?.data.name ?? ""} · ${room.data.number}` }))),
                            select("rateId", "Rate plan", rows("rate").filter(rate => rate.data.active && rate.data.typeId !== row.data.typeId).map(rate => ({ id: rate.id, name: `${rows("room_type").find(type => type.id === rate.data.typeId)?.data.name ?? ""} · ${rate.data.name}` }))),
                            field("reason", "Reason"),
                          ], { id: row.id },
                        )}
                        {row.data.state === "reserved" ? (
                          <>
                            {form("Check in", "stay.check_in", [], { id: row.id })}
                            {(!row.data.bookingPolicy) && form(
                              "Cancel this stay",
                              "stay.cancel",
                              [field("reason", "Reason"), field("fee", "Fee", "money", 0)],
                              { id: row.id },
                            )}
                            {form(
                              "No-show this stay",
                              "stay.no_show",
                              [field("reason", "Reason"), field("fee", "Fee", "money", 0)],
                              { id: row.id },
                            )}
                          </>
                        ) : (
                          <>
                            {form(
                              "Post room charge before check-out",
                              "stay.preview_checkout",
                              [],
                              { id: row.id },
                            )}
                            {form(
                              "Check out",
                              "stay.check_out",
                              [
                                field("lateFee", "Late check-out fee", "money", 0),
                                field("reason", "Notes", "text", "", false),
                              ],
                              { id: row.id },
                            )}
                          </>
                        )}
                        {form(
                          "Assign guest to this room",
                          "stay.guest",
                          [select("guestId", "Guest", guests, row.data.guestId)],
                          { id: row.id },
                        )}
                        {form(
                          "Route next room charge",
                          "stay.route_guest_account",
                          [select("guestAccountId", "Destination guest account", guestAccounts)],
                          { id: row.id },
                        )}
                        {row.data.state === "reserved" && row.data.bookingPolicy && can("pms.reservations") && <StaffBookingChange base={base} snapshot={snapshot!} stay={row} canOverride={can("pms.approve")} onSaved={onRefresh}/>}
                        {(row.data.state === "in_house" || !row.data.bookingPolicy) && form(
                          "Change dates / extend stay",
                          "stay.change_dates",
                          [
                            field("arrival", "Arrival date", "date", row.data.arrival),
                            field("departure", "New departure date", "date", row.data.departure),
                            select("rateId", "Rate plan", rates, row.data.rateId),
                            field("reason", "Reason"),
                          ],
                          { id: row.id },
                        )}
                      </>
                    )}
                  </RecordSurface>
                ))}
                {!visibleStays.length && (
                  <Empty>
                    {language === "th"
                      ? "ไม่มีผู้เข้าพักในคิวนี้ เลือกคิวอื่นเพื่อดูรายการ"
                      : "No guests in this queue. Choose another view to see other stays."}
                  </Empty>
                )}
              </Panel>
            )}</FrontLocalizedTree>;
}
