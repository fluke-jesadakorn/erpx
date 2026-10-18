"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import type { Field } from "./workspace-types";
import { RecordSurface } from "@/components/records/record-surface";
import { ActionLink as UiActionLink, Field as UiField, TextInput } from "@/components/ui/foundation";
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
export function FrontPmsDailyReservationsScreen({context}: {context: FrontPmsDailyContext}) {
  const {tr, from, setFrom, showReservationForms, property, base, erpBase, today, rows, send, query, setQuery, reservationRows, guestName, stays, roomName, form} = context;
  return <FrontLocalizedTree>{(
              <>
                <Panel title="Availability and new reservation">
                  {showReservationForms && property && (
                    <div className="ui-actions">
                      <UiActionLink href={canonicalUiHref(`${erpBase}/pms/reservations?property=${encodeURIComponent(property.id)}&queue=groups`)}>
                        {tr("Manage group allocations")}
                      </UiActionLink>
                    </div>
                  )}
                  <UiField>
                    {tr("Start date")}{" "}
                    <TextInput
                      type="date"
                      value={from}
                      onChange={(event) => event.target.value && setFrom(event.target.value)}
                    />
                  </UiField>
                  {showReservationForms && (
                    <BookingFlow
                      key={property!.id}
                      slug={base.split("/")[2] ?? ""}
                      today={today}
                      staff={{
                        base,
                        propertyId: property!.id,
                        guests: rows("guest"),
                        blocks: rows("block")
                          .filter(
                            (block) => block.data.kind === "group" && !block.data.bookingChangeId && block.data.state === "active",
                          )
                          .map((block) => ({ id: block.id, name: block.data.name })),
                        save: (input) => send("reservation.create", input),
                      }}
                    />
                  )}
                </Panel>
                <Panel title="Reservations">
                  <TextInput
                    aria-label={tr("Search reservations")}
                    placeholder={tr("Search by name or confirmation number")}
                    value={query}
                    onChange={(event) => setQuery(event.target.value)}
                  />
                  {reservationRows.map((row) => (
                    <RecordSurface as="article" recordId={row.id}
                      className={styles.record}
                      key={row.id}
                      data-pms-record={row.id}
                      tabIndex={-1}
                    >
                      <div className={styles.line}>
                        <strong>{row.data.number}</strong>
                        <Badge>{labelState[row.data.state]}</Badge>
                      </div>
                      <p>
                        {guestName(row.data.guestId)} · {row.data.source}
                      </p>
                      {stays
                        .filter((stay) => stay.data.reservationId === row.id)
                        .map((stay) => (
                          <p className={styles.muted} key={stay.id}>
                            {stay.data.arrival} → {stay.data.departure} ·{" "}
                            {roomName(stay.data.roomId)}
                          </p>
                        ))}
                      <PmsRecordDocuments
                        base={base}
                        targetType="pms_reservation"
                        targetId={row.id}
                        expectedTargetVersion={row.version}
                        title="Reservation documents"
                        purposes={reservationDocumentPurposes}
                        editable={showReservationForms}
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
                      {row.data.state === "confirmed" && showReservationForms && (
                        <div className={styles.actions}>
                          {(!row.data.bookingPolicy) && form(
                            "Cancel reservation",
                            "reservation.cancel",
                            [field("reason", "Reason"), field("fee", "Fee", "money", 0)],
                            { id: row.id },
                          )}
                          {form(
                            "Record no-show",
                            "reservation.no_show",
                            [field("reason", "Reason"), field("fee", "Fee", "money", 0)],
                            { id: row.id },
                          )}
                        </div>
                      )}
                    </RecordSurface>
                  ))}
                  {!reservationRows.length && <Empty>{tr("No reservations found")}</Empty>}
                </Panel>
              </>
            )}</FrontLocalizedTree>;
}
