"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import dynamic from "next/dynamic";
import Link from "@/components/workplace/workspace-link";
import type {ReactNode} from "react";
import type {PmsWorkspace} from "@/server/modules/pms/public/pms";
import {accountLabels,pmsAccountKeys,type Kind,type PmsRecord,type DataMap} from "@/server/modules/pms/public/rules/pms-model";
import {RecordSurface} from "@/components/records/record-surface";
import {FeatureWidget} from "@/components/shared/features/feature-page";
import {FrontLocalizedTree,useFrontLanguage} from "@/components/front/front-language";
import {frontText} from "@/lib/shared/front-language";
import type {Field,Option,PmsForm,Send} from "./workspace-types";
import styles from "./workspace.module.css";
const BookingSettings=dynamic(()=>import("./standard-booking-settings").then(m=>m.StandardBookingSettings));
const BookingOnlineAccounts=dynamic(()=>import("./booking-policy-settings").then(m=>m.BookingOnlineAccounts));
const BookingFeeAccounts=dynamic(()=>import("./booking-policy-settings").then(m=>m.BookingFeeAccounts));
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
type Snapshot=NonNullable<PmsWorkspace["snapshot"]>;
type Props={can:(permission:string)=>boolean;erpBase:string;s:Snapshot;send:Send;data:PmsWorkspace;p:Snapshot["property"];base:string;rows:<K extends Kind>(kind:K)=>PmsRecord<K>[];tr:(text:string)=>string;form:PmsForm;today:string;roomTypes:Option[];rateFields:(data?:DataMap["rate"])=>Field[];money:(value:number)=>string};
export function PmsSettingsPanel({can,erpBase,s,send,data,p,base,rows,tr,form,today,roomTypes,rateFields,money}:Props){return (
                <FrontLocalizedTree>
                  {(can("pms.configure") || can("pms.accounting.approve")) && (
                    <Panel title="Hotel cutover">
                      <Link href={canonicalUiHref(`${erpBase}/pms/import?property=${p.id}`)}>
                        Preview imported hotel records and Finance-approved opening balances →
                      </Link>
                    </Panel>
                  )}
                  {(can("pms.accounting.prepare")||can("pms.accounting.approve"))&&<BookingFeeAccounts snapshot={s!} save={send} canPrepare={can("pms.accounting.prepare")} canApprove={can("pms.accounting.approve")} accounts={data.accounts}/>}
                  {(can("pms.accounting.prepare")||can("pms.accounting.approve"))&&<BookingOnlineAccounts snapshot={s!} save={send} canPrepare={can("pms.accounting.prepare")} canApprove={can("pms.accounting.approve")} accounts={data.accounts}/>}
                  {can("pms.configure") && (
                    <BookingSettings
                      key={p.id}
                      snapshot={s!}
                      save={send}
                      slug={base.split("/")[2]}
                      canReview={data.canReviewBookingMedia}
                      readiness={data.bookingReadiness}
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
                        <RecordSurface recordId={r.id}
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
                        </RecordSurface>
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
                        <RecordSurface recordId={r.id}
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
                        </RecordSurface>
                      ))}
                    </Panel>
                  </>
                  <Panel title="Rate plans">
                    {form("Add rate plan", "rate.save", rateFields())}
                    {rows("rate").map((r) => (
                      <RecordSurface recordId={r.id}
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
                      </RecordSurface>
                    ))}
                  </Panel>
                  <Panel title="Accounting & tax">
                    <Link href={canonicalUiHref(`${erpBase}/hospitality/setup`)}>Hotel + Restaurant Setup</Link>
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
                </FrontLocalizedTree>
              );}
