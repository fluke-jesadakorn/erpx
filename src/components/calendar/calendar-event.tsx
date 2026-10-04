"use client";
import { Button } from "@/components/ui/foundation";

import type { CSSProperties, PointerEvent as ReactPointerEvent, PointerEventHandler } from "react";
import { AlertCircle } from "lucide-react";
import type { CalendarItem, CalendarLayer } from "@/lib/calendar";
import styles from "./calendar.module.css";
export const layerNames: Record<CalendarLayer, string> = {
  support: "Customer support",
  crm: "CRM pipeline",
  customer_followups: "Customer follow-ups",
  stock_transit: "Stock in transit",
  supplier_promises: "Supplier deliveries",
  procurement_rfqs: "Supplier quotations",
  supplier_bills: "Supplier invoice reviews",
  quality_holds: "Quality holds",
  supplier_batches: "Supplier payments",
  refunds: "Refund instructions",
  maintenance: "Maintenance and preventive due work",
  legal: "Legal deadlines",
  roster: "Shifts",
  leave: "Leave",
  appointments: "Appointments",
  announcements: "Announcements",
  lifecycle: "People milestones",
};
const palette: Record<CalendarLayer, [string, string, string]> = {
  support: ["#9a6c3d", "#fbf1e5", "#734a24"],
  crm: ["#6475a0", "#edf0fa", "#324579"],
  customer_followups: ["#6475a0", "#edf0fa", "#324579"],
  stock_transit: ["#488775", "#ebf4ef", "#295d49"],
  supplier_bills: ["#6475a0", "#edf0fa", "#324579"],
  procurement_rfqs: ["#6475a0", "#edf0fa", "#324579"],
  supplier_promises: ["#488775", "#ebf4ef", "#295d49"],
  quality_holds: ["#986327", "#fff3df", "#764a1a"],
  supplier_batches: ["#488775", "#ebf4ef", "#295d49"],
  refunds: ["#488775", "#ebf4ef", "#295d49"],
  maintenance: ["#488775", "#ebf4ef", "#295d49"],
  legal: ["#6981c1", "#edf1fc", "#495f96"],
  roster: ["#488775", "#ebf4ef", "#295d49"],
  leave: ["#cf855b", "#fcf0e7", "#915832"],
  appointments: ["#6981c1", "#edf1fc", "#495f96"],
  announcements: ["#b69640", "#fbf5df", "#806c27"],
  lifecycle: ["#aa78a9", "#f6edf6", "#835280"],
};
export function eventStyle(layer: CalendarLayer): CSSProperties {
  const [color, bg, ink] = palette[layer];
  return {
    "--event-color": color,
    "--event-bg": bg,
    "--event-ink": ink,
  } as CSSProperties;
}
export function eventTime(item: CalendarItem, zone: string) {
  if (item.allDay) return item.startDate ? "All day" : "Unscheduled";
  if (item.startsAt === null) return "Unscheduled";
  const fmt = (ms: number) =>
    new Intl.DateTimeFormat("en", {
      timeZone: zone,
      hour: "2-digit",
      minute: "2-digit",
      hourCycle: "h23",
    }).format(new Date(ms));
  if (!item.endsAt || item.endsAt - item.startsAt <= 1) return fmt(item.startsAt);
  const dateInZone = (ms: number, timeZone: string) =>
    new Intl.DateTimeFormat("en-CA", {
      timeZone,
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
    }).format(new Date(ms));
  const days = Math.round(
    (Date.parse(dateInZone(item.endsAt, zone)) - Date.parse(dateInZone(item.startsAt, zone))) /
      86400000,
  );
  return `${fmt(item.startsAt)}–${fmt(item.endsAt)}${days > 0 ? ` (+${days}d)` : ""}`;
}
export function CalendarEvent({
  item,
  zone,
  onOpen,
  showPeople = false,
  onPointerDown,
  resizeHandles = false,
  onResizePointerDown,
  dragging = false,
  compact = false,
}: {
  item: CalendarItem;
  zone: string;
  onOpen: (item: CalendarItem) => void;
  showPeople?: boolean;
  onPointerDown?: PointerEventHandler<HTMLButtonElement>;
  resizeHandles?: boolean;
  onResizePointerDown?: (edge: "start" | "end", event: ReactPointerEvent<HTMLSpanElement>) => void;
  dragging?: boolean;
  compact?: boolean;
}) {
  return (
    <Button
      type="submit"
      variant="plain"
      className={`${styles.event} ${dragging ? styles.dateDragging : ""}`}
      style={eventStyle(item.source)}
      data-calendar-event
      data-warning={item.warnings.length > 0}
      data-status={item.status}
      onPointerDown={onPointerDown}
      onClick={() => onOpen(item)}
      aria-label={`${item.title}, ${eventTime(item, zone)}, ${item.status}`}
    >
      <small>
        {eventTime(item, zone)}
        {item.status === "draft" && " · Draft"}
      </small>
      <strong>{item.title}</strong>
      {showPeople && !!item.participants.length && (
        <small>{item.participants.map((p) => p.name).join(", ")}</small>
      )}
      {item.capacity !== undefined && (
        <small>
          {item.assigned}/{item.capacity}
          {!compact && " assigned"}
        </small>
      )}
      {item.warnings.length > 0 && (
        <small title="Needs attention">
          <AlertCircle size={11} /> {!compact && "Needs attention"}
        </small>
      )}
      {resizeHandles && (
        <>
          <span
            className={styles.dateResizeHandle}
            data-edge="start"
            aria-label="Resize start date"
            onPointerDown={(event) => onResizePointerDown?.("start", event)}
          />
          <span
            className={styles.dateResizeHandle}
            data-edge="end"
            aria-label="Resize end date"
            onPointerDown={(event) => onResizePointerDown?.("end", event)}
          />
        </>
      )}
    </Button>
  );
}
