"use client";
import { FormDisclosure } from "@/components/ui/form-disclosure";
import {SourceSupportChooser} from "@/components/modules/source-support-chooser";
import { ActionLink as UiActionLink } from "@/components/ui/foundation";

import {
  Button,
  Field as UiField,
  Select,
  Table,
  TableFrame,
  TextArea,
  TextInput,
} from "@/components/ui/foundation";

import { FeatureWidget, WidgetGrid } from "@/components/features/feature-page";
import { AttachmentPanel } from "@/components/attachments/attachment-panel";
import { AttachmentFeatureSurface } from "@/components/attachments/attachment-feature-context";
import {
  RecordDocumentPanel,
  type RecordDocumentPurpose,
} from "@/components/attachments/record-document-panel";

import { SetupChecklist } from "@/components/ui/setup-checklist";
import { TaxDocumentReference } from "@/components/hospitality/tax-document-reference";
import { PmsPromptPay } from "./promptpay-counter";
import { decimalToMinor } from "@/lib/finance-money";
import {
  accountLabels,
  visiblePmsSections,
  dates,
  operationPermission,
  plusDays,
  pmsAccountKeys,
  records,
  type DataMap,
  type Kind,
  type PmsRecord,
} from "@/lib/pms-model";
import { useCurrency, useMoneyFormatter } from "@/components/money/currency-provider";
import { availability } from "@/lib/pms-inventory";
import { dailyRoomReport, nightBlockers } from "@/lib/pms-read";
import { workFetch } from "@/lib/work-fetch";
import { TimeText } from "@/components/time/time-preference-context";
import type { PmsWorkspace } from "@/server/pms";
import {
  BedDouble,
  CalendarDays,
  ClipboardCheck,
  Hotel,
  MoonStar,
  Plus,
  RefreshCw,
  Wallet,
} from "lucide-react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import dynamic from "next/dynamic";
const BookingFlow = dynamic(() => import("./booking-flow").then((module) => module.BookingFlow));
const BookingSettings = dynamic(() =>
  import("./booking-settings").then((module) => module.BookingSettings),
);
import styles from "./workspace.module.css";
import { ModuleTiles } from "@/components/hospitality/module-tiles";
import { pmsMenu } from "@/components/hospitality/module-menu";
import { FrontLocalizedTree, useFrontLanguage } from "@/components/front/front-language";
import { frontText } from "@/lib/front-language";

type Option = { id: string; name: string };
function nightAuditMessage(message: string) {
  return message === "PMS_PERIOD_CLOSED"
    ? "Finance needs to check the accounting period for this business date before day close can continue."
    : message;
}
function StatAmount({ formatted }: { formatted: string }) {
  const separator = formatted.indexOf(" · ");
  return separator < 0 ? (
    <>{formatted}</>
  ) : (
    <>
      {formatted.slice(0, separator)}
      <small className={styles.amountContext}> · {formatted.slice(separator + 3)}</small>
    </>
  );
}
type Field = {
  name: string;
  label: string;
  type?:
    "text" | "date" | "time" | "number" | "money" | "textarea" | "checkbox" | "select" | "email";
  options?: Option[];
  value?: string | number | boolean;
  required?: boolean;
  min?: number;
};
type Send = (
  op: string,
  input: unknown,
  version?: number,
  attachmentSetIds?: string[],
) => Promise<boolean>;
type ActionAttachment = {
  base: string;
  targetType: string;
  targetId: string;
  expectedTargetVersion: number;
  purposeKey: string;
  title: string;
  description: string;
};
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
const lostPropertyDocumentPurposes: readonly RecordDocumentPurpose[] = [
  {
    key: "lost_property_photo",
    label: "Lost-property photos",
    description:
      "Attach photos that identify the item and document its condition while held by the hotel.",
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
            base={base}
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
const choices = (items: string[]): Option[] => items.map((id) => ({ id, name: id }));
function Form({
  title,
  op,
  fields = [],
  hidden = {},
  send,
  permissions,
  version,
  transform,
  attachments = [],
  open = false,
}: {
  title: string;
  op: string;
  fields?: Field[];
  hidden?: Record<string, unknown>;
  send: Send;
  permissions: string[];
  version?: number;
  transform?: (v: Record<string, unknown>) => unknown;
  attachments?: ActionAttachment[];
  open?: boolean;
}) {
  const language = useFrontLanguage();
  const localizedFields = fields.map((item) => ({
    ...item,
    label: frontText(item.label, language),
    options: item.options?.map((option) => ({ ...option, name: frontText(option.name, language) })),
  }));
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [success, setSuccess] = useState(false);
  const [roomSets, setRoomSets] = useState(1);
  const roomKeys = ["typeId", "rateId", "arrival", "departure", "adults", "quantity"];
  const roomFields = localizedFields.filter((f) => roomKeys.includes(f.name));
  const displayFields =
    op === "reservation.create"
      ? localizedFields.flatMap((f) =>
          f.name === "typeId"
            ? Array.from({ length: roomSets }, (_, i) =>
                roomFields.map((r) => ({
                  ...r,
                  name: i ? `room_${i}_${r.name}` : r.name,
                  label: `${r.label} · Room set ${i + 1}`,
                })),
              ).flat()
            : roomKeys.includes(f.name)
              ? []
              : [f],
        )
      : localizedFields;
  const editingVersion = useRef<number | undefined>(undefined);
  if (!permissions.includes(operationPermission(op))) return null;
  return (
    <FormDisclosure inline={displayFields.length < 3} dismissDisabled={busy} className={styles.action} open={open || undefined}>
      <summary>
        <Plus size={15} />
        {frontText(title, language)}
      </summary>
      <form
        className={styles.form}
        onChange={() => {
          editingVersion.current ??= version;
          setSuccess(false);
        }}
        onSubmit={async (event) => {
          event.preventDefault();
          const form = event.currentTarget;
          const values = new FormData(form);
          setError("");
          setBusy(true);
          try {
            const v: Record<string, unknown> = { ...hidden };
            for (const f of displayFields) {
              const raw = values.get(f.name);
              v[f.name] =
                f.type === "checkbox"
                  ? raw === "on"
                  : f.type === "money"
                    ? decimalToMinor(String(raw || "0"))
                    : f.type === "number"
                      ? Number(raw)
                      : String(raw ?? "");
            }
            const output = transform ? transform(v) : v;
            if (op === "reservation.create")
              (output as Record<string, unknown>).rooms = Array.from({ length: roomSets }, (_, i) =>
                Object.fromEntries(roomKeys.map((k) => [k, v[i ? `room_${i}_${k}` : k]])),
              );
            const ok = await send(
              op,
              output,
              editingVersion.current ?? version,
              values.getAll("attachmentSetId").map(String).filter(Boolean),
            );
            if (ok) {
              setSuccess(true);
              editingVersion.current = undefined;
              form.reset();
              setRoomSets(1);
            } else setError("Not saved. Check the message above; your form data is preserved.");
          } catch (e) {
            setError(e instanceof Error ? e.message : "Invalid data.");
          } finally {
            setBusy(false);
          }
        }}
      >
        {displayFields.map((f) => (
          <UiField
            orientation="inline"
            key={f.name}
            className={f.type === "textarea" ? styles.wide : undefined}
          >
            {f.label}
            {f.required && f.type !== "checkbox" ? " *" : ""}
            {f.type === "select" ? (
              <Select name={f.name} defaultValue={String(f.value ?? "")} required={f.required}>
                <option value="">Select {f.label}</option>
                {f.options?.map((o) => (
                  <option key={o.id} value={o.id}>
                    {o.name}
                  </option>
                ))}
              </Select>
            ) : f.type === "textarea" ? (
              <TextArea
                name={f.name}
                rows={3}
                defaultValue={String(f.value ?? "")}
                required={f.required}
              />
            ) : f.type === "checkbox" ? (
              <TextInput name={f.name} type="checkbox" defaultChecked={Boolean(f.value)} />
            ) : (
              <TextInput
                name={f.name}
                type={f.type === "money" ? "number" : (f.type ?? "text")}
                step={f.type === "money" ? "0.01" : f.type === "number" ? "1" : undefined}
                min={["number", "money"].includes(f.type ?? "") ? (f.min ?? 0) : undefined}
                defaultValue={String(f.value ?? "")}
                required={f.required}
              />
            )}
          </UiField>
        ))}
        {attachments.map((attachment) => (
          <AttachmentPanel
            key={`${attachment.purposeKey}:${attachment.targetId}`}
            {...attachment}
            componentKind="action"
            componentRef={op}
            fieldName="attachmentSetId"
            bindMode="action"
            visibility="Visible only to hotel staff authorized for this property and action."
          />
        ))}
        {op === "reservation.create" && (
          <div className={styles.formFoot}>
            <Button
              variant="secondary"

              type="button"
              disabled={busy || roomSets >= 20}
              onClick={() => setRoomSets((n) => n + 1)}
            >
              Add another room type / stay segment
            </Button>
            {roomSets > 1 && (
              <Button
                variant="secondary"
                type="button"

                onClick={() => setRoomSets((n) => n - 1)}
              >
                Remove last room set
              </Button>
            )}
          </div>
        )}
        <div className={styles.formFoot}>
          <Button clientManaged type="submit" variant="primary" disabled={busy}>
            {frontText(busy ? "Saving…" : title, language)}
          </Button>
          {error && (
            <Button
              variant="secondary"
              type="button"

              onClick={() => {
                editingVersion.current = version;
                setError("Review the form and save again to use the latest version.");
              }}
            >
              Review and use latest version
            </Button>
          )}
          {success && <span role="status">{frontText("Saved", language)}</span>}
          {error && <span role="alert">{error}</span>}
        </div>
      </form>
    </FormDisclosure>
  );
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

type FrontPmsForm = (
  title: string,
  op: string,
  fields?: Field[],
  hidden?: Record<string, unknown>,
  transform?: (v: Record<string, unknown>) => unknown,
  attachments?: ActionAttachment[],
) => ReactNode;
type FrontPmsRows = <K extends Kind>(kind: K) => PmsRecord<K>[];

function FrontPmsDaily({
  data,
  base,
  erpBase,
  frontHref,
  section,
  snapshot,
  property,
  propertyId,
  selectedRecord,
  today,
  from,
  setFrom,
  query,
  setQuery,
  setFolioId,
  currentFolio,
  rows,
  roomName,
  guestName,
  rooms,
  guests,
  rates,
  folios,
  stays,
  openShifts,
  payments,
  can,
  form,
  send,
  error,
  notice,
  onRefresh,
  onPropertyChange,
}: {
  data: PmsWorkspace;
  base: string;
  erpBase: string;
  frontHref: string;
  section: string;
  snapshot: NonNullable<PmsWorkspace["snapshot"]> | null;
  property: NonNullable<PmsWorkspace["snapshot"]>["property"] | undefined;
  propertyId?: string;
  selectedRecord?: string;
  today: string;
  from: string;
  setFrom: (value: string) => void;
  query: string;
  setQuery: (value: string) => void;
  setFolioId: (value: string) => void;
  currentFolio?: PmsRecord<"folio">;
  rows: FrontPmsRows;
  roomName: (id: string) => string;
  guestName: (id: string) => string;
  rooms: Option[];
  guests: Option[];
  rates: Option[];
  folios: Option[];
  stays: PmsRecord<"stay">[];
  openShifts: Option[];
  payments: Field[];
  can: (permission: string) => boolean;
  form: FrontPmsForm;
  send: Send;
  error: string;
  notice: string;
  onRefresh: () => void;
  onPropertyChange: (value: string) => void;
}) {
  const language = useFrontLanguage();
  const formatSelectedMoney = useMoneyFormatter();
  const money = (value: number) =>
    formatSelectedMoney(value, property?.config.currency ?? "THB", today, 2);
  const tr = (value: string) => frontText(value, language);
  const roomRows = rows("room");
  const taskRows = [...rows("task")].sort(
    (a, b) =>
      Number(b.data.priority === "urgent") - Number(a.data.priority === "urgent") ||
      a.data.date.localeCompare(b.data.date) ||
      roomName(a.data.roomId).localeCompare(roomName(b.data.roomId), undefined, { numeric: true }),
  );
  const visibleStays = stays.filter((row) =>
    query === "arrivals"
      ? row.data.state === "reserved" && row.data.arrival === today
      : query === "departures"
        ? row.data.state === "in_house" && row.data.departure <= today
        : !query || row.data.state === query,
  );
  const reservationRows = rows("reservation").filter((row) =>
    `${row.data.number} ${guestName(row.data.guestId)} ${row.data.externalRef} ${row.data.source}`
      .toLowerCase()
      .includes(query.toLowerCase()),
  );
  const operationalNightBlockers = snapshot
    ? [
        ...stays
          .filter(
            (row) =>
              (row.data.state === "reserved" && row.data.arrival <= today) ||
              (row.data.state === "in_house" && row.data.departure <= today),
          )
          .map((row) =>
            row.data.state === "reserved"
              ? `${guestName(row.data.guestId)} · ${tr("Needs check-in or no-show")}`
              : `${guestName(row.data.guestId)} · ${tr("Checkout due")}`,
          ),
        ...rows("shift")
          .filter((row) => row.data.state === "open")
          .map((row) => `${row.data.name} · ${tr("Close cash shift")}`),
      ]
    : [];
  const canRecoverBlockedNight =
    section === "night-audit" &&
    property?.state === "closing" &&
    can("pms.audit") &&
    rows("night").some((row) => row.data.state === "blocked");
  const canOperate =
    Boolean(data.frontReady ?? (property && property.state === "open")) || canRecoverBlockedNight;
  const showReservationForms = can("pms.reservations");
  const showFrontDeskForms = can("pms.frontdesk");
  const showCashierForms = can("pms.cashier");
  const showHousekeepingForms = can("pms.housekeeping");
  const showInspectionForms = can("pms.inspect");
  const showMaintenanceForms = can("pms.maintenance");
  return (
    <FrontLocalizedTree>
      <div className={styles.workspace} data-source="live" data-front-daily="pms">
        <SourceSupportChooser base={erpBase} selectedId={selectedRecord} records={(snapshot?.records??[]).map(record=>({id:record.id,version:record.version,label:`${record.kind} · ${record.id}`}))}/>
        <header className={styles.header}>
          <div>
            <p className={styles.eyebrow}>{tr("Today's work")}</p>
            <h1>
              <Hotel size={28} /> {property?.name ?? tr("Hotel")}
            </h1>
            <p className={styles.subtitle}>{tr("Guests, rooms, payments and day close")}</p>
          </div>
          <div className={styles.headerTools}>
            {property && (
              <span>
                {tr("Business date")} {today} · {property.config.currency}
              </span>
            )}
            <Button
              variant="secondary"
              type="button"
              onClick={onRefresh}
              aria-label={tr("Refresh")}
            >
              <RefreshCw size={16} />
            </Button>
          </div>
        </header>
        <div className={styles.toolbar}>
          <UiField>
            {tr("Hotel")}
            <Select
              aria-label={tr("Hotel")}
              value={propertyId ?? property?.id ?? ""}
              onChange={(event) => onPropertyChange(event.target.value)}
            >
              <option value="">{tr("Select hotel")}</option>
              {data.properties.map((item) => (
                <option key={item.id} value={item.id}>
                  {item.name}
                </option>
              ))}
            </Select>
          </UiField>
        </div>
        {error && (
          <div className={styles.error} role="alert">
            {error}
          </div>
        )}
        {notice && (
          <div className={styles.notice} role="status">
            {notice}
          </div>
        )}
        {!canOperate ? (
          <section className={styles.setupCard} role="status">
            <h2>{tr("Not ready for daily work")}</h2>
            <p>
              {!property
                ? tr("No hotel has been set up for this company yet.")
                : property.state === "setup"
                  ? tr("This hotel has not been opened for daily work yet.")
                  : property.state === "closing"
                    ? tr(
                        "This hotel is closing its business day. Ask the hotel administrator to finish the day close.",
                      )
                    : tr("Complete these hotel setup steps before daily work can begin.")}
            </p>
            {data.readiness.length > 0 && (
              <ul>
                {data.readiness.map((item) => (
                  <li key={item}>{tr(item)}</li>
                ))}
              </ul>
            )}
            {property?.state === "setup" && data.readiness.length === 0 && (
              <p>{tr("The hotel administrator must verify readiness and open the hotel.")}</p>
            )}
            <p>
              {tr("Ask your hotel administrator or Finance approver to complete these steps.")}
              {property ? ` · ${tr("Hotel")}: ${property.name} (${property.id.slice(0, 8)})` : ""}
            </p>
          </section>
        ) : (
          <>
            {section === "dashboard" && (
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
                      count: rows("folio").filter(
                        (row) => row.data.state !== "closed" && row.data.due > 0,
                      ).length,
                      action: language === "th" ? "เปิดบัญชีผู้เข้าพัก" : "Open guest folios",
                      href: "folios",
                      show: showCashierForms,
                    },
                  ]
                    .filter((item) => item.show)
                    .map((item) => (
                      <Link
                        className={styles.frontQueue}
                        key={item.href + item.label}
                        href={frontHref + item.href}
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
            )}

            {section === "reservations" && (
              <>
                <Panel title="Availability and new reservation">
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
                            (block) => block.data.kind === "group" && block.data.state === "active",
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
                    <article
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
                          {form(
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
                    </article>
                  ))}
                  {!reservationRows.length && <Empty>{tr("No reservations found")}</Empty>}
                </Panel>
              </>
            )}

            {section === "front-desk" && (
              <Panel title="Front desk">
                <div className={styles.filters}>
                  {["", "arrivals", "departures", "reserved", "in_house"].map((value) => (
                    <Button
                      variant="plain"
                      type="button"
                      key={value}
                      aria-pressed={query === value}
                      className={`button ${query === value ? "" : "secondary"}`}
                      onClick={() => setQuery(value)}
                    >
                      {tr(labelState[value] ?? "All")}
                    </Button>
                  ))}
                </div>
                {visibleStays.map((row) => (
                  <article
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
                    {showCashierForms && row.data.folioId && (
                      <UiActionLink
                        variant="secondary"
                        href={`${frontHref.replace(/\/$/, "")}/folios?${new URLSearchParams({ property: propertyId ?? "", folio: row.data.folioId })}`}
                      >
                        {tr("Guest folio")}
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
                        {row.data.state === "reserved" ? (
                          <>
                            {form("Check in", "stay.check_in", [], { id: row.id })}
                            {form(
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
                          "stay.route_folio",
                          [select("folioId", "Destination folio", folios)],
                          { id: row.id },
                        )}
                        {form(
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
                  </article>
                ))}
                {!visibleStays.length && (
                  <Empty>
                    {language === "th"
                      ? "ไม่มีผู้เข้าพักในคิวนี้ เลือกคิวอื่นเพื่อดูรายการ"
                      : "No guests in this queue. Choose another view to see other stays."}
                  </Empty>
                )}
              </Panel>
            )}

            {section === "rooms" && (
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
            )}

            {section === "housekeeping" && (
              <>
                <Panel title="Housekeeping tasks">
                  {showHousekeepingForms && (
                    <details
                      className={styles.taskCreation}
                      open={roomRows.some((row) => row.id === selectedRecord)}
                    >
                      <summary>{tr("Create housekeeping task")}</summary>
                      {roomRows.map(
                        (row) =>
                          showHousekeepingForms && (
                            <div key={row.id} data-pms-record={row.id} tabIndex={-1}>
                              {form(
                                `${tr("Create housekeeping task")} · ${tr("Room")} ${row.data.number}`,
                                "task.create",
                                [
                                  select(
                                    "type",
                                    "Task type",
                                    [
                                      { id: "arrival", name: "Before arrival" },
                                      { id: "stayover", name: "During stay" },
                                      { id: "departure", name: "After check-out" },
                                    ],
                                    "arrival",
                                  ),
                                  field("date", "Work date", "date", today),
                                ],
                                { roomId: row.id },
                              )}
                            </div>
                          ),
                      )}
                    </details>
                  )}
                  {taskRows
                    .filter(
                      (row) =>
                        row.id === selectedRecord ||
                        !["done", "superseded"].includes(row.data.state),
                    )
                    .map((row) => (
                      <article
                        className={styles.record}
                        key={row.id}
                        data-pms-record={row.id}
                        tabIndex={-1}
                      >
                        <div className={styles.line}>
                          <h3>
                            {tr("Room")} {roomName(row.data.roomId)} · {labelState[row.data.type]}
                          </h3>
                          <Badge>{labelState[row.data.state]}</Badge>
                        </div>
                        <p>
                          {row.data.date} ·{" "}
                          {row.data.priority === "urgent" ? tr("Urgent") : tr("Normal")}
                        </p>
                        {showInspectionForms &&
                          form(
                            "Assign task",
                            "task.assign",
                            [
                              select("assigneeId", "Staff member", data.staff),
                              select(
                                "priority",
                                "Priority",
                                [
                                  { id: "normal", name: "Normal" },
                                  { id: "urgent", name: "Urgent" },
                                ],
                                row.data.priority,
                              ),
                            ],
                            { id: row.id },
                          )}
                        {showHousekeepingForms &&
                          row.data.state === "pending" &&
                          form("Start task", "task.start", [], { id: row.id })}
                        {showHousekeepingForms &&
                          ["pending", "working"].includes(row.data.state) &&
                          form(
                            "Mark task complete",
                            "task.complete",
                            [
                              ...row.data.checklist.map((name, index) =>
                                field(`check_${index}`, name, "checkbox", false),
                              ),
                              field("notes", "Notes", "textarea", "", false),
                            ],
                            { id: row.id },
                            (value) => ({
                              ...value,
                              checked: row.data.checklist.filter(
                                (_, index) => value[`check_${index}`],
                              ),
                            }),
                            [
                              {
                                base,
                                targetType: "pms_task",
                                targetId: row.id,
                                expectedTargetVersion: row.version,
                                purposeKey: "room_photo",
                                title: "Room completion photos",
                                description:
                                  "Attach room-condition photos taken while completing this task.",
                              },
                              {
                                base,
                                targetType: "pms_task",
                                targetId: row.id,
                                expectedTargetVersion: row.version,
                                purposeKey: "damage_photo",
                                title: "Damage evidence",
                                description:
                                  "Attach damage photos when the completed task found an exception.",
                              },
                            ],
                          )}
                        {showInspectionForms &&
                          row.data.state === "clean" &&
                          (row.data.completedBy === data.userId ? (
                            <p className={styles.notice}>
                              {language === "th"
                                ? "ส่งตรวจแล้ว ให้พนักงานอีกคนตรวจรับห้องนี้"
                                : "Ready for review. Another staff member must inspect this room."}
                            </p>
                          ) : (
                            form("Inspect task", "task.inspect", [], { id: row.id })
                          ))}
                      </article>
                    ))}
                  {!taskRows.some((row) => !["done", "superseded"].includes(row.data.state)) && (
                    <Empty>{tr("No open room tasks")}</Empty>
                  )}
                </Panel>
                <Panel title="Rooms under maintenance">
                  {rows("block")
                    .filter((row) => row.data.kind === "maintenance" && row.data.state === "active")
                    .map((row) => (
                      <div
                        className={styles.record}
                        key={row.id}
                        data-pms-record={row.id}
                        tabIndex={-1}
                      >
                        <strong>
                          {tr("Room")} {roomName(row.data.roomId)} · {row.data.name}
                        </strong>
                        <p>
                          {row.data.from} → {row.data.to}
                        </p>
                        {showMaintenanceForms &&
                          form("Reopen room after inspection", "maintenance.close", [], {
                            id: row.id,
                          })}
                      </div>
                    ))}
                </Panel>
              </>
            )}

            {section === "folios" && showCashierForms && (
              <Panel title="Guest folio">
                <UiField className={styles.folioPicker}>
                  {tr("Select folio")}{" "}
                  <Select
                    value={currentFolio?.id ?? ""}
                    onChange={(event) => setFolioId(event.target.value)}
                  >
                    {folios.map((item) => (
                      <option key={item.id} value={item.id}>
                        {item.name}
                      </option>
                    ))}
                  </Select>
                </UiField>
                {currentFolio ? (
                  <>
                    <h3>{guestName(currentFolio.data.guestId)}</h3>
                    <div className={styles.stats}>
                      {[
                        ["Balance due", currentFolio.data.due],
                        ["Advance", currentFolio.data.advance],
                        ["Deposit", currentFolio.data.deposit],
                      ].map(([label, value]) => (
                        <div className={styles.stat} key={String(label)}>
                          <span>{tr(String(label))}</span>
                          <strong>
                            <StatAmount formatted={money(Number(value))} />
                          </strong>
                        </div>
                      ))}
                    </div>
                    <Badge>{labelState[currentFolio.data.state]}</Badge>
                    <TableFrame
                      label="Records"
                      className={`${styles.tableWrap} ${styles.folioEntries}`}
                    >
                      <Table>
                        <thead>
                          <tr>
                            <th>{tr("Date")}</th>
                            <th>{tr("Item")}</th>
                            <th>{tr("Amount")}</th>
                            <th>{tr("Correction")}</th>
                          </tr>
                        </thead>
                        <tbody>
                          {rows("entry")
                            .filter((entry) => entry.data.folioId === currentFolio.id)
                            .map((entry) => {
                              const pending = data.chargeCorrections.find(
                                (item) => item.entry_id === entry.id && item.status === "pending",
                              );
                              const reversed = rows("entry").some(
                                (item) => item.data.originalId === entry.id,
                              );
                              const eligible =
                                entry.data.method !== "opening" &&
                                ["charge", "transfer"].includes(entry.data.kind) &&
                                entry.data.total > 0 &&
                                !entry.data.sourceKey?.startsWith("pos") &&
                                !reversed;
                              return (
                                <tr key={entry.id}>
                                  <td>{entry.data.date}</td>
                                  <td>
                                    {entry.data.description}
                                    <small className={styles.muted}>
                                      {entry.data.kind} · {entry.data.reference}
                                    </small>
                                  </td>
                                  <td>
                                    <StatAmount formatted={money(entry.data.total)} />
                                  </td>
                                  <td>
                                    {pending
                                      ? tr("Awaiting independent review")
                                      : eligible
                                        ? form(
                                            "Request reversal",
                                            "charge.correction.request",
                                            [field("reason", "Reason")],
                                            { id: entry.id },
                                          )
                                        : null}
                                  </td>
                                </tr>
                              );
                            })}
                        </tbody>
                      </Table>
                    </TableFrame>
                    {propertyId && (
                      <UiActionLink
                        variant="secondary"
                        href={`${erpBase}/pms/folios?property=${encodeURIComponent(propertyId)}&queue=corrections`}
                      >
                        {tr("Manage folio corrections in ERPX")}
                      </UiActionLink>
                    )}
                    {currentFolio.data.state !== "closed" &&
                      form(
                        "Receive payment / advance",
                        "payment.receive",
                        [
                          field("amount", "Amount", "money", currentFolio.data.due / 100),
                          ...payments,
                        ],
                        { folioId: currentFolio.id },
                      )}
                    {currentFolio.data.state !== "closed" &&
                      form(
                        "Receive deposit",
                        "payment.deposit",
                        [field("amount", "Amount", "money", 0), ...payments],
                        { folioId: currentFolio.id },
                      )}
                    {currentFolio.data.state !== "closed" &&
                      openShifts.length > 0 &&
                      form(
                        "Record bank transfer pending verification",
                        "payment.request_transfer",
                        [
                          field("amount", "Transfer amount", "money", currentFolio.data.due / 100),
                          field("reference", "Bank transfer reference"),
                          select(
                            "kind",
                            "Transfer purpose",
                            [
                              { id: "receipt", name: "Room payment" },
                              { id: "deposit", name: "Security deposit" },
                            ],
                            "receipt",
                          ),
                          select("shiftId", "Cashier shift", openShifts, openShifts[0]?.id),
                        ],
                        { folioId: currentFolio.id },
                      )}
                    {data.transferRequests
                      .filter(
                        (item) =>
                          item.folio_id === currentFolio.id && item.recorded_by === data.userId,
                      )
                      .map((item) => (
                        <article
                          className={styles.record}
                          key={item.id}
                          data-pms-record={item.id}
                          tabIndex={-1}
                        >
                          <strong>
                            {money(Number(item.amount_minor))} · {item.reference}
                          </strong>
                          <Badge
                            tone={
                              item.state === "verified"
                                ? "good"
                                : item.state === "pending"
                                  ? "warn"
                                  : undefined
                            }
                          >
                            {labelState[item.state] ?? item.state}
                          </Badge>
                          {item.state === "pending" &&
                            form("Cancel transfer request", "payment.cancel_transfer", [], {
                              id: item.id,
                              expectedTransferVersion: Number(item.version),
                            })}
                        </article>
                      ))}
                    {data.chargeCorrections
                      .filter((item) => item.folio_id === currentFolio.id)
                      .map((item) => (
                        <article
                          className={styles.record}
                          key={item.id}
                          data-pms-record={item.id}
                          tabIndex={-1}
                        >
                          <strong>
                            {tr("Correction")} · {item.status}
                          </strong>
                          <p>{item.reason}</p>
                          {item.review_reason && <p>{item.review_reason}</p>}
                        </article>
                      ))}
                    {data.documents
                      .filter((document) => document.folio_id === currentFolio.id)
                      .map((document) => (
                        <a
                          className={styles.record}
                          key={document.id}
                          href={`${erpBase}/pms/documents/${document.id}?property=${propertyId}`}
                        >
                          {tr(
                            document.kind === "receipt"
                              ? "Receipt"
                              : document.kind === "refund"
                                ? "Refund"
                                : "Invoice",
                          )}{" "}
                          · <TimeText value={document.created_at} />
                        </a>
                      ))}
                    {currentFolio.data.state !== "closed" &&
                      form(
                        "Add service charge",
                        "charge.add",
                        [field("description", "Item"), field("price", "Price", "money", 0)],
                        { folioId: currentFolio.id },
                      )}
                    {currentFolio.data.state !== "closed" &&
                      form(
                        "Apply deposit to balance due",
                        "payment.apply_deposit",
                        [field("amount", "Amount", "money", 0), field("reason", "Reason")],
                        { folioId: currentFolio.id },
                      )}
                    {currentFolio.data.state !== "closed" &&
                      form(
                        "Request refund",
                        "refund.request",
                        [
                          select(
                            "pocket",
                            "Fund type",
                            [
                              { id: "advance", name: "Advance" },
                              { id: "deposit", name: "Deposit" },
                            ],
                            "advance",
                          ),
                          field("amount", "Amount", "money", 0),
                          field("reason", "Reason"),
                        ],
                        { folioId: currentFolio.id },
                      )}
                    {rows("refund")
                      .filter((row) => row.data.folioId === currentFolio.id)
                      .map((row) => (
                        <article
                          className={styles.record}
                          key={row.id}
                          data-pms-record={row.id}
                          tabIndex={-1}
                        >
                          <strong>
                            {money(row.data.amount)} · {row.data.reason}
                          </strong>
                          <Badge>{labelState[row.data.state]}</Badge>
                          {row.data.state === "approved" &&
                            form("Pay refund", "refund.pay", payments, { id: row.id })}
                        </article>
                      ))}
                    {currentFolio.data.state !== "closed" &&
                      form("Close folio", "folio.close", [], { id: currentFolio.id })}
                  </>
                ) : (
                  <Empty>{tr("The folio will be created when the reservation is confirmed")}</Empty>
                )}
              </Panel>
            )}

            {section === "cashier" && showCashierForms && (
              <Panel title="Cashier shifts">
                {form("Open cash shift", "shift.open", [
                  field("name", "Shift / till name"),
                  field("opening", "Opening float", "money", 0),
                ])}
                {rows("shift").map((row) => (
                  <article
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
                  </article>
                ))}
                {!rows("shift").length && <Empty>{tr("No cash shifts yet")}</Empty>}
              </Panel>
            )}

            {section === "night-audit" && can("pms.audit") && (
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
                    <article
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
                    </article>
                  ))}
              </Panel>
            )}
          </>
        )}
      </div>
    </FrontLocalizedTree>
  );
}

export function PmsWorkspaceView({
  initial,
  base,
  erpBase = base,
  section,
  selectedFolio,
  crossLinks,
  frontMode = false,
  frontHref,
  frontPosHref,
  initialQueue,
  selectedRecord,
}: {
  initial: PmsWorkspace;
  base: string;
  erpBase?: string;
  section: string;
  selectedFolio?: string;
  crossLinks: { inventory: boolean; finance: boolean; pos: boolean };
  frontMode?: boolean;
  frontHref?: string;
  frontPosHref?: string;
  initialQueue?: string;
  selectedRecord?: string;
}) {
  const language = useFrontLanguage();
  const formatSelectedMoney = useMoneyFormatter();
  const { displayCurrency } = useCurrency();
  const tr = useCallback((value: string) => frontText(value, language), [language]);
  const router = useRouter();
  const [data, setData] = useState(initial),
    [error, setError] = useState(""),
    [query, setQuery] = useState(initialQueue ?? ""),
    [folioId, setFolioId] = useState(
      () =>
        selectedFolio ??
        (initial.snapshot
          ? (records(initial.snapshot, "folio").find((row) => row.data.state !== "closed")?.id ??
            records(initial.snapshot, "folio")[0]?.id)
          : "") ??
        "",
    ),
    [from, setFrom] = useState(initial.snapshot?.property.businessDate ?? initial.defaultDate);
  const [notice, setNotice] = useState("");
  const dailyPmsHref = (target: string, params: Record<string, string> = {}) => {
    const root = frontHref ? frontHref + (target ? target : "") : `${base}/pms/${target}`;
    const query = new URLSearchParams(params);
    return query.size ? `${root}?${query}` : root;
  };
  const dailyPosHref = (tab: string) =>
    `${frontPosHref ?? `${base}/pos`}?tab=${encodeURIComponent(tab)}`;
  const attempts = useRef(new Map<string, { body: string; key: string }>());
  const propertyId = data.snapshot?.property.id;
  const [previousInitial, setPreviousInitial] = useState(initial);
  if (previousInitial !== initial) {
    setPreviousInitial(initial);
    setData(initial);
  }
  useEffect(() => {
    let active = true,
      pending = false;
    const controller = new AbortController();
    async function refresh() {
      if (document.visibilityState !== "visible" || pending) return;
      pending = true;
      try {
        const r = await workFetch(
          `/api/v1${base}/pms?${new URLSearchParams({ ...(propertyId ? { property: propertyId } : {}), ...(section === "night-audit" ? { dayControls: "1" } : {}), ...(section === "dashboard" ? { view: "dashboard" } : {}) })}`,
          { signal: controller.signal },
        );
        if (r.ok) {
          const payload = await r.json();
          if (active) setData(payload.data);
        } else if (active && [401, 403, 409].includes(r.status))
          setError(tr("Your access or role changed. Reload the workspace."));
      } catch {
        if (active) setNotice(tr("Connection lost · Showing the latest loaded data"));
      } finally {
        pending = false;
      }
    }
    const timer = [
      "dashboard",
      "reservations",
      "front-desk",
      "rooms",
      "housekeeping",
      "folios",
      "cashier",
      "inventory",
      "night-audit",
    ].includes(section)
      ? setInterval(refresh, 8000)
      : undefined;
    return () => {
      active = false;
      controller.abort();
      clearInterval(timer);
    };
  }, [base, language, propertyId, section, tr]);
  const send: Send = async (op, input, version, attachmentSetIds = []) => {
    if (op === "reservation.create")
      input = { ...(input as Record<string, unknown>), currency: displayCurrency };
    if (frontMode && (op === "property.create" || op.startsWith("settings."))) {
      setError(tr("Hotel setup is available in ERPX administration."));
      return false;
    }
    const body = JSON.stringify({
      operation: op,
      ...(propertyId && op !== "property.create"
        ? {
            propertyId,
            expectedVersion: version ?? data.snapshot?.property.version,
          }
        : {}),
      input,
      attachmentSetIds,
    });
    const signature = JSON.stringify({ propertyId, op, input, attachmentSetIds });
    if (!attempts.current.has(signature))
      attempts.current.set(signature, { body, key: crypto.randomUUID() });
    const attempt = attempts.current.get(signature)!;
    setError("");
    setNotice("");
    try {
      const response = await workFetch(`/api/v1${base}/pms`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": attempt.key,
        },
        body: attempt.body,
      });
      const json = await response.json();
      if (!response.ok) {
        if (response.status < 500) attempts.current.delete(signature);
        throw new Error(json.error?.message ?? tr("Save failed."));
      }
      attempts.current.delete(signature);
      const pid = json.data.propertyId;
      let refreshed = false;
      try {
        const fresh = await workFetch(
          `/api/v1${base}/pms?${new URLSearchParams({ property: pid, ...(section === "dashboard" ? { view: "dashboard" } : {}) })}`,
        );
        if (fresh.ok) {
          setData((await fresh.json()).data);
          refreshed = true;
        }
      } catch {
        /* The command committed; a failed refresh must not invite resubmission. */
      }
      if (op === "property.create") {
        router.push(`${erpBase}/pms/settings?property=${pid}`);
        router.refresh();
      }
      setNotice(
        refreshed
          ? tr("Saved successfully")
          : tr("Saved successfully · Reload to see the latest result"),
      );
      return true;
    } catch (e) {
      setError(e instanceof Error ? e.message : tr("Could not connect. Try again."));
      return false;
    }
  };
  const s = data.snapshot,
    p = s?.property,
    version = p?.version,
    permissions = data.permissions;
  const money = (value: number) =>
    formatSelectedMoney(value, p?.config.currency ?? "THB", p?.businessDate ?? from, 2);
  const can = (permission: string) => permissions.some((p) => p === permission);
  const form = (
    title: string,
    op: string,
    fields: Field[] = [],
    hidden: Record<string, unknown> = {},
    transform?: (v: Record<string, unknown>) => unknown,
    attachments?: ActionAttachment[],
  ) => (
    <Form
      key={`${op}:${JSON.stringify(hidden)}`}
      title={title}
      op={op}
      fields={fields}
      hidden={hidden}
      send={send}
      permissions={permissions}
      version={version}
      transform={transform}
      attachments={attachments}
      open={!!selectedRecord && (hidden.id === selectedRecord || hidden.roomId === selectedRecord)}
    />
  );
  const rows = <K extends Kind>(kind: K): PmsRecord<K>[] => (s ? records(s, kind) : []);
  useEffect(() => {
    if (!selectedRecord) return;
    const target = document.querySelector<HTMLElement>(
      `[data-pms-record="${CSS.escape(selectedRecord)}"]`,
    );
    if (!target) return;
    target.scrollIntoView({ block: "center" });
    target.focus({ preventScroll: true });
  }, [selectedRecord, propertyId, section]);
  const options = <K extends Kind>(
    kind: K,
    label: (d: DataMap[K], id: string) => string,
  ): Option[] => rows(kind).map((r) => ({ id: r.id, name: label(r.data, r.id) }));
  const roomName = (id: string) =>
    rows("room").find((r) => r.id === id)?.data.number ?? "Room not assigned";
  const guestName = (id: string) => rows("guest").find((r) => r.id === id)?.data.name ?? "Guest";
  const roomTypes = options("room_type", (d) => d.name),
    rooms = options("room", (d) => `${d.number} · ${labelState[d.cleanliness]}`),
    guests = options("guest", (d) => d.name),
    rates = options("rate", (d) => `${d.name} · ${money(d.price)}`),
    folios = options(
      "folio",
      (d) => `${guestName(d.guestId)} · ${d.name} · ${tr("Balance due")} ${money(d.due)}`,
    ),
    stays = rows("stay"),
    today = p?.businessDate ?? from;
  const currentFolio =
    rows("folio").find((r) => r.id === (folioId || selectedFolio)) ??
    rows("folio").find((r) => r.data.state !== "closed") ??
    rows("folio")[0];
  const openShifts = rows("shift")
    .filter((r) => r.data.userId === data.userId && r.data.state === "open")
    .map((r) => ({ id: r.id, name: r.data.name }));
  const payments = [
    select(
      "method",
      "Payment / refund method",
      [
        { id: "cash", name: "Cash" },
        { id: "transfer", name: "Bank transfer" },
        { id: "edc", name: "EDC" },
      ].filter((option) => data.availablePaymentMethods.includes(option.id)),
      "cash",
    ),
    select("shiftId", "Cashier shift", openShifts, openShifts[0]?.id),
    field("reference", "Reference / evidence", "text", "", false),
  ];
  function rateFields(d?: DataMap["rate"]): Field[] {
    return [
      field("name", "Rate plan name", "text", d?.name),
      select("typeId", "Room type", roomTypes, d?.typeId),
      field("from", "Sale start", "date", d?.from ?? today),
      field("to", "Last sale date", "date", d?.to ?? plusDays(today, 365)),
      field("price", "Price per night", "money", (d?.price ?? 0) / 100),
      field("extraAdult", "Extra guest price", "money", (d?.extraAdult ?? 0) / 100),
      field("includedGuests", "Guests included in price", "number", d?.includedGuests ?? 2),
      field("minNights", "Minimum nights", "number", d?.minNights ?? 1),
      field("maxNights", "Maximum nights", "number", d?.maxNights ?? 365),
      select("market", "Market", choices(["public", "company", "agent"]), d?.market ?? "public"),
      select("partyId", "Company / agent", data.parties, d?.partyId, false),
      field("package", "Package / inclusions", "textarea", d?.package ?? "", false),
      field("stopSell", "Stop sell", "checkbox", d?.stopSell),
      field("closedArrival", "Close to arrival", "checkbox", d?.closedArrival),
      field("closedDeparture", "Close to departure", "checkbox", d?.closedDeparture),
      field("active", "Rate plan active", "checkbox", d?.active ?? true),
    ];
  }
  if (frontMode)
    return (
      <FrontPmsDaily
        data={data}
        base={base}
        erpBase={erpBase}
        frontHref={frontHref ?? `${base}/pms/`}
        section={section}
        snapshot={s}
        property={p}
        propertyId={propertyId}
        selectedRecord={selectedRecord}
        today={today}
        from={from}
        setFrom={setFrom}
        query={query}
        setQuery={setQuery}
        setFolioId={setFolioId}
        currentFolio={currentFolio}
        rows={rows}
        roomName={roomName}
        guestName={guestName}
        rooms={rooms}
        guests={guests}
        rates={rates}
        folios={folios}
        stays={stays}
        openShifts={openShifts}
        payments={payments}
        can={can}
        form={form}
        send={send}
        error={error}
        notice={notice}
        onRefresh={() => {
          router.refresh();
          setNotice(tr("Loading the latest data"));
        }}
        onPropertyChange={(next) =>
          router.push(
            `${frontHref ?? `${base}/pms/`}${section === "dashboard" ? "" : section}?property=${encodeURIComponent(next)}`,
          )
        }
      />
    );
  return (
    <FrontLocalizedTree>
      <div className={styles.workspace} data-source="live">
        <header className={styles.header}>
          <div>
            <p className={styles.eyebrow}>{tr("ERPX HOSPITALITY")}</p>
            <h1>
              <Hotel size={28} /> {p?.name ?? tr("Hotel PMS")}
            </h1>
            <p className={styles.subtitle}>
              {tr("Rooms, guest services and accounting in one place")}
            </p>
          </div>
          <div className={styles.headerTools}>
            {p && (
              <>
                <Badge tone={p.state === "open" ? "good" : "warn"}>{labelState[p.state]}</Badge>
                <span>
                  {tr("Business date")} {today} · {p.config.currency}
                </span>
              </>
            )}
            <Button
              type="submit"
              variant="secondary"

              onClick={() => {
                router.refresh();
                setNotice(tr("Loading the latest data"));
              }}
              aria-label={tr("Load the latest data")}
            >
              <RefreshCw size={16} />
            </Button>
          </div>
        </header>
        <ModuleTiles
          label={tr("Hotel tasks")}
          current={section}
          items={pmsMenu(language)
            .filter((item) =>
              visiblePmsSections((permission) =>
                permissions.some((value) => value === permission),
              ).some(([id]) => id === item.id),
            )
            .map((item) => ({
              ...item,
              href: `${base}/pms/${item.id}${propertyId ? `?property=${encodeURIComponent(propertyId)}` : ""}`,
            }))}
        />
        <SourceSupportChooser base={erpBase} selectedId={selectedRecord} records={(s?.records??[]).map(record=>({id:record.id,version:record.version,label:`${record.kind} · ${record.id}`}))}/>
        <WidgetGrid>
          {error && (
            <div className={styles.error} role="alert">
              {error}
            </div>
          )}
          {notice && (
            <div className={styles.notice} role="status">
              {notice}
            </div>
          )}
          <div className={styles.toolbar}>
            <UiField>
              {tr("Hotel")}
              <Select
                aria-label={tr("Hotel")}
                value={propertyId ?? ""}
                onChange={(e) => router.push(`${base}/pms/${section}?property=${e.target.value}`)}
              >
                <option value="">{tr("Select hotel")}</option>
                {data.properties.map((h) => (
                  <option key={h.id} value={h.id}>
                    {h.name}
                  </option>
                ))}
              </Select>
            </UiField>
            {p && (
              <span className={styles.localMode}>
                {tr("Managed in ERPX · OTA reservations are recorded by staff")}
              </span>
            )}
          </div>
          {!p ? (
            <Panel title="Set up your first hotel">
              {!frontMode && (
                <UiActionLink variant="secondary" href={`${erpBase}/hospitality/setup`}>
                  Hotel + Restaurant Setup
                </UiActionLink>
              )}
              <Empty>
                {tr(
                  "Add a hotel and connect it to a branch, then configure rooms, rates and accounts.",
                )}
              </Empty>
              {frontMode ? (
                <UiActionLink variant="secondary" href={`${erpBase}/pms/settings`}>
                  {tr("Set up hotel in ERPX")}
                </UiActionLink>
              ) : (
                <Form
                  open
                  title="Create hotel"
                  op="property.create"
                  permissions={permissions}
                  send={send}
                  fields={[
                    field("name", "Hotel name"),
                    select("branchId", "Branch", data.branches),
                    field("businessDate", "Starting business date", "date", today),
                  ]}
                />
              )}
            </Panel>
          ) : (
            <>
              {p.state === "setup" && (
                <SetupChecklist
                  title={tr("Hotel readiness")}
                  items={
                    data.readiness.length
                      ? data.readiness.map((label, index) => ({
                          id: String(index),
                          label: tr(label),
                          ready: false,
                          owner: tr(
                            /finance|account|tax|ledger/i.test(label)
                              ? "Finance approver"
                              : "Hotel administrator",
                          ),
                          href: `${erpBase}/pms/settings?property=${p.id}`,
                        }))
                      : [
                          {
                            id: "open",
                            label: tr("Verify readiness and open hotel"),
                            ready: false,
                            owner: tr("Hotel administrator"),
                            href: `${erpBase}/pms/settings?property=${p.id}`,
                          },
                        ]
                  }
                />
              )}
              {section === "dashboard" && (
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
              )}
              {section === "reservations" && (
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
                            .filter((b) => b.data.kind === "group" && b.data.state === "active")
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
                      .filter((r) => r.data.kind === "group" && r.data.state === "active")
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
                        <article
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
                                    href={dailyPmsHref("folios", {
                                      property: p.id,
                                      folio: st.data.folioId,
                                    })}
                                  >
                                    Open folio
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

                              href={dailyPmsHref("folios", {
                                property: p.id,
                                folio:
                                  stays.find((st) => st.data.reservationId === r.id)?.data
                                    .folioId ?? "",
                              })}
                            >
                              Folio / deposit
                            </UiActionLink>
                          </div>
                          {r.data.state === "confirmed" && (
                            <>
                              {form(
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
                        </article>
                      ))}
                  </Panel>
                </>
              )}
              {section === "front-desk" && (
                <Panel title="Front Office">
                  <div className={styles.filters}>
                    {["", "arrivals", "departures", "reserved", "in_house", "checked_out"].map(
                      (st) => (
                        <Button
                          type="submit"
                          variant="plain"
                          key={st}
                          className={`button ${query === st ? "" : "secondary"}`}
                          onClick={() => setQuery(st)}
                        >
                          {labelState[st] ?? "All"}
                        </Button>
                      ),
                    )}
                  </div>
                  {stays
                    .filter((r) =>
                      query === "arrivals"
                        ? r.data.state === "reserved" && r.data.arrival === today
                        : query === "departures"
                          ? r.data.state === "in_house" && r.data.departure <= today
                          : !query || r.data.state === query,
                    )
                    .map((r) => (
                      <article
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
                          href={dailyPmsHref("folios", { property: p.id, folio: r.data.folioId })}
                        >
                          Open this folio
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
                            {r.data.state === "reserved" ? (
                              <>
                                {form("Check in", "stay.check_in", [], {
                                  id: r.id,
                                })}
                                {form(
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
                              "Route the next room charge to a folio",
                              "stay.route_folio",
                              [select("folioId", "Destination folio", folios)],
                              { id: r.id },
                            )}
                            {form(
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
                      </article>
                    ))}
                  {!stays.length && <Empty>No guests are currently in house.</Empty>}
                </Panel>
              )}
              {["rooms", "housekeeping"].includes(section) && (
                <>
                  <Panel title={section === "rooms" ? "Rooms & readiness" : "Housekeeping"}>
                    <div className={styles.roomGrid}>
                      {rows("room").map((r) => (
                        <article
                          className={`${styles.room} ${r.data.cleanliness === "inspected" ? styles.ready : ""}`}
                          key={r.id}
                        >
                          <strong>{r.data.number}</strong>
                          <small>
                            {r.data.building} · Floor {r.data.floor}
                          </small>
                          <Badge tone={r.data.cleanliness === "inspected" ? "good" : "warn"}>
                            {labelState[r.data.cleanliness]}
                          </Badge>
                          <span>
                            {stays.some(
                              (st) => st.data.state === "in_house" && st.data.roomId === r.id,
                            )
                              ? "Occupied"
                              : "Vacant"}{" "}
                            {r.data.dnd ? "· DND" : ""}
                          </span>
                          {form(
                            "Do-not-disturb request",
                            "room.dnd",
                            [field("dnd", "DND", "checkbox", r.data.dnd)],
                            { id: r.id },
                          )}
                          {form(
                            "Verify occupancy",
                            "room.observe",
                            [field("occupied", "Guest present", "checkbox", false)],
                            { id: r.id },
                          )}
                          {form(
                            "Create housekeeping task",
                            "task.create",
                            [
                              select(
                                "type",
                                "Task type",
                                [
                                  { id: "arrival", name: "Before arrival" },
                                  { id: "stayover", name: "During stay" },
                                  { id: "departure", name: "After check-out" },
                                ],
                                "arrival",
                              ),
                              field("date", "Work date", "date", today),
                            ],
                            { roomId: r.id },
                          )}
                          {form(
                            "Close room for maintenance",
                            "maintenance.open",
                            [
                              field("from", "Start date", "date", today),
                              field("to", "Expected completion", "date", plusDays(today, 1)),
                              field("reason", "Details"),
                            ],
                            { roomId: r.id },
                          )}
                        </article>
                      ))}
                    </div>
                  </Panel>
                  <Panel title="Tasks">
                    {rows("task")
                      .filter(
                        (r) =>
                          r.id === selectedRecord || !["done", "superseded"].includes(r.data.state),
                      )
                      .sort(
                        (a, b) =>
                          Number(b.data.priority === "urgent") -
                          Number(a.data.priority === "urgent"),
                      )
                      .map((r) => (
                        <article
                          key={r.id}
                          className={styles.record}
                          data-pms-record={r.id}
                          tabIndex={-1}
                        >
                          <div className={styles.line}>
                            <h3>
                              Room {roomName(r.data.roomId)} · {labelState[r.data.type]}
                            </h3>
                            <Badge>{labelState[r.data.state]}</Badge>
                          </div>
                          <p>
                            {r.data.date} ·{" "}
                            {data.staff.find((x) => x.id === r.data.assigneeId)?.name ??
                              "Unassigned"}{" "}
                            · {r.data.priority === "urgent" ? "Urgent" : "Normal"}
                          </p>
                          {form(
                            "Assign task",
                            "task.assign",
                            [
                              select("assigneeId", "Staff member", data.staff),
                              select(
                                "priority",
                                "Priority",
                                [
                                  { id: "normal", name: "Normal" },
                                  { id: "urgent", name: "Urgent" },
                                ],
                                r.data.priority,
                              ),
                            ],
                            { id: r.id },
                          )}
                          {r.data.state === "pending" &&
                            form("Start task", "task.start", [], { id: r.id })}
                          {["pending", "working"].includes(r.data.state) &&
                            form(
                              "Mark task complete",
                              "task.complete",
                              [
                                ...r.data.checklist.map((name, i) =>
                                  field(`check_${i}`, name, "checkbox", false),
                                ),
                                field("notes", "Notes", "textarea", "", false),
                              ],
                              { id: r.id },
                              (v) => ({
                                ...v,
                                checked: r.data.checklist.filter((_, i) => v[`check_${i}`]),
                              }),
                              [
                                {
                                  base,
                                  targetType: "pms_task",
                                  targetId: r.id,
                                  expectedTargetVersion: r.version,
                                  purposeKey: "room_photo",
                                  title: "Room completion photos",
                                  description:
                                    "Attach room-condition photos taken while completing this task.",
                                },
                                {
                                  base,
                                  targetType: "pms_task",
                                  targetId: r.id,
                                  expectedTargetVersion: r.version,
                                  purposeKey: "damage_photo",
                                  title: "Damage evidence",
                                  description:
                                    "Attach damage photos when the completed task found an exception.",
                                },
                              ],
                            )}
                          {r.data.state === "clean" &&
                            form("Inspect task", "task.inspect", [], { id: r.id })}
                        </article>
                      ))}
                    {!rows("task").some((t) => !["done", "superseded"].includes(t.data.state)) && (
                      <Empty>No open room tasks.</Empty>
                    )}
                  </Panel>
                  <Panel title="Rooms under maintenance">
                    {rows("block")
                      .filter((r) => r.data.kind === "maintenance" && r.data.state === "active")
                      .map((r) => (
                        <div
                          className={styles.record}
                          key={r.id}
                          data-pms-record={r.id}
                          tabIndex={-1}
                        >
                          <strong>
                            Room {roomName(r.data.roomId)} · {r.data.name}
                          </strong>
                          <p>
                            {r.data.from} → {r.data.to}
                          </p>
                          {form("Reopen room after inspection", "maintenance.close", [], {
                            id: r.id,
                          })}
                        </div>
                      ))}
                  </Panel>
                </>
              )}
              {section === "folios" &&
                (can("pms.cashier") || can("pms.reports") || can("pms.approve")) && (
                  <>
                    <Panel title="Guest folio">
                      <UiField className={styles.folioPicker}>
                        Select folio
                        <Select
                          value={currentFolio?.id ?? ""}
                          onChange={(e) => setFolioId(e.target.value)}
                        >
                          {folios.map((f) => (
                            <option key={f.id} value={f.id}>
                              {f.name}
                            </option>
                          ))}
                        </Select>
                      </UiField>
                      {currentFolio ? (
                        <>
                          <h3>{guestName(currentFolio.data.guestId)}</h3>
                          <div className={styles.stats}>
                            {[
                              ["Balance due", currentFolio.data.due],
                              ["Advance", currentFolio.data.advance],
                              ["Deposit", currentFolio.data.deposit],
                            ].map(([label, n]) => (
                              <div className={styles.stat} key={String(label)}>
                                <span>{label}</span>
                                <strong>
                                  <StatAmount formatted={money(Number(n))} />
                                </strong>
                              </div>
                            ))}
                          </div>
                          <Badge>{labelState[currentFolio.data.state]}</Badge>
                          <TableFrame
                            label="Records"
                            className={`${styles.tableWrap} ${styles.folioEntries}`}
                          >
                            <Table>
                              <thead>
                                <tr>
                                  <th>Date</th>
                                  <th>Item</th>
                                  <th>Amount</th>
                                  <th>Actions</th>
                                </tr>
                              </thead>
                              <tbody>
                                {rows("entry")
                                  .filter((e) => e.data.folioId === currentFolio.id)
                                  .map((e) => (
                                    <tr key={e.id}>
                                      <td>{e.data.date}</td>
                                      <td>
                                        {e.data.description}
                                        <small className={styles.muted}>
                                          {e.data.kind} · {e.data.reference}
                                        </small>
                                      </td>
                                      <td>
                                        <StatAmount formatted={money(e.data.total)} />
                                      </td>
                                      <td>
                                        {["charge", "transfer"].includes(e.data.kind) &&
                                          e.data.total > 0 && (
                                            <>
                                              {e.data.sourceKey?.startsWith("pos") ? null : rows(
                                                  "entry",
                                                ).some(
                                                  (item) => item.data.originalId === e.id,
                                                ) ? null : data.chargeCorrections.some(
                                                  (item) => item.entry_id === e.id,
                                                ) ? (
                                                <span>Awaiting independent review</span>
                                              ) : (
                                                form(
                                                  "Request reversal",
                                                  "charge.correction.request",
                                                  [field("reason", "Reason")],
                                                  { id: e.id },
                                                )
                                              )}
                                              {form(
                                                "Transfer charge",
                                                "folio.transfer",
                                                [
                                                  select(
                                                    "toFolioId",
                                                    "Destination folio",
                                                    folios.filter((f) => f.id !== currentFolio.id),
                                                  ),
                                                  field("reason", "Reason"),
                                                ],
                                                { entryId: e.id },
                                              )}
                                            </>
                                          )}
                                      </td>
                                    </tr>
                                  ))}
                              </tbody>
                            </Table>
                          </TableFrame>
                          {propertyId && (
                            <UiActionLink
                              variant="secondary"
                              href={`${erpBase}/pms/folios?property=${encodeURIComponent(propertyId)}&queue=corrections`}
                            >
                              Manage folio corrections
                            </UiActionLink>
                          )}
                          <>
                            <div>
                              {form(
                                "Receive payment / advance",
                                "payment.receive",
                                [
                                  field("amount", "Amount", "money", currentFolio.data.due / 100),
                                  ...payments,
                                ],
                                { folioId: currentFolio.id },
                              )}
                              {form(
                                "Receive deposit",
                                "payment.deposit",
                                [field("amount", "Amount", "money", 0), ...payments],
                                { folioId: currentFolio.id },
                              )}
                              {data.stripeConnectionReady && (
                                <PmsPromptPay
                                  base={base}
                                  propertyId={p.id}
                                  folioId={currentFolio.id}
                                  dueMinor={currentFolio.data.due}
                                  initialId={
                                    data.stripePayments.find(
                                      (item) =>
                                        item.folio_id === currentFolio.id &&
                                        ["requested", "pending"].includes(item.state),
                                    )?.id
                                  }
                                  history={data.stripePayments.filter(
                                    (item) => item.folio_id === currentFolio.id,
                                  )}
                                />
                              )}
                              {openShifts.length > 0 &&
                                form(
                                  "Record bank transfer pending verification",
                                  "payment.request_transfer",
                                  [
                                    field(
                                      "amount",
                                      "Transfer amount",
                                      "money",
                                      currentFolio.data.due / 100,
                                    ),
                                    field("reference", "Bank transfer reference"),
                                    select(
                                      "kind",
                                      "Transfer purpose",
                                      [
                                        { id: "receipt", name: "Room payment" },
                                        { id: "deposit", name: "Security deposit" },
                                      ],
                                      "receipt",
                                    ),
                                    select(
                                      "shiftId",
                                      "Cashier shift",
                                      openShifts,
                                      openShifts[0]?.id,
                                    ),
                                  ],
                                  { folioId: currentFolio.id },
                                )}
                              {data.transferRequests
                                .filter((item) => item.folio_id === currentFolio.id)
                                .map((item) => {
                                  const bankChoices = data.transferBankEvidence.filter(
                                    (bank) =>
                                      bank.external_id === item.reference &&
                                      Number(bank.amount_minor) === Number(item.amount_minor) &&
                                      bank.currency === item.currency &&
                                      bank.transaction_date === item.business_date,
                                  );
                                  return (
                                    <article
                                      className={styles.record}
                                      key={item.id}
                                      data-pms-record={item.id}
                                      tabIndex={-1}
                                    >
                                      <strong>
                                        {money(Number(item.amount_minor))} · {item.reference}
                                      </strong>{" "}
                                      <Badge
                                        tone={
                                          item.state === "verified"
                                            ? "good"
                                            : item.state === "pending"
                                              ? "warn"
                                              : undefined
                                        }
                                      >
                                        {labelState[item.state] ?? item.state}
                                      </Badge>
                                      <p>
                                        Bank transfer{" "}
                                        {item.state === "pending"
                                          ? "recorded; no folio payment has posted"
                                          : item.state === "verified"
                                            ? "verified against imported bank evidence"
                                            : "cancelled without a payment"}
                                        .
                                      </p>
                                      {item.state === "pending" && (
                                        <>
                                          {form(
                                            "Cancel unverified reference",
                                            "payment.cancel_transfer",
                                            [],
                                            {
                                              id: item.id,
                                              expectedTransferVersion: Number(item.version),
                                            },
                                          )}
                                          {can("finance.approve") &&
                                            (bankChoices.length ? (
                                              form(
                                                "Finance: verify imported bank row",
                                                "payment.verify_transfer",
                                                [
                                                  select(
                                                    "bankTransactionId",
                                                    "Imported bank row",
                                                    bankChoices.map((bank) => ({
                                                      id: bank.id,
                                                      name: `${bank.transaction_date} · ${bank.external_id} · ${money(Number(bank.amount_minor))}`,
                                                    })),
                                                  ),
                                                ],
                                                {
                                                  id: item.id,
                                                  expectedTransferVersion: Number(item.version),
                                                },
                                                (value) => ({
                                                  ...value,
                                                  bankVersion: Number(
                                                    bankChoices.find(
                                                      (bank) => bank.id === value.bankTransactionId,
                                                    )?.version ?? 0,
                                                  ),
                                                }),
                                              )
                                            ) : (
                                              <p>
                                                No exact imported bank row is available.{" "}
                                                {crossLinks.finance && (
                                                  <Link href={`${erpBase}/finance/bank`}>
                                                    Review bank evidence in Finance
                                                  </Link>
                                                )}
                                              </p>
                                            ))}
                                        </>
                                      )}
                                    </article>
                                  );
                                })}
                              {form(
                                "Add service charge",
                                "charge.add",
                                [field("description", "Item"), field("price", "Price", "money", 0)],
                                { folioId: currentFolio.id },
                              )}
                              {form(
                                "Apply deposit to balance due",
                                "payment.apply_deposit",
                                [field("amount", "Amount", "money", 0), field("reason", "Reason")],
                                { folioId: currentFolio.id },
                              )}
                              {form(
                                "Request refund",
                                "refund.request",
                                [
                                  select(
                                    "pocket",
                                    "Fund type",
                                    [
                                      { id: "advance", name: "Advance" },
                                      { id: "deposit", name: "Deposit" },
                                    ],
                                    "advance",
                                  ),
                                  field("amount", "Amount", "money", 0),
                                  field("reason", "Reason"),
                                ],
                                { folioId: currentFolio.id },
                              )}
                            </div>
                            <div>
                              {form(
                                "Create separate folio",
                                "folio.create",
                                [field("name", "Folio name")],
                                { reservationId: currentFolio.data.reservationId },
                              )}
                              {form(
                                "Merge folios",
                                "folio.merge",
                                [
                                  select(
                                    "toFolioId",
                                    "Destination folio",
                                    folios.filter((f) => f.id !== currentFolio.id),
                                  ),
                                ],
                                { id: currentFolio.id },
                              )}
                              {form(
                                "Transfer company / agent receivable",
                                "folio.credit",
                                [
                                  select(
                                    "creditId",
                                    "Approved credit",
                                    rows("credit")
                                      .filter((c) => c.data.state === "approved")
                                      .map((c) => ({
                                        id: c.id,
                                        name: `${data.parties.find((p) => p.id === c.data.partyId)?.name ?? c.data.partyId} · ${money(c.data.limit)}`,
                                      })),
                                  ),
                                ],
                                { id: currentFolio.id },
                              )}
                              {form(
                                "Record OTA / company settlement",
                                "ota.settle",
                                [
                                  field(
                                    "gross",
                                    "Receivable settled",
                                    "money",
                                    currentFolio.data.due / 100,
                                  ),
                                  field("commission", "Commission", "money", 0),
                                  field("fees", "Fees", "money", 0),
                                  field("reference", "Reference"),
                                  select("shiftId", "Cashier shift", openShifts, openShifts[0]?.id),
                                ],
                                { folioId: currentFolio.id },
                                undefined,
                                [
                                  {
                                    base,
                                    targetType: "pms_property",
                                    targetId: p.id,
                                    expectedTargetVersion: p.version,
                                    purposeKey: "ota_reconciliation",
                                    title: "OTA reconciliation evidence",
                                    description:
                                      "Attach the OTA or agent settlement evidence used for this posting.",
                                  },
                                ],
                              )}
                              {form("Issue invoice / close folio", "folio.close", [], {
                                id: currentFolio.id,
                              })}
                            </div>
                          </>
                        </>
                      ) : (
                        <Empty>The folio will be created when the reservation is confirmed.</Empty>
                      )}
                    </Panel>
                    <Panel title="Refunds & credit">
                      {can("pms.approve") && (
                        <UiActionLink
                          variant="secondary"
                          href={`${erpBase}/pms/cashier?property=${propertyId}&queue=approval`}
                        >
                          {tr("Review cashier shifts")}
                        </UiActionLink>
                      )}
                      {!rows("refund").some((r) => !["paid", "rejected"].includes(r.data.state)) &&
                        !rows("credit").some((r) => r.data.state === "requested") && (
                          <Empty>No refund or credit requests available for review.</Empty>
                        )}
                      {rows("refund")
                        .filter((r) => r.data.state !== "paid" && r.data.state !== "rejected")
                        .map((r) => (
                          <article
                            className={styles.record}
                            key={r.id}
                            data-pms-record={r.id}
                            tabIndex={-1}
                          >
                            <strong>
                              {money(r.data.amount)} · {r.data.reason}
                            </strong>
                            <Badge>{labelState[r.data.state]}</Badge>
                            {r.data.state === "requested" ? (
                              <>
                                {form("Approve refund", "refund.approve", [], {
                                  id: r.id,
                                })}
                                {form("Reject", "refund.reject", [], {
                                  id: r.id,
                                })}
                              </>
                            ) : (
                              form("Pay refund", "refund.pay", payments, {
                                id: r.id,
                              })
                            )}
                          </article>
                        ))}
                      {form("Request company / agent credit", "credit.request", [
                        select("partyId", "Customer", data.parties),
                        field("limit", "Credit limit", "money", 0),
                        field("terms", "Credit days", "number", 30),
                      ])}
                      {rows("credit")
                        .filter((r) => r.data.state === "requested")
                        .map((r) => (
                          <div
                            key={r.id}
                            className={styles.record}
                            data-pms-record={r.id}
                            tabIndex={-1}
                          >
                            <strong>
                              {data.parties.find((p) => p.id === r.data.partyId)?.name} ·{" "}
                              {money(r.data.limit)}
                            </strong>
                            {form("Approve credit", "credit.approve", [], {
                              id: r.id,
                            })}
                            {form("Reject credit", "credit.reject", [], {
                              id: r.id,
                            })}
                          </div>
                        ))}
                    </Panel>
                    <Panel title="Folio charge corrections">
                      {data.chargeCorrections.map((item) => (
                        <article
                          className={styles.record}
                          key={item.id}
                          data-pms-record={item.id}
                          tabIndex={-1}
                        >
                          <strong>
                            {item.entry_description} · {money(Number(item.entry_total))}
                          </strong>
                          <p>
                            Requested by {item.requested_by} · {item.reason}
                          </p>
                          <Badge>Awaiting independent review</Badge>
                          {can("pms.approve") &&
                            item.requested_by !== data.userId &&
                            form(
                              "Review correction",
                              "charge.correction.review",
                              [
                                select("action", "Decision", [
                                  { id: "approve", name: "Approve reversal" },
                                  { id: "reject", name: "Return request" },
                                ]),
                                field("reason", "Review reason"),
                              ],
                              { id: item.id, expectedVersion: Number(item.version) },
                            )}
                        </article>
                      ))}
                      {!data.chargeCorrections.length && (
                        <Empty>No folio corrections are awaiting review.</Empty>
                      )}
                    </Panel>
                    {data.bookingRefunds.length > 0 && (
                      <Panel title="Online booking refunds for Finance">
                        {data.bookingRefunds.map((item) => (
                          <article
                            className={styles.record}
                            key={item.id}
                            data-pms-record={item.id}
                            tabIndex={-1}
                          >
                            <strong>
                              {money(item.amount_minor)} · {item.id.slice(0, 8)}
                            </strong>
                            <Badge>{item.refund_state}</Badge>
                            {item.last_error && <span>{item.last_error}</span>}
                            {item.refund_state === "requested" && (
                              <Button
                                type="submit"
                                variant="primary"
                                onClick={async () => {
                                  setError("");
                                  try {
                                    const response = await workFetch(
                                      `/api/v1${base}/pms/booking-refunds`,
                                      {
                                        method: "POST",
                                        headers: { "Content-Type": "application/json" },
                                        body: JSON.stringify({ id: item.id }),
                                      },
                                    );
                                    const result = await response.json();
                                    if (!response.ok)
                                      throw new Error(result.error?.message ?? "Approval failed");
                                    setNotice("Refund approved; waiting for Stripe confirmation");
                                    router.refresh();
                                  } catch (error) {
                                    setError(
                                      error instanceof Error ? error.message : "Approval failed",
                                    );
                                  }
                                }}
                              >
                                Approve Stripe refund
                              </Button>
                            )}
                          </article>
                        ))}
                      </Panel>
                    )}
                    <Panel title="Documents">
                      {!currentFolio?.data.invoiceId &&
                        !data.documents.some(
                          (d) => !currentFolio || d.folio_id === currentFolio.id,
                        ) && <Empty>No documents available for this folio yet.</Empty>}
                      {currentFolio?.data.invoiceId && (
                        <TaxDocumentReference
                          base={erpBase}
                          invoiceId={currentFolio.data.invoiceId}
                        />
                      )}
                      {data.documents
                        .filter((d) => !currentFolio || d.folio_id === currentFolio.id)
                        .map((d) => (
                          <Link
                            className={styles.line}
                            key={d.id}
                            href={`${erpBase}/pms/documents/${d.id}?property=${p.id}`}
                            target="_blank"
                          >
                            <span>
                              {d.kind} · {d.id.slice(0, 8)}
                            </span>
                            <span>
                              <TimeText value={d.created_at} />
                            </span>
                          </Link>
                        ))}
                    </Panel>
                  </>
                )}
              {section === "cashier" && (
                <Panel title="Cashier shifts">
                  {form("Open shift", "shift.open", [
                    field("name", "Shift / till name"),
                    field("opening", "Opening float", "money", 0),
                  ])}
                  {rows("shift").map((r) => (
                    <article
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
                    </article>
                  ))}
                </Panel>
              )}
              {section === "inventory" && (
                <>
                  <Panel title="Minibar, supplies & linen">
                    {crossLinks.inventory && (
                      <p className={styles.muted}>
                        Shared inventory:{" "}
                        <Link href={`${erpBase}/commerce?view=stock`}>Stock & movements</Link> ·{" "}
                        <Link href={`${erpBase}/commerce?view=products`}>Products</Link> ·{" "}
                        <Link href={`${erpBase}/commerce?view=warehouses`}>Warehouses</Link>
                      </p>
                    )}
                    {form("Add item", "item.save", [
                      field("name", "Item name"),
                      select(
                        "kind",
                        "Type",
                        [
                          { id: "minibar", name: "Minibar" },
                          { id: "supply", name: "Consumable supply" },
                          { id: "linen", name: "Linen" },
                        ],
                        "minibar",
                      ),
                      select("productId", "Inventory product", data.products),
                      select("warehouseId", "Warehouse", data.warehouses),
                      field("price", "Sale price", "money", 0),
                    ])}
                    {!rows("item").length && (
                      <p className={styles.empty}>{tr("No items available in this view.")}</p>
                    )}
                    {rows("item").map((r) => (
                      <article
                        className={styles.record}
                        key={r.id}
                        data-pms-record={r.id}
                        tabIndex={-1}
                      >
                        <h3>{r.data.name}</h3>
                        {r.data.kind === "linen" && (
                          <p>
                            Clean {r.data.clean} · Issued {r.data.issued} · In laundry{" "}
                            {r.data.laundry} · Lost {r.data.lost}
                          </p>
                        )}
                        {(r.data.kind === "linen"
                          ? [
                              ["linen.receive", "Receive linen from warehouse"],
                              ["linen.issue", "Issue linen to room"],
                              ["linen.return", "Receive used linen / send to laundry"],
                              ["linen.clean", "Receive clean laundry"],
                              ["linen.loss", "Record linen loss"],
                            ]
                          : r.data.kind === "minibar"
                            ? [["minibar.charge", "Post minibar charge to folio"]]
                            : [["supply.consume", "Consume supply"]]
                        ).map(([op, label]) => (
                          <div key={op}>
                            {form(
                              label,
                              op,
                              [
                                field("quantity", "Quantity", "number", 1),
                                select("roomId", "Room", rooms, "", false),
                                ...(op === "minibar.charge"
                                  ? [select("folioId", "Folio", folios)]
                                  : []),
                                field("reason", "Notes / reason", "text", "", op === "linen.loss"),
                              ],
                              { itemId: r.id },
                            )}
                          </div>
                        ))}
                      </article>
                    ))}
                  </Panel>
                  <Panel title="Lost & found">
                    {form("Record lost item", "lost.save", [
                      field("description", "Item description"),
                      select("roomId", "Found in room", rooms),
                      select("guestId", "Guest", guests, "", false),
                      field("location", "Storage location"),
                      field("foundDate", "Date found", "date", today),
                      field("notes", "Notes", "textarea", "", false),
                    ])}
                    {!rows("lost").length && (
                      <p className={styles.empty}>
                        {tr("No lost-property records available in this view.")}
                      </p>
                    )}
                    {rows("lost").map((r) => (
                      <div
                        className={styles.record}
                        key={r.id}
                        data-pms-record={r.id}
                        tabIndex={-1}
                      >
                        <strong>{r.data.description}</strong>
                        <p>
                          Room {roomName(r.data.roomId)} · {r.data.location} ·{" "}
                          {labelState[r.data.state]}
                        </p>
                        <PmsRecordDocuments
                          base={base}
                          targetType="pms_lost_property"
                          targetId={r.id}
                          expectedTargetVersion={r.version}
                          title="Lost-property photos"
                          purposes={lostPropertyDocumentPurposes}
                          editable={can("pms.housekeeping")}
                        />
                        {r.data.state === "stored" &&
                          form(
                            "Return / dispose",
                            "lost.release",
                            [
                              select(
                                "state",
                                "Outcome",
                                [
                                  { id: "returned", name: "Returned to owner" },
                                  { id: "disposed", name: "Disposed" },
                                ],
                                "returned",
                              ),
                              field("notes", "Recipient and evidence"),
                            ],
                            { id: r.id },
                            undefined,
                            [
                              {
                                base,
                                targetType: "pms_lost_property",
                                targetId: r.id,
                                expectedTargetVersion: r.version,
                                purposeKey: "handover_evidence",
                                title: "Handover evidence",
                                description:
                                  "Attach the signed handover or disposal evidence for this action.",
                              },
                            ],
                          )}
                      </div>
                    ))}
                  </Panel>
                  <Panel title="Movement history">
                    {!rows("movement").length && (
                      <p className={styles.empty}>
                        {tr("No stock movements available in this view.")}
                      </p>
                    )}
                    {rows("movement")
                      .slice(-30)
                      .reverse()
                      .map((r) => (
                        <div className={styles.line} key={r.id}>
                          <span>
                            {r.data.date} ·{" "}
                            {rows("item").find((i) => i.id === r.data.itemId)?.data.name}
                          </span>
                          <span>
                            {r.data.kind} · {r.data.quantity}
                          </span>
                        </div>
                      ))}
                  </Panel>
                </>
              )}
              {section === "guests" && (
                <Panel title="Guest directory">
                  {form("Add guest", "guest.save", [
                    field("name", "Full name"),
                    field("email", "Email", "email", "", false),
                    field("phone", "Phone", "text", "", false),
                    field("nationality", "Nationality", "text", "", false),
                    field("documentRef", "Document reference", "text", "", false),
                    field("notes", "Preferences / notes", "textarea", "", false),
                  ])}
                  <TextInput
                    aria-label="Search guests"
                    placeholder="Search by name or phone"
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                  />
                  {!rows("guest").some((r) => `${r.data.name} ${r.data.phone}`.includes(query)) && (
                    <p className={styles.empty}>
                      {tr("No matching guest records available in this view.")}
                    </p>
                  )}
                  {rows("guest")
                    .filter((r) => `${r.data.name} ${r.data.phone}`.includes(query))
                    .map((r) => (
                      <article
                        className={styles.record}
                        key={r.id}
                        data-pms-record={r.id}
                        tabIndex={-1}
                      >
                        <h3>{r.data.name}</h3>
                        {[r.data.phone, r.data.email, r.data.nationality].some(Boolean) && (
                          <p>
                            {[r.data.phone, r.data.email, r.data.nationality]
                              .filter(Boolean)
                              .join(" · ")}
                          </p>
                        )}
                        {r.data.notes && <p>{r.data.notes}</p>}
                        <p className={styles.muted}>
                          History {stays.filter((s) => s.data.guestId === r.id).length} stays
                        </p>
                        {stays
                          .filter((st) => st.data.guestId === r.id)
                          .map((st) => (
                            <div className={styles.line} key={st.id}>
                              <span>
                                {st.data.arrival} → {st.data.departure}
                              </span>
                              <span>{roomName(st.data.roomId)}</span>
                              <Badge>{labelState[st.data.state]}</Badge>
                              <Link
                                href={dailyPmsHref("folios", {
                                  property: p.id,
                                  folio: st.data.folioId,
                                })}
                              >
                                Stay folio
                              </Link>
                            </div>
                          ))}
                        {form(
                          "Edit guest",
                          "guest.save",
                          [
                            field("name", "Full name", "text", r.data.name),
                            field("email", "Email", "email", r.data.email, false),
                            field("phone", "Phone", "text", r.data.phone, false),
                            field("nationality", "Nationality", "text", r.data.nationality, false),
                            field(
                              "documentRef",
                              "Document reference",
                              "text",
                              r.data.documentRef,
                              false,
                            ),
                            field("notes", "Notes", "textarea", r.data.notes, false),
                          ],
                          { id: r.id },
                        )}
                      </article>
                    ))}
                </Panel>
              )}
              {section === "night-audit" && (can("pms.audit") || can("pms.reports")) && (
                <>
                  <Panel title="Night Audit">
                    <div className={styles.auditHero}>
                      <MoonStar size={38} />
                      <div>
                        <h3>
                          {tr("Close business day")} {today}
                        </h3>
                        <p>Review activity, post room charges and start the next day.</p>
                      </div>
                    </div>
                    {p.state === "open" && (
                      <>
                        {data.dayControls && <RestaurantDayControls controls={data.dayControls} money={money}>
                          {crossLinks.pos && <>
                            <Link href={dailyPosHref("shifts")}>Review restaurant cash sessions</Link>{" "}·{" "}
                            <Link href={dailyPosHref("history")}>Review restaurant receipts</Link>
                          </>}
                        </RestaurantDayControls>}
                        {nightBlockers(s!).length ? (
                          <ul className={styles.blockers}>
                            {nightBlockers(s!).map((b) => (
                              <li key={b}>{b}</li>
                            ))}
                          </ul>
                        ) : (
                          !data.dayControls?.blockers.length && (
                            <p className={styles.notice}>Ready to close the day.</p>
                          )
                        )}
                        {!nightBlockers(s!).length &&
                          !data.dayControls?.blockers.length &&
                          form("Start day close", "night.start")}
                      </>
                    )}
                    {rows("night")
                      .slice()
                      .reverse()
                      .map((r) => (
                        <article
                          className={styles.record}
                          key={r.id}
                          data-pms-record={r.id}
                          tabIndex={-1}
                        >
                          <strong>{r.data.date}</strong>{" "}
                          <Badge tone={r.data.state === "completed" ? "good" : "warn"}>
                            {labelState[r.data.state]}
                          </Badge>
                          <p>{tr(nightAuditMessage(r.data.message))}</p>
                          {r.data.state === "blocked" &&
                            form("Resolve and continue", "night.resume", [], {
                              id: r.id,
                            })}
                        </article>
                      ))}
                  </Panel>
                </>
              )}
              {section === "reports" && can("pms.reports") && (
                <>
                  {crossLinks.finance && (
                    <p className={styles.muted}>
                      Review shared accounting in{" "}
                      <Link href={`${erpBase}/finance/accounting`}>Finance Accounting</Link>.
                    </p>
                  )}
                  <Panel title="Daily performance">
                    <UiField>
                      Date{" "}
                      <TextInput
                        type="date"
                        value={from}
                        onChange={(e) => {
                          if (e.target.value) setFrom(e.target.value);
                        }}
                      />
                    </UiField>
                    {(() => {
                      const { sold, capacity, roomRevenue } = dailyRoomReport(s!, from);
                      const entries = rows("entry").filter(
                        (e) =>
                          e.data.date === from && ["charge", "adjustment"].includes(e.data.kind),
                      );
                      const net = entries.reduce((n, e) => n + e.data.net + e.data.service, 0);
                      return (
                        <div className={styles.stats}>
                          {[
                            [
                              "Occupancy",
                              `${capacity ? ((sold / capacity) * 100).toFixed(1) : "0"}%`,
                            ],
                            ["ADR", money(sold ? Math.round(roomRevenue / sold) : 0)],
                            ["RevPAR", money(capacity ? Math.round(roomRevenue / capacity) : 0)],
                            ["Net room revenue", money(net)],
                            ["Tax", money(entries.reduce((n, e) => n + e.data.tax, 0))],
                          ].map(([label, value]) => (
                            <div className={styles.stat} key={label}>
                              <span>{label}</span>
                              <strong>{value}</strong>
                            </div>
                          ))}
                        </div>
                      );
                    })()}
                  </Panel>
                  <Panel title="PMS vs GL">
                    <TableFrame label="Records" className={styles.tableWrap}>
                      <Table>
                        <thead>
                          <tr>
                            <th>Control account</th>
                            <th>PMS</th>
                            <th>GL</th>
                            <th>Variance</th>
                          </tr>
                        </thead>
                        <tbody>
                          {(["due", "advance", "deposit"] as const).map((key) => {
                            const n = rows("folio").reduce((n, f) => n + f.data[key], 0),
                              gl = data.ledger?.[key === "due" ? "receivable" : key] ?? 0;
                            return (
                              <tr key={key}>
                                <th>
                                  {key === "due"
                                    ? "Receivable"
                                    : key === "advance"
                                      ? "Advance"
                                      : "Deposit"}
                                </th>
                                <td>{money(n)}</td>
                                <td>{money(gl)}</td>
                                <td>{money(n - gl)}</td>
                              </tr>
                            );
                          })}
                        </tbody>
                      </Table>
                    </TableFrame>
                    <p>
                      Debit {money(data.ledger?.debit ?? 0)} · Credit{" "}
                      {money(data.ledger?.credit ?? 0)}
                    </p>
                  </Panel>
                  <Panel title="Receivables & balances">
                    {rows("folio")
                      .filter((r) => r.data.due || r.data.advance || r.data.deposit)
                      .map((r) => (
                        <Link
                          className={styles.line}
                          key={r.id}
                          href={dailyPmsHref("folios", { property: p.id, folio: r.id })}
                        >
                          <strong>{r.data.name}</strong>
                          <span>
                            {r.data.partyId
                              ? data.parties.find((p) => p.id === r.data.partyId)?.name
                              : "Guest"}
                          </span>
                          <span>
                            Due {money(r.data.due)} · Advance {money(r.data.advance)} · Deposit{" "}
                            {money(r.data.deposit)}
                          </span>
                        </Link>
                      ))}
                  </Panel>
                </>
              )}
              {section === "settings" && (
                <>
                  {(can("pms.configure") || can("pms.accounting.approve")) && (
                    <Panel title="Hotel cutover">
                      <Link href={`${erpBase}/pms/import?property=${p.id}`}>
                        Preview imported hotel records and Finance-approved opening balances →
                      </Link>
                    </Panel>
                  )}
                  {can("pms.configure") && (
                    <BookingSettings
                      key={p.id}
                      snapshot={s!}
                      save={send}
                      slug={base.split("/")[2]}
                    />
                  )}
                  {can("pms.configure") && data.bookingDelivery.length > 0 && (
                    <Panel title="Booking email delivery">
                      {data.bookingDelivery.map((email) => (
                        <div className={styles.record} key={email.id}>
                          <p>
                            {
                              rows("reservation").find((r) => r.id === email.reservation_id)?.data
                                .number
                            }{" "}
                            · {email.state} · {email.attempts} attempts
                          </p>
                          {email.state === "failed" &&
                            form("Retry email", "booking.retry_email", [], {
                              id: email.id,
                            })}
                        </div>
                      ))}
                    </Panel>
                  )}
                  <Panel title="Hotel details">
                    {!can("pms.configure") && (
                      <>
                        <dl className={styles.settingsOverview}>
                          {[
                            ["Hotel name", p.name],
                            ["Country code", p.config.country],
                            ["Time zone", p.config.timezone],
                            ["Business date", p.businessDate],
                            ["Check-in time", p.config.checkIn],
                            ["Check-out time", p.config.checkOut],
                          ].map(([label, value]) => (
                            <div key={label}>
                              <dt>{tr(label)}</dt>
                              <dd>{value}</dd>
                            </div>
                          ))}
                        </dl>
                        {p.config.cancellationPolicy && (
                          <p className={styles.muted}>
                            {tr("Cancellation policy")}: {p.config.cancellationPolicy}
                          </p>
                        )}
                      </>
                    )}
                    {form("Save hotel", "property.save", [
                      field("name", "Hotel name", "text", p.name),
                      field("country", "Country code", "text", p.config.country),
                      field("timezone", "Time zone", "text", p.config.timezone),
                      field("checkIn", "Check-in time", "time", p.config.checkIn),
                      field("checkOut", "Check-out time", "time", p.config.checkOut),
                      field(
                        "cancellationPolicy",
                        "Cancellation policy",
                        "textarea",
                        p.config.cancellationPolicy,
                        false,
                      ),
                    ])}
                    {form("Add another hotel", "property.create", [
                      field("name", "Hotel name"),
                      select("branchId", "Branch", data.branches),
                      field("businessDate", "Starting business date", "date", today),
                    ])}
                  </Panel>
                  <>
                    <Panel title="Room types">
                      {form("Add room type", "room_type.save", [
                        field("name", "Type name"),
                        field("capacity", "Maximum guests", "number", 2),
                        field("amenities", "Amenities", "textarea", "", false),
                      ])}
                      {rows("room_type").map((r) => (
                        <div
                          className={styles.record}
                          key={r.id}
                          data-pms-record={r.id}
                          tabIndex={-1}
                        >
                          <strong>{r.data.name}</strong> · {r.data.capacity} guests
                          {form(
                            "Edit room type",
                            "room_type.save",
                            [
                              field("name", "Type name", "text", r.data.name),
                              field("capacity", "Maximum guests", "number", r.data.capacity),
                              field("amenities", "Amenities", "textarea", r.data.amenities, false),
                            ],
                            { id: r.id },
                          )}
                        </div>
                      ))}
                    </Panel>
                    <Panel title="Rooms">
                      {form("Add room", "room.save", [
                        field("number", "Room number"),
                        select("typeId", "Type", roomTypes),
                        field("building", "Building", "text", "", false),
                        field("floor", "Floor", "text", "", false),
                        field("active", "Room active", "checkbox", true),
                      ])}
                      {rows("room").map((r) => (
                        <div
                          className={styles.record}
                          key={r.id}
                          data-pms-record={r.id}
                          tabIndex={-1}
                        >
                          <strong>{r.data.number}</strong> · {r.data.building} {r.data.floor}
                          {form(
                            "Edit room",
                            "room.save",
                            [
                              field("number", "Room number", "text", r.data.number),
                              select("typeId", "Type", roomTypes, r.data.typeId),
                              field("building", "Building", "text", r.data.building, false),
                              field("floor", "Floor", "text", r.data.floor, false),
                              field("active", "Room active", "checkbox", r.data.active),
                            ],
                            { id: r.id },
                          )}
                        </div>
                      ))}
                    </Panel>
                  </>
                  <Panel title="Rate plans">
                    {form("Add rate plan", "rate.save", rateFields())}
                    {rows("rate").map((r) => (
                      <div
                        className={styles.record}
                        key={r.id}
                        data-pms-record={r.id}
                        tabIndex={-1}
                      >
                        <strong>
                          {r.data.name} · {money(r.data.price)}
                        </strong>
                        <p>
                          {r.data.from} → {r.data.to} · {r.data.package}
                        </p>
                        {form("Edit rate plan", "rate.save", rateFields(r.data), {
                          id: r.id,
                        })}
                        {form(
                          "Date-based price / stop sell",
                          "rate.calendar",
                          [
                            field("from", "Start date", "date", today),
                            field("to", "End date", "date", today),
                            field("price", "Price per night", "money", r.data.price / 100),
                            field("stopSell", "Stop sell", "checkbox", false),
                            field("closedArrival", "Close to arrival", "checkbox", false),
                            field("closedDeparture", "Close to departure", "checkbox", false),
                          ],
                          { id: r.id },
                        )}
                      </div>
                    ))}
                  </Panel>
                  <Panel title="Accounting & tax">
                    <Link href={`${erpBase}/hospitality/setup`}>Hotel + Restaurant Setup</Link>
                    <p>Pilot currency THB · The preparer and approver must be different people.</p>
                    <Badge tone={p.config.approvedBy ? "good" : "warn"}>
                      {p.config.approvedBy ? "Approved" : "Awaiting approval"} · Revision{" "}
                      {p.config.revision}
                    </Badge>
                    {!can("pms.configure") && (
                      <>
                        <dl className={styles.settingsOverview}>
                          {[
                            [
                              "Chart of accounts",
                              data.charts.find((chart) => chart.id === p.config.chartId)?.name ??
                                tr("Not configured"),
                            ],
                            ["Currency", p.config.currency],
                            ["Tax (%)", `${p.config.taxBps / 100}%`],
                            ["Service Charge (%)", `${p.config.serviceBps / 100}%`],
                            [
                              "Sale prices include tax and Service Charge",
                              tr(p.config.taxInclusive ? "Yes" : "No"),
                            ],
                            [
                              "Advance is taxed when received",
                              tr(p.config.advanceTax ? "Yes" : "No"),
                            ],
                          ].map(([label, value]) => (
                            <div key={label}>
                              <dt>{tr(label)}</dt>
                              <dd>{value}</dd>
                            </div>
                          ))}
                        </dl>
                        <details className={styles.settingsMapping} open={!p.config.approvedBy}>
                          <summary>
                            {tr("Account mappings")} · {pmsAccountKeys.length}
                          </summary>
                          <dl className={styles.settingsOverview}>
                            {pmsAccountKeys.map((key) => {
                              const account = data.accounts.find(
                                (row) => row.id === p.config.accounts[key],
                              );
                              return (
                                <div key={key}>
                                  <dt>{tr(accountLabels[key])}</dt>
                                  <dd>
                                    {account
                                      ? `${account.code} · ${account.name}`
                                      : tr("Account unavailable")}
                                  </dd>
                                </div>
                              );
                            })}
                          </dl>
                        </details>
                      </>
                    )}
                    {form(
                      "Save accounting and tax setup",
                      "accounting.save",
                      [
                        select("chartId", "Chart of accounts", data.charts, p.config.chartId),
                        ...pmsAccountKeys.map((k) =>
                          select(
                            k,
                            accountLabels[k],
                            data.accounts.map((a) => ({
                              id: a.id,
                              name: `${a.code} · ${a.name}`,
                            })),
                            p.config.accounts[k],
                          ),
                        ),
                        field("taxBps", "Tax (%)", "money", p.config.taxBps / 100),
                        field(
                          "serviceBps",
                          "Service Charge (%)",
                          "money",
                          p.config.serviceBps / 100,
                        ),
                        field(
                          "taxInclusive",
                          "Sale prices include tax and Service Charge",
                          "checkbox",
                          p.config.taxInclusive,
                        ),
                        field(
                          "advanceTax",
                          "Advance is taxed when received",
                          "checkbox",
                          p.config.advanceTax,
                        ),
                      ],
                      {},
                      (v) => ({
                        ...v,
                        accounts: Object.fromEntries(pmsAccountKeys.map((k) => [k, v[k]])),
                      }),
                    )}
                    {!p.config.approvedBy &&
                      (p.config.preparedBy === data.userId ? (
                        <p className={styles.muted}>
                          {tr("A different Finance reviewer must approve this setup.")}
                        </p>
                      ) : (
                        form("Approve accounting and tax setup", "accounting.approve")
                      ))}
                    {form("Verify readiness and open hotel", "property.enable")}
                  </Panel>
                </>
              )}
            </>
          )}
        </WidgetGrid>
      </div>
    </FrontLocalizedTree>
  );
}
