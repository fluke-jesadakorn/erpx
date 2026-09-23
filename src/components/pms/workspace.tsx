"use client";
import { FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";

import { SetupChecklist } from "@/components/ui/setup-checklist";
import { TaxDocumentReference } from "@/components/hospitality/tax-document-reference";
import { decimalToMinor } from "@/lib/finance-money";
import {
accountLabels,
dates,
money,
operationPermission,
plusDays,
pmsAccountKeys,
records,
type DataMap,
type Kind,
type PmsRecord,
} from "@/lib/pms-model";
import { availability } from "@/lib/pms-inventory";
import { dailyRoomReport, nightBlockers } from "@/lib/pms-read";
import { workFetch } from "@/lib/work-fetch";
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
import { useEffect,useRef,useState,type ReactNode } from "react";
import dynamic from "next/dynamic";
const BookingFlow = dynamic(() => import("./booking-flow").then(module => module.BookingFlow));
const BookingSettings = dynamic(() => import("./booking-settings").then(module => module.BookingSettings));
import styles from "./workspace.module.css";

type Option = { id: string; name: string };
type Field = {
  name: string;
  label: string;
  type?:
    | "text"
    | "date"
    | "time"
    | "number"
    | "money"
    | "textarea"
    | "checkbox"
    | "select"
    | "email";
  options?: Option[];
  value?: string | number | boolean;
  required?: boolean;
  min?: number;
};
type Send = (op: string, input: unknown, version?: number) => Promise<boolean>;
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
const choices = (items: string[]): Option[] =>
  items.map((id) => ({ id, name: id }));
function Form({
  title,
  op,
  fields = [],
  hidden = {},
  send,
  permissions,
  version,
  transform,
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
  open?: boolean;
}) {
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [success, setSuccess] = useState(false);
  const [roomSets, setRoomSets] = useState(1);
  const roomKeys = [
    "typeId",
    "rateId",
    "arrival",
    "departure",
    "adults",
    "quantity",
  ];
  const roomFields = fields.filter((f) => roomKeys.includes(f.name));
  const displayFields =
    op === "reservation.create"
      ? fields.flatMap((f) =>
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
      : fields;
  const editingVersion = useRef<number | undefined>(undefined);
  if (!permissions.includes(operationPermission(op))) return null;
  return (
    <details className={styles.action} open={open || undefined}>
      <summary>
        <Plus size={15} />
        {title}
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
              (output as Record<string, unknown>).rooms = Array.from(
                { length: roomSets },
                (_, i) =>
                  Object.fromEntries(
                    roomKeys.map((k) => [k, v[i ? `room_${i}_${k}` : k]]),
                  ),
              );
            const ok = await send(
              op,
              output,
              editingVersion.current ?? version,
            );
            if (ok) {
              setSuccess(true);
              editingVersion.current = undefined;
              form.reset();
              setRoomSets(1);
            } else
              setError(
                "Not saved. Check the message above; your form data is preserved.",
              );
          } catch (e) {
            setError(e instanceof Error ? e.message : "Invalid data.");
          } finally {
            setBusy(false);
          }
        }}
      >
        {displayFields.map((f) => (
          <label
            key={f.name}
            className={f.type === "textarea" ? styles.wide : undefined}
          >
            {f.label}
            {f.required && f.type !== "checkbox" ? " *" : ""}
            {f.type === "select" ? (
              <select
                name={f.name}
                defaultValue={String(f.value ?? "")}
                required={f.required}
              >
                <option value="">Select {f.label}</option>
                {f.options?.map((o) => (
                  <option key={o.id} value={o.id}>
                    {o.name}
                  </option>
                ))}
              </select>
            ) : f.type === "textarea" ? (
              <textarea
                name={f.name}
                rows={3}
                defaultValue={String(f.value ?? "")}
                required={f.required}
              />
            ) : f.type === "checkbox" ? (
              <input
                name={f.name}
                type="checkbox"
                defaultChecked={Boolean(f.value)}
              />
            ) : (
              <input
                name={f.name}
                type={f.type === "money" ? "number" : (f.type ?? "text")}
                step={
                  f.type === "money"
                    ? "0.01"
                    : f.type === "number"
                      ? "1"
                      : undefined
                }
                min={
                  ["number", "money"].includes(f.type ?? "")
                    ? (f.min ?? 0)
                    : undefined
                }
                defaultValue={String(f.value ?? "")}
                required={f.required}
              />
            )}
          </label>
        ))}
        {op === "reservation.create" && (
          <div className={styles.formFoot}>
            <button
              className="button secondary"
              type="button"
              disabled={busy || roomSets >= 20}
              onClick={() => setRoomSets((n) => n + 1)}
            >
              Add another room type / stay segment
            </button>
            {roomSets > 1 && (
              <button
                type="button"
                className="button secondary"
                onClick={() => setRoomSets((n) => n - 1)}
              >
                Remove last room set
              </button>
            )}
          </div>
        )}
        <div className={styles.formFoot}>
          <button className="button" disabled={busy}>
            {busy ? "Saving…" : title}
          </button>
          {error && (
            <button
              type="button"
              className="button secondary"
              onClick={() => {
                editingVersion.current = version;
                setError(
                  "Review the form and save again to use the latest version.",
                );
              }}
            >
              Review and use latest version
            </button>
          )}
          {success && <span role="status">Saved</span>}
          {error && <span role="alert">{error}</span>}
        </div>
      </form>
    </details>
  );
}
const Badge = ({
  children,
  tone = "",
}: {
  children: ReactNode;
  tone?: string;
}) => (
  <span
    className={`${styles.badge} ${tone === "good" ? styles.good : tone === "warn" ? styles.warn : ""}`}
  >
    {children}
  </span>
);
const Empty = ({ children }: { children: ReactNode }) => (
  <div className={styles.empty}>
    <Hotel size={24} />
    <p>{children}</p>
  </div>
);
const Panel = ({ title, children }: { title: string; children: ReactNode }) => (
  <FeatureWidget id={"pms-"+title.toLowerCase().replace(/[^a-z0-9]+/g,"-").replace(/^-|-$/g,"")} title={title}>{children}</FeatureWidget>
);
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

export function PmsWorkspaceView({
  initial,
  base,
  section,
  selectedFolio,
}: {
  initial: PmsWorkspace;
  base: string;
  section: string;
  selectedFolio?: string;
}) {
  const router = useRouter();
  const [data, setData] = useState(initial),
    [error, setError] = useState(""),
    [query, setQuery] = useState(""),
    [folioId, setFolioId] = useState(selectedFolio ?? ""),
    [from, setFrom] = useState(
      initial.snapshot?.property.businessDate ?? initial.defaultDate,
    );
  const [notice, setNotice] = useState("");
  const attempts = useRef(new Map<string, { body: string; key: string }>());
  const propertyId = data.snapshot?.property.id;
  const [previousInitial, setPreviousInitial] = useState(initial);
  if (previousInitial !== initial) {
    setPreviousInitial(initial);
    setData(initial);
  }
  useEffect(() => {
    let active = true, pending = false;
    const controller = new AbortController();
    async function refresh() {
      if (document.visibilityState !== "visible" || pending) return;
      pending=true;
      try {
        const r = await workFetch(
          `/api/v1${base}/pms?${new URLSearchParams({...(propertyId?{property:propertyId}:{}),...(section==='night-audit'?{dayControls:'1'}:{})})}`,
          {signal:controller.signal},
        );
        if (r.ok) {
          const payload = await r.json();
          if (active) setData(payload.data);
        } else if (active && [401, 403, 409].includes(r.status))
          setError("Your access or role changed. Reload the workspace.");
      } catch {
        if (active)
          setNotice("Connection lost · Showing the latest loaded data");
      } finally { pending=false; }
    }
    const timer = ["dashboard","reservations","front-desk","rooms","housekeeping","folios","cashier","inventory","night-audit"].includes(section) ? setInterval(refresh,8000) : undefined;
    return () => {
      active = false;
      controller.abort();
      clearInterval(timer);
    };
  }, [base, propertyId, section]);
  const send: Send = async (op, input, version) => {
    const body = JSON.stringify({
      operation: op,
      ...(propertyId && op !== "property.create"
        ? {
            propertyId,
            expectedVersion: version ?? data.snapshot?.property.version,
          }
        : {}),
      input,
    });
    const signature = JSON.stringify({ propertyId, op, input });
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
        throw new Error(json.error?.message ?? "Save failed.");
      }
      attempts.current.delete(signature);
      const pid = json.data.propertyId;
      let refreshed = false;
      try {
        const fresh = await workFetch(
          `/api/v1${base}/pms?property=${encodeURIComponent(pid)}`,
        );
        if (fresh.ok) {
          setData((await fresh.json()).data);
          refreshed = true;
        }
      } catch {
        /* The command committed; a failed refresh must not invite resubmission. */
      }
      if (op === "property.create") {
        router.push(`${base}/pms/settings?property=${pid}`);
        router.refresh();
      }
      setNotice(
        refreshed
          ? "Saved successfully"
          : "Saved successfully · Reload to see the latest result",
      );
      return true;
    } catch (e) {
      setError(
        e instanceof Error ? e.message : "Could not connect. Try again.",
      );
      return false;
    }
  };
  const s = data.snapshot,
    p = s?.property,
    version = p?.version,
    permissions = data.permissions;
  const can = (permission: string) => permissions.some((p) => p === permission);
  const form = (
    title: string,
    op: string,
    fields: Field[] = [],
    hidden: Record<string, unknown> = {},
    transform?: (v: Record<string, unknown>) => unknown,
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
    />
  );
  const rows = <K extends Kind>(kind: K): PmsRecord<K>[] =>
    s ? records(s, kind) : [];
  const options = <K extends Kind>(
    kind: K,
    label: (d: DataMap[K], id: string) => string,
  ): Option[] =>
    rows(kind).map((r) => ({ id: r.id, name: label(r.data, r.id) }));
  const roomName = (id: string) =>
    rows("room").find((r) => r.id === id)?.data.number ?? "Room not assigned";
  const guestName = (id: string) =>
    rows("guest").find((r) => r.id === id)?.data.name ?? "Guest";
  const roomTypes = options("room_type", (d) => d.name),
    rooms = options(
      "room",
      (d) => `${d.number} · ${labelState[d.cleanliness]}`,
    ),
    guests = options("guest", (d) => d.name),
    rates = options("rate", (d) => `${d.name} · ${money(d.price)}`),
    folios = options("folio", (d) => `${d.name} · Due ${money(d.due)}`),
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
      field(
        "extraAdult",
        "Extra guest price",
        "money",
        (d?.extraAdult ?? 0) / 100,
      ),
      field(
        "includedGuests",
        "Guests included in price",
        "number",
        d?.includedGuests ?? 2,
      ),
      field("minNights", "Minimum nights", "number", d?.minNights ?? 1),
      field("maxNights", "Maximum nights", "number", d?.maxNights ?? 365),
      select(
        "market",
        "Market",
        choices(["public", "company", "agent"]),
        d?.market ?? "public",
      ),
      select("partyId", "Company / agent", data.parties, d?.partyId, false),
      field(
        "package",
        "Package / inclusions",
        "textarea",
        d?.package ?? "",
        false,
      ),
      field("stopSell", "Stop sell", "checkbox", d?.stopSell),
      field("closedArrival", "Close to arrival", "checkbox", d?.closedArrival),
      field(
        "closedDeparture",
        "Close to departure",
        "checkbox",
        d?.closedDeparture,
      ),
      field("active", "Rate plan active", "checkbox", d?.active ?? true),
    ];
  }
  return (
    <main className={styles.workspace} data-source="live">
      <header className={styles.header}>
        <div>
          <p className={styles.eyebrow}>ERPX HOSPITALITY</p>
          <h1>
            <Hotel size={28} /> {p?.name ?? "Hotel PMS"}
          </h1>
          <p className={styles.subtitle}>
            Rooms, guest services and accounting in one place
          </p>
        </div>
        <div className={styles.headerTools}>
          {p && (
            <>
              <Badge tone={p.state === "open" ? "good" : "warn"}>
                {labelState[p.state]}
              </Badge>
              <span>
                Business date {today} · {p.config.currency}
              </span>
            </>
          )}
          <button
            className="button secondary"
            onClick={() => {
              router.refresh();
              setNotice("Loading the latest data");
            }}
            aria-label="Load the latest data"
          >
            <RefreshCw size={16} />
          </button>
        </div>
      </header><FeatureTabs/><WidgetGrid>
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
        <label>
          Hotel
          <select
            aria-label="Hotel"
            value={propertyId ?? ""}
            onChange={(e) =>
              router.push(`${base}/pms/${section}?property=${e.target.value}`)
            }
          >
            <option value="">Select hotel</option>
            {data.properties.map((h) => (
              <option key={h.id} value={h.id}>
                {h.name}
              </option>
            ))}
          </select>
        </label>
        {p && (
          <span className={styles.localMode}>
            Managed in ERPX · OTA reservations are recorded by staff
          </span>
        )}
      </div>
      {!p ? (
        <Panel title="Set up your first hotel">
          <Empty>
            Add a hotel and connect it to a branch, then configure rooms, rates
            and accounts.
          </Empty>
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
        </Panel>
      ) : (
        <>
          
          {p.state === "setup" && <SetupChecklist title="Hotel readiness" items={data.readiness.length ? data.readiness.map((label,index)=>({id:String(index),label,ready:false,owner:/account|tax|ledger/i.test(label)?"Finance approver":"Hotel administrator",href:`${base}/pms/settings?property=${p.id}`})) : [{id:"open",label:"Verify readiness and open hotel",ready:false,owner:"Hotel administrator",href:`${base}/pms/settings?property=${p.id}`}]} />}
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
                    stays.filter(
                      (r) =>
                        r.data.arrival === today && r.data.state === "reserved",
                    ).length,
                  ],
                  [
                    Wallet,
                    "Departing today",
                    stays.filter(
                      (r) =>
                        r.data.departure === today &&
                        r.data.state === "in_house",
                    ).length,
                  ],
                  [
                    ClipboardCheck,
                    "Open room tasks",
                    rows("task").filter(
                      (r) => !["done", "superseded"].includes(r.data.state),
                    ).length,
                  ],
                ].map(([Icon, label, value], i) => {
                  const I = Icon as typeof Hotel;
                  return (
                    <div className={styles.stat} key={i}>
                      <I size={22} />
                      <span>{String(label)}</span>
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
                        (x) =>
                          x.data.state === "in_house" && x.data.roomId === r.id,
                      );
                      return (
                        <Link
                          className={`${styles.room} ${r.data.cleanliness === "inspected" ? styles.ready : ""}`}
                          href={`${base}/pms/rooms?property=${p.id}`}
                          key={r.id}
                        >
                          <strong>{r.data.number}</strong>
                          <span>{stay ? "Occupied" : "Vacant"}</span>
                          <Badge>{labelState[r.data.cleanliness]}</Badge>
                          {r.data.dnd && <small>DND</small>}
                        </Link>
                      );
                    })}
                  </div>
                  {!rows("room").length && (
                    <Empty>Add rooms from Settings.</Empty>
                  )}
                </Panel>
                <Panel title="Today's work">
                  {stays
                    .filter(
                      (r) =>
                        (r.data.arrival === today &&
                          r.data.state === "reserved") ||
                        (r.data.departure === today &&
                          r.data.state === "in_house"),
                    )
                    .map((r) => (
                      <Link
                        className={styles.line}
                        href={`${base}/pms/front-desk?property=${p.id}`}
                        key={r.id}
                      >
                        <strong>{guestName(r.data.guestId)}</strong>
                        <span>{roomName(r.data.roomId)}</span>
                        <Badge>{labelState[r.data.state]}</Badge>
                      </Link>
                    ))}
                  {!stays.length && (
                    <Empty>
                      No reservations yet. Open the hotel to start accepting
                      bookings.
                    </Empty>
                  )}
                  <Link
                    className="button secondary"
                    href={`${base}/pms/reservations?property=${p.id}`}
                  >
                    View all reservations
                  </Link>
                </Panel>
              </>
            </>
          )}
          {section === "reservations" && (
            <>
              <Panel title="Availability by room type">
                <label>
                  Start date{" "}
                  <input
                    type="date"
                    value={from}
                    onChange={(e) => {
                      if (e.target.value) setFrom(e.target.value);
                    }}
                  />
                </label>
                <div className={styles.tableWrap}>
                  <table>
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
                                className={
                                  a.available <= 0 ? styles.soldOut : ""
                                }
                                title={`Rooms ${a.capacity} · Booked ${a.occupied} · Group-held ${a.held}`}
                              >
                                {a.available}
                              </td>
                            );
                          })}
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
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
                        .filter(
                          (b) =>
                            b.data.kind === "group" &&
                            b.data.state === "active",
                        )
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
                  field(
                    "releaseDate",
                    "Release unused allocation",
                    "date",
                    plusDays(today, 6),
                  ),
                  field("quantity", "Room count", "number", 1),
                ])}
                {rows("block")
                  .filter(
                    (r) => r.data.kind === "group" && r.data.state === "active",
                  )
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
                <input
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
                    <article key={r.id} className={styles.record}>
                      <div className={styles.line}>
                        <strong>{r.data.number}</strong>
                        <Badge>{labelState[r.data.state]}</Badge>
                      </div>
                      <p>
                        {guestName(r.data.guestId)} · {r.data.source}{" "}
                        {r.data.externalRef} · {r.data.groupName}
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
                      <details className={styles.action}>
                        <summary>Reservation details & history</summary>
                        <p>
                          {r.data.bookingPolicy?.text ??
                            "Original hotel terms apply; contact staff for cancellation."}
                        </p>
                        <p>
                          {
                            rows("guest").find((g) => g.id === r.data.guestId)
                              ?.data.email
                          }{" "}
                          ·{" "}
                          {
                            rows("guest").find((g) => g.id === r.data.guestId)
                              ?.data.phone
                          }
                        </p>
                        {stays
                          .filter((st) => st.data.reservationId === r.id)
                          .map((st) => (
                            <div className={styles.line} key={st.id}>
                              <span>
                                {
                                  roomTypes.find((t) => t.id === st.data.typeId)
                                    ?.name
                                }{" "}
                                · {st.data.adults} adults ·{" "}
                                {st.data.childAges?.length ?? 0} children ·{" "}
                                {st.data.state} ·{" "}
                                {money(
                                  st.data.prices.reduce(
                                    (n, p) => n + p.total,
                                    0,
                                  ),
                                )}
                              </span>
                              <Link
                                href={`${base}/pms/folios?property=${p.id}&folio=${st.data.folioId}`}
                              >
                                Open folio
                              </Link>
                            </div>
                          ))}
                        <ol>
                          {r.data.history?.map((h, i) => (
                            <li key={i}>
                              {new Date(h.at).toLocaleString("en-GB")} ·{" "}
                              {h.action} ·{" "}
                              {data.staff.find((st) => st.id === h.actor)
                                ?.name ?? h.actor}
                            </li>
                          ))}
                        </ol>
                      </details>
                      <div className={styles.actions}>
                        <Link
                          className="button secondary"
                          href={`${base}/pms/front-desk?property=${p.id}`}
                        >
                          Assign room / check in
                        </Link>
                        <Link
                          className="button secondary"
                          href={`${base}/pms/folios?property=${p.id}&folio=${stays.find((st) => st.data.reservationId === r.id)?.data.folioId ?? ""}`}
                        >
                          Folio / deposit
                        </Link>
                      </div>
                      {r.data.state === "confirmed" && (
                        <>
                          {form(
                            "Cancel reservation",
                            "reservation.cancel",
                            [
                              field("reason", "Reason"),
                              field("fee", "Fee", "money", 0),
                            ],
                            { id: r.id },
                          )}
                          {form(
                            "Record no-show",
                            "reservation.no_show",
                            [
                              field("reason", "Reason"),
                              field("fee", "Fee", "money", 0),
                            ],
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
                {[
                  "",
                  "arrivals",
                  "departures",
                  "reserved",
                  "in_house",
                  "checked_out",
                ].map((st) => (
                  <button
                    key={st}
                    className={`button ${query === st ? "" : "secondary"}`}
                    onClick={() => setQuery(st)}
                  >
                    {labelState[st] ?? "All"}
                  </button>
                ))}
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
                  <article key={r.id} className={styles.record}>
                    <div className={styles.line}>
                      <h3>{guestName(r.data.guestId)}</h3>
                      <Badge>{labelState[r.data.state]}</Badge>
                    </div>
                    <p>
                      {roomName(r.data.roomId)} · {r.data.arrival} →{" "}
                      {r.data.departure} · {r.data.adults} guests
                    </p>
                    <Link
                      href={`${base}/pms/folios?property=${p.id}&folio=${r.data.folioId}`}
                    >
                      Open this folio
                    </Link>
                    {["reserved", "in_house"].includes(r.data.state) && (
                      <>
                        {form(
                          r.data.state === "in_house"
                            ? "Move room"
                            : "Assign room",
                          r.data.state === "in_house"
                            ? "stay.move"
                            : "stay.assign",
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
                              [
                                field("reason", "Reason"),
                                field("fee", "Fee", "money", 0),
                              ],
                              { id: r.id },
                            )}
                            {form(
                              "No-show this stay",
                              "stay.no_show",
                              [
                                field("reason", "Reason"),
                                field("fee", "Fee", "money", 0),
                              ],
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
                                field(
                                  "lateFee",
                                  "Late check-out fee",
                                  "money",
                                  0,
                                ),
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
                            field(
                              "arrival",
                              "Arrival date",
                              "date",
                              r.data.arrival,
                            ),
                            field(
                              "departure",
                              "New departure date",
                              "date",
                              r.data.departure,
                            ),
                            select("rateId", "Rate plan", rates, r.data.rateId),
                            field("reason", "Reason"),
                          ],
                          { id: r.id },
                        )}
                      </>
                    )}
                  </article>
                ))}
              {!stays.length && (
                <Empty>No guests are currently in house.</Empty>
              )}
            </Panel>
          )}
          {["rooms", "housekeeping"].includes(section) && (
            <>
              <Panel
                title={
                  section === "rooms" ? "Rooms & readiness" : "Housekeeping"
                }
              >
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
                      <Badge
                        tone={
                          r.data.cleanliness === "inspected" ? "good" : "warn"
                        }
                      >
                        {labelState[r.data.cleanliness]}
                      </Badge>
                      <span>
                        {stays.some(
                          (st) =>
                            st.data.state === "in_house" &&
                            st.data.roomId === r.id,
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
                          field(
                            "to",
                            "Expected completion",
                            "date",
                            plusDays(today, 1),
                          ),
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
                  .filter((r) => !["done", "superseded"].includes(r.data.state))
                  .sort(
                    (a, b) =>
                      Number(b.data.priority === "urgent") -
                      Number(a.data.priority === "urgent"),
                  )
                  .map((r) => (
                    <article key={r.id} className={styles.record}>
                      <div className={styles.line}>
                        <h3>
                          Room {roomName(r.data.roomId)} ·{" "}
                          {labelState[r.data.type]}
                        </h3>
                        <Badge>{labelState[r.data.state]}</Badge>
                      </div>
                      <p>
                        {r.data.date} ·{" "}
                        {data.staff.find((x) => x.id === r.data.assigneeId)
                          ?.name ?? "Unassigned"}{" "}
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
                            checked: r.data.checklist.filter(
                              (_, i) => v[`check_${i}`],
                            ),
                          }),
                        )}
                      {r.data.state === "clean" &&
                        form("Inspect task", "task.inspect", [], { id: r.id })}
                    </article>
                  ))}
                {!rows("task").some(
                  (t) => !["done", "superseded"].includes(t.data.state),
                ) && <Empty>No open room tasks.</Empty>}
              </Panel>
              <Panel title="Rooms under maintenance">
                {rows("block")
                  .filter(
                    (r) =>
                      r.data.kind === "maintenance" &&
                      r.data.state === "active",
                  )
                  .map((r) => (
                    <div className={styles.record} key={r.id}>
                      <strong>
                        Room {roomName(r.data.roomId)} · {r.data.name}
                      </strong>
                      <p>
                        {r.data.from} → {r.data.to}
                      </p>
                      {form(
                        "Reopen room after inspection",
                        "maintenance.close",
                        [],
                        {
                          id: r.id,
                        },
                      )}
                    </div>
                  ))}
              </Panel>
            </>
          )}
          {section === "folios" &&
            (can("pms.cashier") || can("pms.reports")) && (
              <>
                <Panel title="Guest folio">
                  <label>
                    Select folio
                    <select
                      value={currentFolio?.id ?? ""}
                      onChange={(e) => setFolioId(e.target.value)}
                    >
                      {folios.map((f) => (
                        <option key={f.id} value={f.id}>
                          {f.name}
                        </option>
                      ))}
                    </select>
                  </label>
                  {currentFolio ? (
                    <>
                      <div className={styles.stats}>
                        {[
                          ["Balance due", currentFolio.data.due],
                          ["Advance", currentFolio.data.advance],
                          ["Deposit", currentFolio.data.deposit],
                        ].map(([label, n]) => (
                          <div className={styles.stat} key={String(label)}>
                            <span>{label}</span>
                            <strong>{money(Number(n))}</strong>
                          </div>
                        ))}
                      </div>
                      <Badge>{labelState[currentFolio.data.state]}</Badge>
                      <div className={styles.tableWrap}>
                        <table>
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
                                  <td>{money(e.data.total)}</td>
                                  <td>
                                    {["charge", "transfer"].includes(
                                      e.data.kind,
                                    ) &&
                                      e.data.total > 0 && (
                                        <>
                                          {form(
                                            "Reverse entry",
                                            "charge.reverse",
                                            [field("reason", "Reason")],
                                            { id: e.id },
                                          )}
                                          {form(
                                            "Transfer charge",
                                            "folio.transfer",
                                            [
                                              select(
                                                "toFolioId",
                                                "Destination folio",
                                                folios.filter(
                                                  (f) =>
                                                    f.id !== currentFolio.id,
                                                ),
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
                        </table>
                      </div>
                      <>
                        <div>
                          {form(
                            "Receive payment / advance",
                            "payment.receive",
                            [
                              field(
                                "amount",
                                "Amount",
                                "money",
                                currentFolio.data.due / 100,
                              ),
                              ...payments,
                            ],
                            { folioId: currentFolio.id },
                          )}
                          {form(
                            "Receive deposit",
                            "payment.deposit",
                            [
                              field("amount", "Amount", "money", 0),
                              ...payments,
                            ],
                            { folioId: currentFolio.id },
                          )}
                          {form(
                            "Add service charge",
                            "charge.add",
                            [
                              field("description", "Item"),
                              field("price", "Price", "money", 0),
                            ],
                            { folioId: currentFolio.id },
                          )}
                          {form(
                            "Apply deposit to balance due",
                            "payment.apply_deposit",
                            [
                              field("amount", "Amount", "money", 0),
                              field("reason", "Reason"),
                            ],
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
                              select(
                                "shiftId",
                                "Cashier shift",
                                openShifts,
                                openShifts[0]?.id,
                              ),
                            ],
                            { folioId: currentFolio.id },
                          )}
                          {form(
                            "Issue invoice / close folio",
                            "folio.close",
                            [],
                            {
                              id: currentFolio.id,
                            },
                          )}
                        </div>
                      </>
                    </>
                  ) : (
                    <Empty>
                      The folio will be created when the reservation is
                      confirmed.
                    </Empty>
                  )}
                </Panel>
                <Panel title="Refunds & credit">
                  {rows("refund")
                    .filter(
                      (r) =>
                        r.data.state !== "paid" && r.data.state !== "rejected",
                    )
                    .map((r) => (
                      <article className={styles.record} key={r.id}>
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
                      <div key={r.id} className={styles.record}>
                        <strong>
                          {
                            data.parties.find((p) => p.id === r.data.partyId)
                              ?.name
                          }{" "}
                          · {money(r.data.limit)}
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
                <Panel title="Documents">
                  {currentFolio?.data.invoiceId && <TaxDocumentReference base={base} invoiceId={currentFolio.data.invoiceId}/>}
                  {data.documents
                    .filter(
                      (d) => !currentFolio || d.folio_id === currentFolio.id,
                    )
                    .map((d) => (
                      <Link
                        className={styles.line}
                        key={d.id}
                        href={`${base}/pms/documents/${d.id}?property=${p.id}`}
                        target="_blank"
                      >
                        <span>
                          {d.kind} · {d.id.slice(0, 8)}
                        </span>
                        <span>
                          {new Date(d.created_at).toLocaleString("en-GB", {
                            timeZone: p.config.timezone,
                          })}
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
                <article className={styles.record} key={r.id}>
                  <div className={styles.line}>
                    <h3>{r.data.name}</h3>
                    <Badge>{labelState[r.data.state]}</Badge>
                  </div>
                  <p>
                    {r.data.date} ·{" "}
                    {data.staff.find((x) => x.id === r.data.userId)?.name ??
                      r.data.userId}
                  </p>
                  <p>
                    System cash {money(r.data.cash)} · Counted cash{" "}
                    {money(r.data.counted)} · Variance {money(r.data.variance)}
                  </p>
                  {r.data.state === "open" && (
                    <>
                      {form(
                        "Move cash / deposit to bank",
                        "cash.move",
                        [
                          {
                            ...field(
                              "amount",
                              "Amount (+cash in / −bank deposit)",
                              "money",
                              0,
                            ),
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
                          field(
                            "counted",
                            "Counted cash",
                            "money",
                            r.data.cash / 100,
                          ),
                          field(
                            "handover",
                            "Handover record",
                            "textarea",
                            "",
                            false,
                          ),
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
                {rows("item").map((r) => (
                  <article className={styles.record} key={r.id}>
                    <h3>{r.data.name}</h3>
                    {r.data.kind === "linen" && (
                      <p>
                        Clean {r.data.clean} · Issued {r.data.issued} · In
                        laundry {r.data.laundry} · Lost {r.data.lost}
                      </p>
                    )}
                    {(r.data.kind === "linen"
                      ? [
                          ["linen.receive", "Receive linen from warehouse"],
                          ["linen.issue", "Issue linen to room"],
                          [
                            "linen.return",
                            "Receive used linen / send to laundry",
                          ],
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
                            field(
                              "reason",
                              "Notes / reason",
                              "text",
                              "",
                              op === "linen.loss",
                            ),
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
                {rows("lost").map((r) => (
                  <div className={styles.record} key={r.id}>
                    <strong>{r.data.description}</strong>
                    <p>
                      Room {roomName(r.data.roomId)} · {r.data.location} ·{" "}
                      {labelState[r.data.state]}
                    </p>
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
                      )}
                  </div>
                ))}
              </Panel>
              <Panel title="Movement history">
                {rows("movement")
                  .slice(-30)
                  .reverse()
                  .map((r) => (
                    <div className={styles.line} key={r.id}>
                      <span>
                        {r.data.date} ·{" "}
                        {
                          rows("item").find((i) => i.id === r.data.itemId)?.data
                            .name
                        }
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
              <input
                aria-label="Search guests"
                placeholder="Search by name or phone"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
              {rows("guest")
                .filter((r) => `${r.data.name} ${r.data.phone}`.includes(query))
                .map((r) => (
                  <article className={styles.record} key={r.id}>
                    <h3>{r.data.name}</h3>
                    <p>
                      {r.data.phone} · {r.data.email} · {r.data.nationality}
                    </p>
                    <p>{r.data.notes}</p>
                    <p className={styles.muted}>
                      History{" "}
                      {stays.filter((s) => s.data.guestId === r.id).length}{" "}
                      stays
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
                            href={`${base}/pms/folios?property=${p.id}&folio=${st.data.folioId}`}
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
                        field(
                          "nationality",
                          "Nationality",
                          "text",
                          r.data.nationality,
                          false,
                        ),
                        field(
                          "documentRef",
                          "Document reference",
                          "text",
                          r.data.documentRef,
                          false,
                        ),
                        field(
                          "notes",
                          "Notes",
                          "textarea",
                          r.data.notes,
                          false,
                        ),
                      ],
                      { id: r.id },
                    )}
                  </article>
                ))}
            </Panel>
          )}
          {section === "night-audit" &&
            (can("pms.audit") || can("pms.reports")) && (
              <>
                <Panel title="Night Audit">
                  <div className={styles.auditHero}>
                    <MoonStar size={38} />
                    <div>
                      <h3>Close business day {today}</h3>
                      <p>
                        Review activity, post room charges and start the next
                        day.
                      </p>
                    </div>
                  </div>
                  {p.state === "open" && (
                    <>
                      {data.dayControls && <section aria-label="Restaurant day controls" className={styles.record}>
                        <h3>Restaurant day controls</h3>
                        <p>Business date {data.dayControls.businessDate} · {data.dayControls.openBills} open bills · {data.dayControls.pendingPayments} pending payments · {data.dayControls.openShifts} open cash sessions</p>
                        <p>Direct sales {money(data.dayControls.directSalesMinor)} · Room charges {money(data.dayControls.roomChargesMinor)} · Cash received {money(data.dayControls.cashReceivedMinor)} · Verified transfers {money(data.dayControls.verifiedTransferMinor)}</p>
                        <p>Restaurant stock movement {money(data.dayControls.stockValueMinor)} · Inventory ledger movement {money(data.dayControls.stockLedgerMinor)}</p>
                        {data.dayControls.blockers.length ? <><ul className={styles.blockers}>{data.dayControls.blockers.map(blocker=><li key={blocker}>{blocker}</li>)}</ul><Link href={`${base}/pos?tab=shifts`}>Review restaurant cash sessions</Link> · <Link href={`${base}/pos?tab=history`}>Review restaurant receipts</Link></> : <p className={styles.notice}>Restaurant controls are ready for day close.</p>}
                      </section>}
                      {nightBlockers(s!).length ? (
                        <ul className={styles.blockers}>
                          {nightBlockers(s!).map((b) => (
                            <li key={b}>{b}</li>
                          ))}
                        </ul>
                      ) : (
                        <p className={styles.notice}>Ready to close the day.</p>
                      )}
                      {!nightBlockers(s!).length && !data.dayControls?.blockers.length && form("Start day close", "night.start")}
                    </>
                  )}
                  {rows("night")
                    .slice()
                    .reverse()
                    .map((r) => (
                      <article className={styles.record} key={r.id}>
                        <strong>{r.data.date}</strong>{" "}
                        <Badge
                          tone={r.data.state === "completed" ? "good" : "warn"}
                        >
                          {labelState[r.data.state]}
                        </Badge>
                        <p>{r.data.message}</p>
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
              <Panel title="Daily performance">
                <label>
                  Date{" "}
                  <input
                    type="date"
                    value={from}
                    onChange={(e) => {
                      if (e.target.value) setFrom(e.target.value);
                    }}
                  />
                </label>
                {(() => {
                  const { sold, capacity, roomRevenue } = dailyRoomReport(
                    s!,
                    from,
                  );
                  const entries = rows("entry").filter(
                    (e) =>
                      e.data.date === from &&
                      ["charge", "adjustment"].includes(e.data.kind),
                  );
                  const net = entries.reduce(
                    (n, e) => n + e.data.net + e.data.service,
                    0,
                  );
                  return (
                    <div className={styles.stats}>
                      {[
                        [
                          "Occupancy",
                          `${capacity ? ((sold / capacity) * 100).toFixed(1) : "0"}%`,
                        ],
                        [
                          "ADR",
                          money(sold ? Math.round(roomRevenue / sold) : 0),
                        ],
                        [
                          "RevPAR",
                          money(
                            capacity ? Math.round(roomRevenue / capacity) : 0,
                          ),
                        ],
                        ["Net room revenue", money(net)],
                        [
                          "Tax",
                          money(entries.reduce((n, e) => n + e.data.tax, 0)),
                        ],
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
                <div className={styles.tableWrap}>
                  <table>
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
                        const n = rows("folio").reduce(
                            (n, f) => n + f.data[key],
                            0,
                          ),
                          gl =
                            data.ledger?.[key === "due" ? "receivable" : key] ??
                            0;
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
                  </table>
                </div>
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
                      href={`${base}/pms/folios?property=${p.id}&folio=${r.id}`}
                    >
                      <strong>{r.data.name}</strong>
                      <span>
                        {r.data.partyId
                          ? data.parties.find((p) => p.id === r.data.partyId)
                              ?.name
                          : "Guest"}
                      </span>
                      <span>
                        Due {money(r.data.due)} · Advance{" "}
                        {money(r.data.advance)} · Deposit{" "}
                        {money(r.data.deposit)}
                      </span>
                    </Link>
                  ))}
              </Panel>
            </>
          )}
          {section === "settings" && (
            <>
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
                          rows("reservation").find(
                            (r) => r.id === email.reservation_id,
                          )?.data.number
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
                {form("Save hotel", "property.save", [
                  field("name", "Hotel name", "text", p.name),
                  field("country", "Country code", "text", p.config.country),
                  field("timezone", "Time zone", "text", p.config.timezone),
                  field("checkIn", "Check-in time", "time", p.config.checkIn),
                  field(
                    "checkOut",
                    "Check-out time",
                    "time",
                    p.config.checkOut,
                  ),
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
                  field(
                    "businessDate",
                    "Starting business date",
                    "date",
                    today,
                  ),
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
                    <div className={styles.record} key={r.id}>
                      <strong>{r.data.name}</strong> · {r.data.capacity} guests
                      {form(
                        "Edit room type",
                        "room_type.save",
                        [
                          field("name", "Type name", "text", r.data.name),
                          field(
                            "capacity",
                            "Maximum guests",
                            "number",
                            r.data.capacity,
                          ),
                          field(
                            "amenities",
                            "Amenities",
                            "textarea",
                            r.data.amenities,
                            false,
                          ),
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
                    <div className={styles.record} key={r.id}>
                      <strong>{r.data.number}</strong> · {r.data.building}{" "}
                      {r.data.floor}
                      {form(
                        "Edit room",
                        "room.save",
                        [
                          field("number", "Room number", "text", r.data.number),
                          select("typeId", "Type", roomTypes, r.data.typeId),
                          field(
                            "building",
                            "Building",
                            "text",
                            r.data.building,
                            false,
                          ),
                          field("floor", "Floor", "text", r.data.floor, false),
                          field(
                            "active",
                            "Room active",
                            "checkbox",
                            r.data.active,
                          ),
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
                  <div className={styles.record} key={r.id}>
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
                        field(
                          "price",
                          "Price per night",
                          "money",
                          r.data.price / 100,
                        ),
                        field("stopSell", "Stop sell", "checkbox", false),
                        field(
                          "closedArrival",
                          "Close to arrival",
                          "checkbox",
                          false,
                        ),
                        field(
                          "closedDeparture",
                          "Close to departure",
                          "checkbox",
                          false,
                        ),
                      ],
                      { id: r.id },
                    )}
                  </div>
                ))}
              </Panel>
              <Panel title="Accounting & tax">
                <p>
                  Pilot currency THB · The preparer and approver must be
                  different people.
                </p>
                <Badge tone={p.config.approvedBy ? "good" : "warn"}>
                  {p.config.approvedBy ? "Approved" : "Awaiting approval"} ·
                  Revision {p.config.revision}
                </Badge>
                {form(
                  "Save accounting and tax setup",
                  "accounting.save",
                  [
                    select(
                      "chartId",
                      "Chart of accounts",
                      data.charts,
                      p.config.chartId,
                    ),
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
                    accounts: Object.fromEntries(
                      pmsAccountKeys.map((k) => [k, v[k]]),
                    ),
                  }),
                )}
                {form("Approve accounting and tax setup", "accounting.approve")}
                {form("Verify readiness and open hotel", "property.enable")}
              </Panel>
            </>
          )}
        </>
      )}
    </WidgetGrid></main>
  );
}
