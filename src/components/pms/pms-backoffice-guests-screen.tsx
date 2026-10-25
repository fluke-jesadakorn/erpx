"use client";
import type { Option, Field } from "./workspace-types";
import { RecordSurface } from "@/components/records/record-surface";
import { TextInput } from "@/components/ui/foundation";
import { FeatureWidget } from "@/components/features/feature-page";
import Link from "next/link";
import { type ReactNode } from "react";
import styles from "./workspace.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@/components/front/front-language";
import { frontText } from "@/lib/front-language";
import type {BackofficePmsWorkspaceContext} from "./pms-backoffice-workspace";


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
export function BackofficePmsWorkspaceGuestsScreen({context}: {context: BackofficePmsWorkspaceContext & {p: NonNullable<BackofficePmsWorkspaceContext["p"]>}}) {
  const {form, query, setQuery, rows, tr, can, stays, roomName, dailyPmsHref, p} = context;
  return <FrontLocalizedTree>{(
                <Panel title="Guest directory">
                  {form("Add guest", "guest.save", [
                    field("name", "Full name"),
                    field("email", "Email", "email", "", false),
                    field("phone", "Phone", "text", "", false),
                    field("nationality", "Nationality", "text", "", false),
                    field("documentRef", "Document reference", "text", "", false),
                    field("preferredLanguage", "Preferred language", "text", "", false),
                    field("roomPreference", "Room preference", "text", "", false),
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
                      <RecordSurface as="article" recordId={r.id}
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
                        {(r.data.preferredLanguage || r.data.roomPreference) && <p>{[r.data.preferredLanguage,r.data.roomPreference].filter(Boolean).join(' · ')}</p>}
                        {can("pms.frontdesk") && <><h4>Service requests</h4>{(r.data.serviceRequests??[]).map(request=><div className={styles.line} key={request.id}><span>{request.text} · {request.state}</span>{request.state==='open'&&form("Resolve request","guest.service_request",[select("state","Outcome",[{id:'completed',name:'Completed'},{id:'cancelled',name:'Cancelled'}],'completed')],{guestId:r.id,requestId:request.id,text:request.text})}</div>)}{form("Add service request","guest.service_request",[field("text","Request","textarea")],{guestId:r.id})}</>}

                        <p className={styles.muted}>
                          Recent stays (up to 20): {stays.filter((s) => s.data.guestId === r.id).length}
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
                                href={dailyPmsHref("guest-accounts", {
                                  property: p.id,
                                  guestAccount: st.data.guestAccountId,
                                })}
                              >
                                Stay guest account
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
                            field("preferredLanguage", "Preferred language", "text", r.data.preferredLanguage??"", false),
                            field("roomPreference", "Room preference", "text", r.data.roomPreference??"", false),
                            field("notes", "Notes", "textarea", r.data.notes, false),
                          ],
                          { id: r.id },
                        )}
                      </RecordSurface>
                    ))}
                </Panel>
              )}</FrontLocalizedTree>;
}
