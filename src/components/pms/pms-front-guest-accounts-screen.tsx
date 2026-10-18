"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import type { Option, Field } from "./workspace-types";
import { CollectionRecordLink } from "@/components/records/record-collection";
import { RecordSurface } from "@/components/records/record-surface";
import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { Field as UiField, Select, Table, TableFrame } from "@/components/ui/foundation";
import { FeatureWidget } from "@/components/shared/features/feature-page";
import { TimeText } from "@/components/people_payroll/time/time-preference-context";
import { Hotel } from "lucide-react";
import { type ReactNode } from "react";
import styles from "./workspace.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@/components/front/front-language";
import { frontText } from "@/lib/shared/front-language";
import type {FrontPmsDailyContext} from "./standard-pms-front-workspace";


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
export function FrontPmsDailyGuestAccountsScreen({context}: {context: FrontPmsDailyContext}) {
  const {tr, currentGuestAccount, setGuestAccountId, guestAccounts, guestName, money, rows, data, form, propertyId, erpBase, payments, openShifts} = context;
  return <FrontLocalizedTree>{(
              <Panel title="Guest Account">
                <UiField className={styles.guestAccountPicker}>
                  {tr("Select guest account")}{" "}
                  <Select
                    value={currentGuestAccount?.id ?? ""}
                    onChange={(event) => setGuestAccountId(event.target.value)}
                  >
                    {guestAccounts.map((item) => (
                      <option key={item.id} value={item.id}>
                        {item.name}
                      </option>
                    ))}
                  </Select>
                </UiField>
                {currentGuestAccount ? (
                  <>
                    <h3>{guestName(currentGuestAccount.data.guestId)}</h3>
                    <div className={styles.stats}>
                      {[
                        ["Balance due", currentGuestAccount.data.due],
                        ["Advance", currentGuestAccount.data.advance],
                        ["Deposit", currentGuestAccount.data.deposit],
                      ].map(([label, value]) => (
                        <div className={styles.stat} key={String(label)}>
                          <span>{tr(String(label))}</span>
                          <strong>
                            <StatAmount formatted={money(Number(value))} />
                          </strong>
                        </div>
                      ))}
                    </div>
                    <Badge>{labelState[currentGuestAccount.data.state]}</Badge>
                    <TableFrame
                      label="Records"
                      className={`${styles.tableWrap} ${styles.guestAccountEntries}`}
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
                            .filter((entry) => entry.data.guestAccountId === currentGuestAccount.id)
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
                                    <CollectionRecordLink kind="pms_entry" id={entry.id}>{entry.data.description}</CollectionRecordLink>
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
                        href={canonicalUiHref(`${erpBase}/pms/guest-accounts?property=${encodeURIComponent(propertyId)}&queue=corrections`)}
                      >
                        {tr("Manage guest account corrections in ERPX")}
                      </UiActionLink>
                    )}
                    {currentGuestAccount.data.state !== "closed" &&
                      form(
                        "Receive payment / advance",
                        "payment.receive",
                        [
                          field("amount", "Amount", "money", currentGuestAccount.data.due / 100),
                          ...payments,
                        ],
                        { guestAccountId: currentGuestAccount.id },
                      )}
                    {currentGuestAccount.data.state !== "closed" &&
                      form(
                        "Receive deposit",
                        "payment.deposit",
                        [field("amount", "Amount", "money", 0), ...payments],
                        { guestAccountId: currentGuestAccount.id },
                      )}
                    {currentGuestAccount.data.state !== "closed" &&
                      openShifts.length > 0 &&
                      form(
                        "Record bank transfer pending verification",
                        "payment.request_transfer",
                        [
                          field("amount", "Transfer amount", "money", currentGuestAccount.data.due / 100),
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
                        { guestAccountId: currentGuestAccount.id },
                      )}
                    {data.transferRequests
                      .filter(
                        (item) =>
                          item.guest_account_id === currentGuestAccount.id && item.recorded_by === data.userId,
                      )
                      .map((item) => (
                        <RecordSurface as="article" recordId={item.id}
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
                        </RecordSurface>
                      ))}
                    {data.chargeCorrections
                      .filter((item) => item.guest_account_id === currentGuestAccount.id)
                      .map((item) => (
                        <RecordSurface as="article" recordId={item.id}
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
                        </RecordSurface>
                      ))}
                    {data.documents
                      .filter((document) => document.guest_account_id === currentGuestAccount.id)
                      .map((document) => (
                        <a
                          className={styles.record}
                          key={document.id}
                          href={canonicalUiHref(`${erpBase}/pms/documents/${document.id}?property=${propertyId}`)}
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
                    {currentGuestAccount.data.state !== "closed" &&
                      form(
                        "Add service charge",
                        "charge.add",
                        [field("description", "Item"), field("price", "Price", "money", 0)],
                        { guestAccountId: currentGuestAccount.id },
                      )}
                    {currentGuestAccount.data.state !== "closed" &&
                      form(
                        "Apply deposit to balance due",
                        "payment.apply_deposit",
                        [field("amount", "Amount", "money", 0), field("reason", "Reason")],
                        { guestAccountId: currentGuestAccount.id },
                      )}
                    {currentGuestAccount.data.state !== "closed" &&
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
                        { guestAccountId: currentGuestAccount.id },
                      )}
                    {rows("refund")
                      .filter((row) => row.data.guestAccountId === currentGuestAccount.id)
                      .map((row) => (
                        <RecordSurface as="article" recordId={row.id}
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
                        </RecordSurface>
                      ))}
                    {currentGuestAccount.data.state !== "closed" &&
                      form("Close guest account", "guest_account.close", [], { id: currentGuestAccount.id })}
                  </>
                ) : (
                  <Empty>{tr("The guest account will be created when the reservation is confirmed")}</Empty>
                )}
              </Panel>
            )}</FrontLocalizedTree>;
}
