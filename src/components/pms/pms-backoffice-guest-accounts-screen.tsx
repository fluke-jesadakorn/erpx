"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import type { Option, Field } from "./workspace-types";
import { CollectionRecordLink } from "@/components/records/record-collection";
import { RecordSurface } from "@/components/records/record-surface";
import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { Button, Field as UiField, Select, Table, TableFrame } from "@/components/ui/foundation";
import { FeatureWidget } from "@/components/shared/features/feature-page";
import { workFetch } from "@/lib/work_projection/work-fetch";
import { TimeText } from "@/components/people_payroll/time/time-preference-context";
import { Hotel } from "lucide-react";
import Link from "@/components/workplace/workspace-link";
import { type ReactNode } from "react";
import dynamic from "next/dynamic";
import styles from "./workspace.module.css";
import { FrontLocalizedTree, useFrontLanguage } from "@/components/front/front-language";
import { frontText } from "@/lib/shared/front-language";
import type {BackofficePmsWorkspaceGuestAccountsScreenContext as BackofficePmsWorkspaceContext} from "./backoffice-screen-context";


const CounterRefunds = dynamic(() => import("./counter-refunds").then(module => module.CounterRefunds));


const RoomChargeReversalReviews = dynamic(() => import("@/components/shared/hospitality/room-charge-reversals").then(module => module.RoomChargeReversalReviews));


const TaxDocumentReference = dynamic(() => import("@/components/shared/hospitality/tax-document-reference").then(module => module.TaxDocumentReference));


const PmsPromptPay = dynamic(() => import("./promptpay-counter").then(module => module.PmsPromptPay));


const BookingRefundRecovery = dynamic(() => import("./booking-refund-recovery").then(module => module.BookingRefundRecovery));


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
export function BackofficePmsWorkspaceGuestAccountsScreen({context}: {context: BackofficePmsWorkspaceContext & {p: NonNullable<BackofficePmsWorkspaceContext["p"]>}}) {
  const {currentGuestAccount, setGuestAccountId, guestAccounts, guestName, money, rows, data, form, propertyId, erpBase, payments, base, p, openShifts, can, crossLinks, tr, frontMode, router, setError, setNotice} = context;
  return <FrontLocalizedTree>{(
                  <>
                    <Panel title="Guest Account">
                      <UiField className={styles.guestAccountPicker}>
                        Select guest account
                        <Select
                          value={currentGuestAccount?.id ?? ""}
                          onChange={(e) => setGuestAccountId(e.target.value)}
                        >
                          {guestAccounts.map((f) => (
                            <option key={f.id} value={f.id}>
                              {f.name}
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
                            ].map(([label, n]) => (
                              <div className={styles.stat} key={String(label)}>
                                <span>{label}</span>
                                <strong>
                                  <StatAmount formatted={money(Number(n))} />
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
                                  <th>Date</th>
                                  <th>Item</th>
                                  <th>Amount</th>
                                  <th>Actions</th>
                                </tr>
                              </thead>
                              <tbody>
                                {rows("entry")
                                  .filter((e) => e.data.guestAccountId === currentGuestAccount.id)
                                  .map((e) => (
                                    <tr key={e.id}>
                                      <td>{e.data.date}</td>
                                      <td>
                                        <CollectionRecordLink kind="pms_entry" id={e.id}>{e.data.description}</CollectionRecordLink>
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
                                                "guest_account.transfer",
                                                [
                                                  select(
                                                    "toGuestAccountId",
                                                    "Destination guest account",
                                                    guestAccounts.filter((f) => f.id !== currentGuestAccount.id),
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
                              href={canonicalUiHref(`${erpBase}/pms/guest-accounts?property=${encodeURIComponent(propertyId)}&queue=corrections`)}
                            >
                              Manage guest account corrections
                            </UiActionLink>
                          )}
                          <>
                            <div>
                              {form(
                                "Receive payment / advance",
                                "payment.receive",
                                [
                                  field("amount", "Amount", "money", currentGuestAccount.data.due / 100),
                                  ...payments,
                                ],
                                { guestAccountId: currentGuestAccount.id },
                              )}
                              {form(
                                "Receive deposit",
                                "payment.deposit",
                                [field("amount", "Amount", "money", 0), ...payments],
                                { guestAccountId: currentGuestAccount.id },
                              )}
                              {data.stripeConnectionReady && (
                                <PmsPromptPay
                                  base={base}
                                  propertyId={p.id}
                                  guestAccountId={currentGuestAccount.id}
                                  dueMinor={currentGuestAccount.data.due}
                                  initialId={
                                    data.stripePayments.find(
                                      (item) =>
                                        item.guest_account_id === currentGuestAccount.id &&
                                        ["requested", "pending"].includes(item.state),
                                    )?.id
                                  }
                                  history={data.stripePayments.filter(
                                    (item) => item.guest_account_id === currentGuestAccount.id,
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
                                      currentGuestAccount.data.due / 100,
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
                                  { guestAccountId: currentGuestAccount.id },
                                )}
                              {data.transferRequests
                                .filter((item) => item.guest_account_id === currentGuestAccount.id)
                                .map((item) => {
                                  const bankChoices = data.transferBankEvidence.filter(
                                    (bank) =>
                                      bank.external_id === item.reference &&
                                      Number(bank.amount_minor) === Number(item.amount_minor) &&
                                      bank.currency === item.currency &&
                                      bank.transaction_date === item.business_date,
                                  );
                                  return (
                                    <RecordSurface as="article" recordId={item.id}
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
                                          ? "recorded; no guest account payment has posted"
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
                                                  <Link href={canonicalUiHref(`${erpBase}/finance/bank`)}>
                                                    Review bank evidence in Finance
                                                  </Link>
                                                )}
                                              </p>
                                            ))}
                                        </>
                                      )}
                                    </RecordSurface>
                                  );
                                })}
                              {form(
                                "Add service charge",
                                "charge.add",
                                [field("description", "Item"), field("price", "Price", "money", 0)],
                                { guestAccountId: currentGuestAccount.id },
                              )}
                              {form(
                                "Apply deposit to balance due",
                                "payment.apply_deposit",
                                [field("amount", "Amount", "money", 0), field("reason", "Reason")],
                                { guestAccountId: currentGuestAccount.id },
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
                                { guestAccountId: currentGuestAccount.id },
                              )}
                            </div>
                            <div>
                              {form(
                                "Create separate guest account",
                                "guest_account.create",
                                [field("name", "Guest Account name")],
                                { reservationId: currentGuestAccount.data.reservationId },
                              )}
                              {form(
                                "Merge guest accounts",
                                "guest_account.merge",
                                [
                                  select(
                                    "toGuestAccountId",
                                    "Destination guest account",
                                    guestAccounts.filter((f) => f.id !== currentGuestAccount.id),
                                  ),
                                ],
                                { id: currentGuestAccount.id },
                              )}
                              {form(
                                "Transfer company / agent receivable",
                                "guest_account.credit",
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
                                { id: currentGuestAccount.id },
                              )}
                              {form(
                                "Record OTA / company settlement",
                                "ota.settle",
                                [
                                  field(
                                    "gross",
                                    "Receivable settled",
                                    "money",
                                    currentGuestAccount.data.due / 100,
                                  ),
                                  field("commission", "Commission", "money", 0),
                                  field("fees", "Fees", "money", 0),
                                  field("reference", "Reference"),
                                  select("shiftId", "Cashier shift", openShifts, openShifts[0]?.id),
                                ],
                                { guestAccountId: currentGuestAccount.id },
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
                              {form("Issue invoice / close guest account", "guest_account.close", [], {
                                id: currentGuestAccount.id,
                              })}
                            </div>
                          </>
                        </>
                      ) : (
                        <Empty>The guest account will be created when the reservation is confirmed.</Empty>
                      )}
                    </Panel>
                    <Panel title="Refunds & credit">
                      {can("pms.approve") && (
                        <UiActionLink
                          variant="secondary"
                          href={canonicalUiHref(`${erpBase}/pms/cashier?property=${propertyId}&queue=approval`)}
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
                          <RecordSurface as="article" recordId={r.id}
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
                          </RecordSurface>
                        ))}
                      {form("Request company / agent credit", "credit.request", [
                        select("partyId", "Customer", data.parties),
                        field("limit", "Credit limit", "money", 0),
                        field("terms", "Credit days", "number", 30),
                      ])}
                      {rows("credit")
                        .filter((r) => r.data.state === "requested")
                        .map((r) => (
                          <RecordSurface recordId={r.id}
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
                          </RecordSurface>
                        ))}
                    </Panel>
                    <RoomChargeReversalReviews base={frontMode ? base : erpBase} propertyId={propertyId} owner={{userId:data.userId,companyId:p.companyId,property:{id:p.id,companyId:p.companyId,branchId:p.branchId,version:p.version}}}/>
                    <Panel title="Guest Account charge corrections">
                      {data.chargeCorrections.map((item) => (
                        <RecordSurface as="article" recordId={item.id}
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
                        </RecordSurface>
                      ))}
                      {!data.chargeCorrections.length && (
                        <Empty>No guest account corrections are awaiting review.</Empty>
                      )}
                    </Panel>
                    {data.counterRefunds.length > 0 && <CounterRefunds base={base} rows={data.counterRefunds} canApprove={can("pms.approve")}/>}
                    {data.bookingRefunds.length > 0 && (
                      <Panel title="Online booking refunds for PMS">
                        {data.bookingRefunds.map((item) => (
                          <RecordSurface as="article" recordId={item.id}
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
                            {(item.refund_state === "failed" && !item.needs_receipt_recovery || item.needs_receipt_recovery && can("finance.approve")) && <BookingRefundRecovery base={base} id={item.id} receipt={item.needs_receipt_recovery} onSaved={()=>router.refresh()}/> }
                            {item.refund_state === "requested" && !item.needs_receipt_recovery && (
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
                          </RecordSurface>
                        ))}
                      </Panel>
                    )}
                    <Panel title="Documents">
                      {!currentGuestAccount?.data.invoiceId &&
                        !data.documents.some(
                          (d) => !currentGuestAccount || d.guest_account_id === currentGuestAccount.id,
                        ) && <Empty>No documents available for this guest account yet.</Empty>}
                      {currentGuestAccount?.data.invoiceId && (
                        <TaxDocumentReference
                          base={erpBase}
                          invoiceId={currentGuestAccount.data.invoiceId}
                        />
                      )}
                      {data.documents
                        .filter((d) => !currentGuestAccount || d.guest_account_id === currentGuestAccount.id)
                        .map((d) => (
                          <Link
                            className={styles.line}
                            key={d.id}
                            href={canonicalUiHref(`${erpBase}/pms/documents/${d.id}?property=${p.id}`)}
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
                )}</FrontLocalizedTree>;
}
