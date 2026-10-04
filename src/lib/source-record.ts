/** A reference carries identity and read version, never command authority. */
export type SourceRecordReference = {
  kind: string;
  id: string;
  version?: number | string;
  href: string;
  label: string;
};
export function sourceRecordLabel(kind: string) {
  const labels: Record<string, string> = {
    staff_task: "Task", staff_project: "Project", purchase_request: "Purchase request",
    service_request: "Service request", equipment_request: "Equipment request", expense: "Expense",
    leave: "Leave", quality_hold: "Quality hold", quality_alert: "Quality follow-up",
    reservation: "Reservation", stay: "Stay", folio: "Folio", refund: "Refund", correction: "Correction",
    shift: "Shift", night: "Night audit", room: "Room", task: "Housekeeping task", block: "Room block",
    transfer: "Transfer", pms_record: "Hotel work", order: "Order", invoice: "Invoice", journal: "Journal",
    calendar_appointment: "Appointment",
    maintenance_plan: "Preventive maintenance", maintenance_work: "Maintenance work",
  };
  return labels[kind] ?? kind.replaceAll("_", " ").replace(/^./, letter => letter.toUpperCase());
}
