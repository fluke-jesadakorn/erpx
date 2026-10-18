"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import type { Option, Field } from "./workspace-types";
import { RecordSurface } from "@/components/records/record-surface";
import { FeatureWidget } from "@/components/shared/features/feature-page";
import { AttachmentFeatureSurface } from "@/components/files/attachment-feature-context";
import { type RecordDocumentPurpose } from "@/components/files/record-document-panel";
import Link from "@/components/workplace/workspace-link";
import { useState, type ReactNode } from "react";
import dynamic from "next/dynamic";
import styles from "./workspace.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@/components/front/front-language";
import { frontText } from "@/lib/shared/front-language";
import type {BackofficePmsWorkspaceInventoryScreenContext as BackofficePmsWorkspaceContext} from "./backoffice-screen-context";


const RecordDocumentPanel = dynamic(() => import("@/components/files/record-document-panel").then(module => module.RecordDocumentPanel));





const pmsFileBase=(base:string)=>base.startsWith('/front/')?`${base.replace(/\/$/,'')}/pms`:base;


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
export function BackofficePmsWorkspaceInventoryScreen({context}: {context: BackofficePmsWorkspaceContext}) {
  const {crossLinks, erpBase, form, data, rows, s, selectedRecord, tr, rooms, guestAccounts, guests, today, roomName, base, can} = context;
  return <FrontLocalizedTree>{(
                <>
                  <Panel title="Minibar, supplies & linen">
                    {crossLinks.inventory && (
                      <p className={styles.muted}>
                        Shared inventory:{" "}
                        <Link href={canonicalUiHref(`${erpBase}/commerce?view=stock`)}>Stock & movements</Link> ·{" "}
                        <Link href={canonicalUiHref(`${erpBase}/commerce?view=products`)}>Products</Link> ·{" "}
                        <Link href={canonicalUiHref(`${erpBase}/commerce?view=warehouses`)}>Warehouses</Link>
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
                    {!rows("item").some(r=>!s?.listMeta||s.listMeta.ids.includes(r.id)||r.id===selectedRecord) && (
                      <p className={styles.empty}>{tr("No items available in this view.")}</p>
                    )}
                    {rows("item").filter(r=>!s?.listMeta||s.listMeta.ids.includes(r.id)||r.id===selectedRecord).map((r) => (
                      <RecordSurface as="article" recordId={r.id}
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
                            ? [["minibar.charge", "Post minibar charge to guest account"]]
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
                                  ? [select("guestAccountId", "Guest Account", guestAccounts)]
                                  : []),
                                field("reason", "Notes / reason", "text", "", op === "linen.loss"),
                              ],
                              { itemId: r.id },
                            )}
                          </div>
                        ))}
                      </RecordSurface>
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
                      <RecordSurface recordId={r.id}
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
                      </RecordSurface>
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
              )}</FrontLocalizedTree>;
}
