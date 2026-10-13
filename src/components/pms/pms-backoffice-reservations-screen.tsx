"use client";
import type { Option, Field } from "./workspace-types";
import { RecordSurface } from "@/components/records/record-surface";
import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { Field as UiField, Table, TableFrame, TextInput } from "@/components/ui/foundation";
import { FeatureWidget } from "@/components/shared/features/feature-page";
import { AttachmentFeatureSurface } from "@/components/files/attachment-feature-context";
import { type RecordDocumentPurpose } from "@/components/files/record-document-panel";
import { dates, plusDays } from "@/server/modules/pms/public/rules/pms-model";
import { availability } from "@/server/modules/pms/public/rules/pms-inventory";
import { TimeText } from "@/components/people_payroll/time/time-preference-context";
import Link from "next/link";
import { useState, type ReactNode } from "react";
import dynamic from "next/dynamic";
import styles from "./workspace.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@/components/front/front-language";
import { frontText } from "@/lib/shared/front-language";
import type {BackofficePmsWorkspaceContext} from "./pms-backoffice-workspace";


const RecordDocumentPanel = dynamic(() => import("@/components/files/record-document-panel").then(module => module.RecordDocumentPanel));


const BookingFlow = dynamic(() => import("./booking-flow").then((module) => module.BookingFlow));





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


const reservationDocumentPurposes: readonly RecordDocumentPurpose[] = [
  {
    key: "voucher",
    label: "Reservation voucher",
    description: "Attach the OTA, travel-agent, or customer reservation voucher.",
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
export function BackofficePmsWorkspaceReservationsScreen({context}: {context: BackofficePmsWorkspaceContext & {p: NonNullable<BackofficePmsWorkspaceContext["p"]>}}) {
  const {from, setFrom, rows, s, can, p, base, today, send, form, roomTypes, query, setQuery, guestName, stays, roomName, money, dailyPmsHref, data} = context;
  return <FrontLocalizedTree>{(
                <>
                  <Panel title="Availability by room type">
                    <UiField>
                      Start date{" "}
                      <TextInput
                        type="date"
                        value={from}
                        onChange={(e) => {
                          if (e.target.value) setFrom(e.target.value);
                        }}
                      />
                    </UiField>
                    <TableFrame label="Records" className={styles.tableWrap}>
                      <Table>
                        <thead>
                          <tr>
                            <th>Room type</th>
                            {dates(from, plusDays(from, 14)).map((d) => (
                              <th key={d}>{d.slice(5)}</th>
                            ))}
                          </tr>
                        </thead>
                        <tbody>
                          {rows("room_type").map((r) => (
                            <tr key={r.id}>
                              <th>{r.data.name}</th>
                              {dates(from, plusDays(from, 14)).map((d) => {
                                const a = availability(s!, r.id, d);
                                return (
                                  <td
                                    key={d}
                                    className={a.available <= 0 ? styles.soldOut : ""}
                                    title={`Rooms ${a.capacity} · Booked ${a.occupied} · Group-held ${a.held}`}
                                  >
                                    {a.available}
                                  </td>
                                );
                              })}
                            </tr>
                          ))}
                        </tbody>
                      </Table>
                    </TableFrame>
                  </Panel>
                  <Panel title="Room reservations">
                    {can("pms.reservations") && (
                      <BookingFlow
                        key={p.id}
                        slug={base.split("/")[2]}
                        today={today}
                        staff={{
                          base,
                          propertyId: p.id,
                          guests: rows("guest"),
                          blocks: rows("block")
                            .filter((b) => b.data.kind === "group" && !b.data.bookingChangeId && b.data.state === "active")
                            .map((b) => ({ id: b.id, name: b.data.name })),
                          save: (input) => send("reservation.create", input),
                        }}
                      />
                    )}
                    {form("Create group block", "block.create", [
                      field("name", "Group name"),
                      select("typeId", "Room type", roomTypes),
                      field("from", "Arrival", "date", plusDays(today, 7)),
                      field("to", "Departure", "date", plusDays(today, 8)),
                      field("releaseDate", "Release unused allocation", "date", plusDays(today, 6)),
                      field("quantity", "Room count", "number", 1),
                    ])}
                    {rows("block")
                      .filter((r) => r.data.kind === "group" && !r.data.bookingChangeId && r.data.state === "active")
                      .map((r) => (
                        <div className={styles.line} key={r.id}>
                          <span>
                            {r.data.name} · {r.data.quantity} rooms · Release date{" "}
                            {r.data.releaseDate}
                          </span>
                          {form("Release allocation", "block.release", [], {
                            id: r.id,
                          })}
                        </div>
                      ))}
                  </Panel>
                  <Panel title="Reservations">
                    <TextInput
                      aria-label="Search reservations"
                      placeholder="Search by name, confirmation number or source"
                      value={query}
                      onChange={(e) => setQuery(e.target.value)}
                    />
                    {rows("reservation")
                      .filter((r) =>
                        `${r.data.number} ${guestName(r.data.guestId)} ${r.data.externalRef} ${r.data.source}`
                          .toLowerCase()
                          .includes(query.toLowerCase()),
                      )
                      .map((r) => (
                        <RecordSurface as="article" recordId={r.id}
                          key={r.id}
                          className={styles.record}
                          data-pms-record={r.id}
                          tabIndex={-1}
                        >
                          <div className={styles.line}>
                            <strong>{r.data.number}</strong>
                            <Badge>{labelState[r.data.state]}</Badge>
                          </div>
                          <p>
                            {guestName(r.data.guestId)} · {r.data.source} {r.data.externalRef} ·{" "}
                            {r.data.groupName}
                          </p>
                          <p className={styles.muted}>
                            {stays
                              .filter((st) => st.data.reservationId === r.id)
                              .map(
                                (st) =>
                                  `${st.data.arrival} → ${st.data.departure} (${roomName(st.data.roomId)})`,
                              )
                              .join(" · ")}
                          </p>
                          <PmsRecordDocuments
                            base={base}
                            targetType="pms_reservation"
                            targetId={r.id}
                            expectedTargetVersion={r.version}
                            title="Reservation documents"
                            purposes={reservationDocumentPurposes}
                            editable={can("pms.reservations")}
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
                          <details className={styles.action}>
                            <summary>Reservation details & history</summary>
                            <p>
                              {r.data.bookingPolicy?.text ??
                                "Original hotel terms apply; contact staff for cancellation."}
                            </p>
                            <p>
                              {rows("guest").find((g) => g.id === r.data.guestId)?.data.email} ·{" "}
                              {rows("guest").find((g) => g.id === r.data.guestId)?.data.phone}
                            </p>
                            {stays
                              .filter((st) => st.data.reservationId === r.id)
                              .map((st) => (
                                <div className={styles.line} key={st.id}>
                                  <span>
                                    {roomTypes.find((t) => t.id === st.data.typeId)?.name} ·{" "}
                                    {st.data.adults} adults · {st.data.childAges?.length ?? 0}{" "}
                                    children · {st.data.state} ·{" "}
                                    {money(st.data.prices.reduce((n, p) => n + p.total, 0))}
                                  </span>
                                  <Link
                                    href={dailyPmsHref("guest-accounts", {
                                      property: p.id,
                                      guestAccount: st.data.guestAccountId,
                                    })}
                                  >
                                    Open guest account
                                  </Link>
                                </div>
                              ))}
                            <ol>
                              {r.data.history?.map((h, i) => (
                                <li key={i}>
                                  <TimeText value={h.at} /> · {h.action} ·{" "}
                                  {data.staff.find((st) => st.id === h.actor)?.name ?? h.actor}
                                </li>
                              ))}
                            </ol>
                          </details>
                          <div className={styles.actions}>
                            <UiActionLink
                              variant="secondary"

                              href={dailyPmsHref("front-desk", { property: p.id })}
                            >
                              Assign room / check in
                            </UiActionLink>
                            <UiActionLink
                              variant="secondary"

                              href={dailyPmsHref("guest-accounts", {
                                property: p.id,
                                guestAccount:
                                  stays.find((st) => st.data.reservationId === r.id)?.data
                                    .guestAccountId ?? "",
                              })}
                            >
                              Guest Account / deposit
                            </UiActionLink>
                          </div>
                          {r.data.state === "confirmed" && (
                            <>
                              {(!r.data.bookingPolicy) && form(
                                "Cancel reservation",
                                "reservation.cancel",
                                [field("reason", "Reason"), field("fee", "Fee", "money", 0)],
                                { id: r.id },
                              )}
                              {form(
                                "Record no-show",
                                "reservation.no_show",
                                [field("reason", "Reason"), field("fee", "Fee", "money", 0)],
                                { id: r.id },
                              )}
                            </>
                          )}
                        </RecordSurface>
                      ))}
                  </Panel>
                </>
              )}</FrontLocalizedTree>;
}
