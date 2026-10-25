"use client";
import { PmsChoiceSelect } from "./choice-select";
import type { Option, Field, Send, ActionAttachment } from "./workspace-types";
import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field as UiField, Select, TextArea, TextInput } from "@/components/ui/foundation";
import { decimalToMinor } from "@/lib/finance-money";
import { operationPermission, records, type DataMap, type Kind, type PmsRecord } from "@/server/modules/pms/public/rules/pms-model";
import { useCurrency, useMoneyFormatter } from "@/components/money/currency-provider";
import { workFetch } from "@/lib/work-fetch";
import { createLatestRead } from "@/lib/latest-read";
import type { PmsWorkspace } from "@/server/modules/pms/public/pms";
import { Hotel, Plus, RefreshCw } from "lucide-react";
import { useRouter, useSearchParams } from "next/navigation";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { PmsListPagination } from "./list-pagination";
import dynamic from "next/dynamic";
import styles from "./workspace.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@/components/front/front-language";
import { frontText } from "@/lib/front-language";


const PmsEntryDetail = dynamic(() => import("@/components/records/pms-entry-detail").then(module => module.PmsEntryDetail));


const SourceSupportChooser = dynamic(() => import("@/components/modules/source-support-chooser").then(module => module.SourceSupportChooser));


const AttachmentPanel = dynamic(() => import("@/components/attachments/attachment-panel").then(module => module.AttachmentPanel));





const pmsFileBase=(base:string)=>base.startsWith('/front/')?`${base.replace(/\/$/,'')}/pms`:base;


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
  choiceApi,
  propertyId,
}: {
  title: string;
  choiceApi?:string;
  propertyId?:string;
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
            {f.type === "select" && f.name === "guestId" && choiceApi && propertyId ? (
              <PmsChoiceSelect key={`${choiceApi}:${propertyId}:${f.name}`} api={choiceApi} propertyId={propertyId} kind="guest" label={f.label} name={f.name} options={f.options} defaultValue={String(f.value??"")} required={f.required} autoLoad={false} emptyLabel={`Select ${f.label}`} />
            ) : f.type === "select" ? (
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
          <div
            key={`${attachment.purposeKey}:${attachment.targetId}`}
            className={styles.wide}
          >
            <AttachmentPanel
              {...attachment}
              componentKind="action"
              componentRef={op}
              fieldName="attachmentSetId"
              bindMode="action"
              visibility="Visible only to hotel staff authorized for this property and action."
            />
          </div>
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
const FrontPmsDailyDashboardScreen = dynamic(() => import("./pms-front-dashboard-screen").then(module => module.FrontPmsDailyDashboardScreen));
const FrontPmsDailyReservationsScreen = dynamic(() => import("./pms-front-reservations-screen").then(module => module.FrontPmsDailyReservationsScreen));
const FrontPmsDailyFrontDeskScreen = dynamic(() => import("./pms-front-front-desk-screen").then(module => module.FrontPmsDailyFrontDeskScreen));
const FrontPmsDailyRoomsScreen = dynamic(() => import("./pms-front-rooms-screen").then(module => module.FrontPmsDailyRoomsScreen));
const FrontPmsDailyHousekeepingScreen = dynamic(() => import("./pms-front-housekeeping-screen").then(module => module.FrontPmsDailyHousekeepingScreen));
const FrontPmsDailyGuestAccountsScreen = dynamic(() => import("./pms-front-guest-accounts-screen").then(module => module.FrontPmsDailyGuestAccountsScreen));
const FrontPmsDailyCashierScreen = dynamic(() => import("./pms-front-cashier-screen").then(module => module.FrontPmsDailyCashierScreen));
const FrontPmsDailyNightAuditScreen = dynamic(() => import("./pms-front-night-audit-screen").then(module => module.FrontPmsDailyNightAuditScreen));
function useFrontPmsDailyState({
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
  setGuestAccountId,
  currentGuestAccount,
  rows,
  roomName,
  guestName,
  rooms,
  guests,
  rates,
  guestAccounts,
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
  setGuestAccountId: (value: string) => void;
  currentGuestAccount?: PmsRecord<"guest_account">;
  rows: FrontPmsRows;
  roomName: (id: string) => string;
  guestName: (id: string) => string;
  rooms: Option[];
  guests: Option[];
  rates: Option[];
  guestAccounts: Option[];
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
  const listRouter=useRouter(),listParams=useSearchParams();
  const language = useFrontLanguage();
  const formatSelectedMoney = useMoneyFormatter();
  const money = (value: number) =>
    formatSelectedMoney(value, property?.config.currency ?? "THB", today, 2);
  const tr = (value: string) => frontText(value, language);
  const [staySearch,setStaySearch]=useState("");
  const [floor,setFloor]=useState(""),[assignee,setAssignee]=useState(""),[taskState,setTaskState]=useState("");
  const roomRows = rows("room");
  const taskRows = [...rows("task")].sort(
    (a, b) =>
      Number(b.data.priority === "urgent") - Number(a.data.priority === "urgent") ||
      a.data.date.localeCompare(b.data.date) ||
      roomName(a.data.roomId).localeCompare(roomName(b.data.roomId), undefined, { numeric: true }),
  );
  const visibleStays = stays.filter(row=>!snapshot?.listMeta||snapshot.listMeta.section!=="front-desk"||snapshot.listMeta.ids.includes(row.id)||row.id===selectedRecord).filter(row=>`${guestName(row.data.guestId)} ${rows("reservation").find(r=>r.id===row.data.reservationId)?.data.number??""}`.toLowerCase().includes(staySearch.toLowerCase())).filter((row) =>
    query === "arrivals"
      ? row.data.state === "reserved" && row.data.arrival === today
      : query === "departures"
        ? row.data.state === "in_house" && row.data.departure <= today
        : !query || row.data.state === query,
  );
  const reservationRows = rows("reservation").filter(row=>!snapshot?.listMeta||snapshot.listMeta.ids.includes(row.id)||row.id===selectedRecord).filter((row) =>
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
  return {data, base, erpBase, frontHref, section, snapshot, property, propertyId, selectedRecord, today, from, setFrom, query, setQuery, setGuestAccountId, currentGuestAccount, rows, roomName, guestName, rooms, guests, rates, guestAccounts, stays, openShifts, payments, can, form, send, error, notice, onRefresh, onPropertyChange, listRouter, listParams, language, formatSelectedMoney, money, tr, staySearch, setStaySearch, floor, setFloor, assignee, setAssignee, taskState, setTaskState, roomRows, taskRows, visibleStays, reservationRows, operationalNightBlockers, canRecoverBlockedNight, canOperate, showReservationForms, showFrontDeskForms, showCashierForms, showHousekeepingForms, showInspectionForms, showMaintenanceForms};
}
export type FrontPmsDailyContext = ReturnType<typeof useFrontPmsDailyState>;
function FrontPmsDaily(props: {
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
  setGuestAccountId: (value: string) => void;
  currentGuestAccount?: PmsRecord<"guest_account">;
  rows: FrontPmsRows;
  roomName: (id: string) => string;
  guestName: (id: string) => string;
  rooms: Option[];
  guests: Option[];
  rates: Option[];
  guestAccounts: Option[];
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
  const context = useFrontPmsDailyState(props);
  const {data, erpBase, section, snapshot, property, propertyId, selectedRecord, today, rows, can, error, notice, onRefresh, onPropertyChange, tr, canOperate, showCashierForms} = context;
  
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
        {snapshot?.listMeta&&<PmsListPagination key={JSON.stringify(snapshot.listMeta)} meta={snapshot.listMeta} roomTypes={rows("room_type").map(row=>({id:row.id,name:row.data.name}))}/>}
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
            {section === "dashboard" && <FrontPmsDailyDashboardScreen context={context}/>}

            {section === "reservations" && <FrontPmsDailyReservationsScreen context={context}/>}

            {section === "front-desk" && <FrontPmsDailyFrontDeskScreen context={context}/>}

            {section === "rooms" && <FrontPmsDailyRoomsScreen context={context}/>}

            {section === "housekeeping" && <FrontPmsDailyHousekeepingScreen context={context}/>}

            {section === "guest-accounts" && showCashierForms && <FrontPmsDailyGuestAccountsScreen context={context}/>}

            {section === "cashier" && showCashierForms && <FrontPmsDailyCashierScreen context={context}/>}

            {section === "night-audit" && can("pms.audit") && <FrontPmsDailyNightAuditScreen context={context}/>}
          </>
        )}
      </div>
    </FrontLocalizedTree>
  );
}
export function FrontPmsWorkspace({
  initial,
  base,
  erpBase = base,
  section,
  selectedGuestAccount,
  frontMode = false,
  frontHref,
  initialQueue,
  selectedRecord,
}: {
  initial: PmsWorkspace;
  base: string;
  erpBase?: string;
  section: string;
  selectedGuestAccount?: string;
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
  const router = useRouter(),recordSelection=useSearchParams();
  const [data, setData] = useState(initial),
    [error, setError] = useState(""),
    [query, setQuery] = useState((['guests','reservations'].includes(section)?initial.snapshot?.listMeta?.q:initial.snapshot?.listMeta?.status)??initialQueue ?? ""),
    [guestAccountId, setGuestAccountId] = useState(
      () =>
        selectedGuestAccount ??
        (initial.snapshot
          ? (records(initial.snapshot, "guest_account").find((row) => row.data.state !== "closed")?.id ??
            records(initial.snapshot, "guest_account")[0]?.id)
          : "") ??
        "",
    ),
    [from, setFrom] = useState(initial.snapshot?.property.businessDate ?? initial.defaultDate);
  const [notice, setNotice] = useState("");
  const attempts = useRef(new Map<string, { body: string; key: string }>());
  const propertyId = data.snapshot?.property.id;
  const [previousInitial, setPreviousInitial] = useState(initial);
  if (previousInitial !== initial) {
    setPreviousInitial(initial);
    setData(initial);
    if(initial.snapshot?.listMeta)setQuery(['guests','reservations'].includes(section)?initial.snapshot.listMeta.q:initial.snapshot.listMeta.status);
  }
  const listMeta=data.snapshot?.listMeta;
  const selectedListId=selectedRecord??(section==='guest-accounts'?guestAccountId:undefined)??listMeta?.selectedId;
  const listReadParams=listMeta?{view:'list',section:listMeta.section,from:listMeta.from,to:listMeta.to,page:String(listMeta.page),q:listMeta.q,status:listMeta.status,building:listMeta.building,floor:listMeta.floor,typeId:listMeta.typeId,...(selectedListId?{selectedId:selectedListId}:{})}:section==='settings'?{view:'setup'}:section==='reports'?{view:'report'}:{};
  const listReadKey=JSON.stringify(listReadParams);
  const [reads] = useState(()=>createLatestRead<PmsWorkspace>());
  const pendingWrites = useRef(0), readEpoch = useRef(0);
  const invalidateReads=useCallback(()=>{readEpoch.current++;reads.cancel();},[reads]);
  useEffect(() => {
    let active = true;
    async function refresh() {
      if (document.visibilityState !== "visible" || pendingWrites.current) return;
      try {
        const endpoint=`/api/v1${base}/pms?${new URLSearchParams({ ...(propertyId ? { property: propertyId } : {}), ...(section === "night-audit" ? { dayControls: "1" } : {}), ...(section === "dashboard" ? { view: "dashboard" } : {}),...JSON.parse(listReadKey) })}`;
        await reads.read(endpoint,async signal=>{
          const response=await workFetch(endpoint,{signal,cache:"no-store"});
          if(!response.ok){
            if(active && [401,403,409].includes(response.status))setError(tr("Your access or role changed. Reload the workspace."));
            throw new Error("PMS_READ_FAILED");
          }
          return (await response.json()).data as PmsWorkspace;
        },updated=>{if(active)setData(updated);});
      } catch {
        if (active) setNotice(tr("Connection lost · Showing the latest loaded data"));
      }
    }
    const timer = [
      "dashboard",
      "reservations",
      "front-desk",
      "rooms",
      "housekeeping",
      "guest-accounts",
      "cashier",
      "inventory",
      "night-audit",
    ].includes(section)
      ? setInterval(refresh, 8000)
      : undefined;
    if(timer)document.addEventListener("visibilitychange",refresh);
    return () => {
      active = false;
      invalidateReads();
      clearInterval(timer);
      document.removeEventListener("visibilitychange",refresh);
    };
  }, [base, language, propertyId, section, tr,listReadKey,reads,initial,invalidateReads]);
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
    const epoch=readEpoch.current;
    pendingWrites.current++;
    reads.cancel();
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
      if(epoch!==readEpoch.current)return true;
      const pid = json.data.propertyId;
      let refreshed = false;
      try {
        const endpoint=`/api/v1${base}/pms?${new URLSearchParams({ property: pid, ...(section === "night-audit" ? {dayControls:"1"} : {}), ...(section === "dashboard" ? { view: "dashboard" } : {}),...JSON.parse(listReadKey) })}`;
        await reads.read(endpoint,async signal=>{
          const fresh=await workFetch(endpoint,{signal,cache:"no-store"});
          if(!fresh.ok)throw new Error("PMS_READ_FAILED");
          return (await fresh.json()).data as PmsWorkspace;
        },updated=>{if(epoch===readEpoch.current){setData(updated);refreshed=true;}});
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
      if(epoch===readEpoch.current)setError(e instanceof Error ? e.message : tr("Could not connect. Try again."));
      return false;
    } finally {
      pendingWrites.current--;
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
      choiceApi={`/api/v1${base}/pms`}
      propertyId={propertyId}
      hidden={hidden}
      send={send}
      permissions={permissions}
      version={version}
      transform={transform}
      attachments={attachments?.map(item=>({...item,base:pmsFileBase(item.base)}))}
      open={recordSelection.get("detailKind")!=="pms_record" && !!selectedRecord && (hidden.id === selectedRecord || hidden.roomId === selectedRecord)}
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
  const rooms = options("room", (d) => `${d.number} · ${labelState[d.cleanliness]}`),
    guests = options("guest", (d) => d.name),
    rates = options("rate", (d) => `${d.name} · ${money(d.price)}`),
    guestAccounts = options(
      "guest_account",
      (d) => `${guestName(d.guestId)} · ${d.name} · ${tr("Balance due")} ${money(d.due)}`,
    ),
    stays = rows("stay"),
    today = p?.businessDate ?? from;
  const currentGuestAccount =
    rows("guest_account").find((r) => r.id === (guestAccountId || selectedGuestAccount)) ??
    rows("guest_account").find((r) => r.data.state !== "closed") ??
    rows("guest_account")[0];
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
  const entryDrawer=<PmsEntryDetail base={base} propertyId={propertyId} renderAction={(id,snapshot)=>{
    const entry=snapshot.snapshot?.records.find(row=>row.kind==='entry'&&row.id===id) as PmsRecord<'entry'>|undefined;
    if(!entry)return null;
    const pending=snapshot.chargeCorrections.some(row=>row.entry_id===id&&row.status==='pending');
    const reversed=snapshot.snapshot?.records.some(row=>row.kind==='entry'&&(row as PmsRecord<'entry'>).data.originalId===id);
    return pending?<p>Awaiting independent review</p>:entry.data.method!=='opening'&&['charge','transfer'].includes(entry.data.kind)&&entry.data.total>0&&!entry.data.sourceKey?.startsWith('pos')&&!reversed?form('Request reversal','charge.correction.request',[field('reason','Reason')],{id}):null;
  }}/>;
    return (
      <>{entryDrawer}<FrontPmsDaily
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
        setGuestAccountId={setGuestAccountId}
        currentGuestAccount={currentGuestAccount}
        rows={rows}
        roomName={roomName}
        guestName={guestName}
        rooms={rooms}
        guests={guests}
        rates={rates}
        guestAccounts={guestAccounts}
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
      /></>
    );
}
