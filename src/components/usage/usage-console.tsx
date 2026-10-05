"use client";
import { CollectionRecordLink, RecordCollectionDrawer } from "@/components/records/record-collection";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, Table, TableFrame, TextInput } from "@/components/ui/foundation";

import { betaFetch } from "@/lib/beta-client";
import { sandboxPricingForMonth } from "@/lib/billing-pricing";
import Link from "next/link";
import { useRouter, useSearchParams } from "next/navigation";
import { useEffect, useState } from "react";
import styles from "./usage-console.module.css";
import { Card } from "@/components/ui/foundation";

type Meter = {
  requests?: number;
  inputTokens?: number;
  outputTokens?: number;
  bytes?: number;
  fileCount?: number;
};
type Branch = { branchId: string; branchName: string; totals: { ai: Meter; storage: Meter } };
type Company = {
  companyId: string;
  companyName: string;
  branches: Branch[];
  unassigned: { ai: Meter; storage: Meter };
  totals: { ai: Meter; storage: Meter };
};
type Holding = {
  holdingId: string;
  holdingSlug: string;
  holdingName: string;
  companies: Company[];
  totals: { ai: Meter; storage: Meter };
  sandboxStoragePreview?: { status: "estimated" | "unpriced"; amountSatang: number | null };
  meterCoverage?: {
    ai: "partial" | "measured";
    storage: "partial" | "measured";
    unreconciledOriginals?: number;
  };
};
type UsageItem = {
  id: string;
  holdingId: string;
  companyId: string;
  companyName: string;
  branchId: string | null;
  branchName: string | null;
  type: "ai" | "storage";
  quantity: number;
  unit: string;
  period: string;
  source: string;
  pricingStatus: "byok" | "simulation" | "unpriced" | "measured";
  estimatedSatang: number | null;
};
type UsageView = {
  month: string;
  scope: "holding" | "branch" | "platform";
  holdings: Holding[];
  items: UsageItem[];
  measured: boolean;
  coverage?: "partial" | "complete";
  accounts?: { holdingId: string; holdingName: string }[];
  unavailable?: { holdingId: string; holdingName: string; reason: string }[];
};

const number = (value: number | undefined) => new Intl.NumberFormat("en-US").format(value ?? 0);
const baht = (satang: number) =>
  new Intl.NumberFormat("en-US", { style: "currency", currency: "THB" }).format(satang / 100);
const size = (bytes: number | undefined) => {
  if (!bytes) return "0 B";
  const unit = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), 3);
  return `${new Intl.NumberFormat("en-US", { maximumFractionDigits: 2 }).format(bytes / 1024 ** unit)} ${["B", "KiB", "MiB", "GiB"][unit]}`;
};
const files = (count: number | undefined) => `${number(count)} ${count === 1 ? "file" : "files"}`;
const meterText = (meter: { ai: Meter; storage: Meter }) =>
  `${number(meter.ai.requests)} AI requests · ${number(meter.ai.inputTokens)} input tokens · ${number(meter.ai.outputTokens)} output tokens · ${size(meter.storage.bytes)} in ${files(meter.storage.fileCount)}`;

function itemStatus(item: UsageItem) {
  if (item.pricingStatus === "byok") return "Customer-paid provider · No ERPX AI charge";
  if (item.pricingStatus === "simulation") return "Simulation · No charge";
  if (item.estimatedSatang !== null && Number.isFinite(item.estimatedSatang))
    return `${baht(item.estimatedSatang)} · Historical Sandbox preview, not an invoice`;
  return "Measured; pricing pending";
}

function itemPriceUsed(item: UsageItem) {
  if (item.pricingStatus === "byok") return "Customer-paid provider";
  if (item.pricingStatus === "simulation") return "Simulation";
  return item.estimatedSatang === null ? "Pending review" : "Historical Sandbox preview";
}

function monthHref(path: string, month: string, holdingId?: string) {
  const params = new URLSearchParams({ month });
  if (holdingId) params.set("holdingId", holdingId);
  return `${path}?${params}`;
}

export function UsageConsole({
  base,
  mode,
  initialMonth,
  initialHoldingId,
}: {
  base: string;
  mode: "platform" | "holding" | "branch";
  initialMonth: string;
  initialHoldingId?: string;
}) {
  const router = useRouter();
  const [month, setMonth] = useState(initialMonth);
  const [holdingId, setHoldingId] = useState(initialHoldingId ?? "");
  const [data, setData] = useState<UsageView | null>(null);
  const [error, setError] = useState("");
  const pricing = sandboxPricingForMonth(month);
  const path = mode === "platform" ? `${base}/manage-erpx/billing/usage` : `${base}/usage`;
  const api = mode === "platform" ? "/api/v1/platform/billing/usage" : `/api/v1${base}/usage`;

  useEffect(() => {
    let active = true;
    const params = new URLSearchParams({ month });
    if (mode === "platform" && holdingId) params.set("holdingId", holdingId);
    betaFetch<UsageView>(`${api}?${params}`)
      .then((view) => {
        if (active) setData(view);
      })
      .catch((reason) => {
        if (active)
          setError(reason instanceof Error ? reason.message : "Usage could not be loaded.");
      });
    return () =>
 {
      active = false;
    };
  }, [api, month, holdingId, mode]);

  function selectMonth(next: string) {
    if (!/^\d{4}-(0[1-9]|1[0-2])$/.test(next)) return;
    setData(null);
    setError("");
    setMonth(next);
    router.replace(monthHref(path, next, holdingId || undefined), { scroll: false });
  }
  function selectHolding(next: string) {
    setData(null);
    setError("");
    setHoldingId(next);
    router.push(monthHref(path, month, next || undefined), { scroll: false });
  }

  const branch = mode === "branch";
  const total = data?.holdings.length
    ? data.holdings.reduce(
        (acc, holding) => ({
          ai: {
            requests: (acc.ai.requests ?? 0) + (holding.totals.ai.requests ?? 0),
            inputTokens: (acc.ai.inputTokens ?? 0) + (holding.totals.ai.inputTokens ?? 0),
            outputTokens: (acc.ai.outputTokens ?? 0) + (holding.totals.ai.outputTokens ?? 0),
          },
          storage: {
            bytes: (acc.storage.bytes ?? 0) + (holding.totals.storage.bytes ?? 0),
            fileCount: (acc.storage.fileCount ?? 0) + (holding.totals.storage.fileCount ?? 0),
          },
        }),
        { ai: {} as Meter, storage: {} as Meter },
      )
    : null;
  const selected =
    mode === "platform"
      ? data?.holdings.find((holding) => holding.holdingId === holdingId)
      : data?.holdings[0];
  return (
    <div className={`${styles.root} content-stack`}>
      <RecordCollectionDrawer<UsageView> kind="usage_item" title="Usage source" endpoint={`${api}?${new URLSearchParams({month,...(holdingId?{holdingId}:{})})}`} select={(snapshot,id)=>snapshot.items.find(row=>row.id===id) as unknown as Record<string,unknown>??null}/>
      <RecordCollectionDrawer<UsageView> kind="usage_group" title="Usage source records" endpoint={`${api}?${new URLSearchParams({month,...(holdingId?{holdingId}:{})})}`} select={(snapshot,id)=>{
        for(const holding of snapshot.holdings){
          if(id===holding.holdingId)return {id,name:holding.holdingName,holdingId:holding.holdingId};
          for(const company of holding.companies){
            if(id===company.companyId)return {id,name:company.companyName,holdingId:holding.holdingId,companyId:company.companyId};
            const branch=company.branches.find(branch=>branch.branchId===id);
            if(branch)return {id,name:branch.branchName,holdingId:holding.holdingId,companyId:company.companyId,branchId:branch.branchId};
            if(id===`${company.companyId}:unassigned`)return {id,name:'Company-wide / unassigned Branch',holdingId:holding.holdingId,companyId:company.companyId,branchId:null};
          }
        }return null;
      }} render={(record,snapshot)=><UsageSourceRecords record={record} snapshot={snapshot}/>}/>

      <header className={styles.heading}>
        <div>
          <h1>
            {mode === "platform" ? "Customer usage" : branch ? "Branch usage" : "Holding usage"}
          </h1>
          <p>
            Measured AI and original file storage, shown for the selected month. This page is
            read-only.
          </p>
        </div>
        {!branch && <span className={styles.badge}>SEAT-ONLY SANDBOX TARGET</span>}
      </header>
      {!branch && (
        <section className={styles.notice} aria-label="Pricing model status">
          <strong>
            Current target: THB 499 per active named seat-month from the first seat, with no Holding
            base or entry fee.
          </strong>{" "}
          Each paid seat-month proposes 2 GB of original-file allowance, then THB 5 per excess
          GB-month; verified ERPX-funded AI cost has a 10% markup. This is a Sandbox hypothesis, not
          an assigned price or storage entitlement.
          {pricing && (
            <>
              {" "}
              The earlier {month} preview used {baht(pricing.baseMonthlySatang)} per Holding,{" "}
              {number(pricing.includedOriginalStorageBytes / 1_000_000_000)} GB, and AI cost plus{" "}
              {pricing.aiMarkupPercent}%; its history is preserved separately.
            </>
          )}{" "}
          The current 1 GiB per Company upload cap still applies. Open Billing for target-rated
          charges. No amount on this page is an approved invoice.
        </section>
      )}
      <div className={styles.toolbar}>
        <Field appearance="standard">
          Month{" "}
          <TextInput appearance="standard"
            aria-label="Usage month"
            type="month"
            value={month}
            onChange={(event) => selectMonth(event.target.value)}
          />
        </Field>
        {mode === "platform" && data && (
          <Field appearance="standard">
            Customer Holding
            <Select appearance="standard"
              aria-label="Customer Holding"
              value={holdingId}
              onChange={(event) => selectHolding(event.target.value)}
            >
              <option value="">All customers</option>
              {(data.accounts?.length
                ? data.accounts.map((account) => ({
                    id: account.holdingId,
                    name: account.holdingName,
                  }))
                : data.holdings.map((holding) => ({
                    id: holding.holdingId,
                    name: holding.holdingName,
                  }))
              ).map((account) => (
                <option key={account.id} value={account.id}>
                  {account.name}
                </option>
              ))}
            </Select>
          </Field>
        )}
        {mode === "platform" && (
          <Link
            href={`${base}/manage-erpx/billing?tab=target${holdingId ? `&holdingId=${encodeURIComponent(holdingId)}` : ""}`}
          >
            Target billing review
          </Link>
        )}
        {mode === "holding" && (
          <Link href={`${base}/settings?section=usage`}>Usage &amp; billing</Link>
        )}
      </div>
      {error && (
        <InlineNotice tone="danger" className={styles.error} role="alert">
          {error}
        </InlineNotice>
      )}
      {!error && !data && <InlineNotice tone="info" role="status">Loading measured usage…</InlineNotice>}
      {data && (
        <>
          {!data.measured ? (
            <p className={styles.notice}>Measurement data is not available for this period.</p>
          ) : (
            <>
              {total && (
                <section className={styles.summary} aria-label="Usage summary">
                  <div>
                    <span>AI requests</span>
                    <strong>{number(total.ai.requests)}</strong>
                  </div>
                  <div>
                    <span>Input tokens</span>
                    <strong>{number(total.ai.inputTokens)}</strong>
                  </div>
                  <div>
                    <span>Output tokens</span>
                    <strong>{number(total.ai.outputTokens)}</strong>
                  </div>
                  <div>
                    <span>Original files</span>
                    <strong>{size(total.storage.bytes)}</strong>
                    <small>{files(total.storage.fileCount)}</small>
                  </div>
                </section>
              )}
              {data.coverage === "partial" && (
                <p className={styles.subtle}>
                  Coverage is partial. These counts include sources currently measured by ERPX and
                  may not represent every historical item.
                </p>
              )}
              {mode === "platform" && !holdingId && (
                <Card as="section" appearance="standard" className="ui-panel">
                  <h2>Customer Holdings</h2>
                  <TableFrame appearance="standard" label="Records" className={styles.tableScroll}>
                    <Table className="business-table">
                      <thead>
                        <tr>
                          <th>Holding</th>
                          <th>AI requests</th>
                          <th>Input tokens</th>
                          <th>Output tokens</th>
                          <th>Original files</th>
                          <th>Storage coverage</th>
                          <th>Historical storage preview</th>
                          <th>Unpriced meter groups</th>
                          <th>Overall estimate</th>
                        </tr>
                      </thead>
                      <tbody>
                        {data.holdings.map((holding) => (
                          <tr key={holding.holdingId}>
                            <td>
                              <Button appearance="standard"
                                variant="ghost"
                                type="button"
                                className={styles.linkButton}
                                onClick={() => selectHolding(holding.holdingId)}
                              >
                                {holding.holdingName}
                              </Button><CollectionRecordLink kind="usage_group" id={holding.holdingId}>View source records</CollectionRecordLink>
                            </td>
                            <td>{number(holding.totals.ai.requests)}</td>
                            <td>{number(holding.totals.ai.inputTokens)}</td>
                            <td>{number(holding.totals.ai.outputTokens)}</td>
                            <td>{size(holding.totals.storage.bytes)}</td>
                            <td>
                              {holding.meterCoverage?.storage === "measured"
                                ? "Reconciled source"
                                : "Storage measurement incomplete"}
                            </td>
                            <td>
                              {holding.sandboxStoragePreview?.status === "estimated" &&
                              holding.sandboxStoragePreview.amountSatang !== null
                                ? `${baht(holding.sandboxStoragePreview.amountSatang)} · Historical hypothesis`
                                : "Measured; pricing pending"}
                            </td>
                            <td>
                              {number(
                                data.items.filter(
                                  (item) =>
                                    item.holdingId === holding.holdingId &&
                                    item.pricingStatus === "unpriced",
                                ).length,
                              )}
                            </td>
                            <td>Unavailable · cost validation pending</td>
                          </tr>
                        ))}
                        {data.unavailable?.map((holding) => (
                          <tr key={holding.holdingId}>
                            <td>{holding.holdingName}</td>
                            <td colSpan={8}>Meter source unavailable</td>
                          </tr>
                        ))}
                      </tbody>
                    </Table>
                  </TableFrame>
                  {!data.holdings.length && !data.unavailable?.length && (
                    <p>No customer Holdings are available.</p>
                  )}
                </Card>
              )}
              {selected && !branch && (
                <section className={styles.notice} aria-label="Historical Sandbox preview status">
                  <strong>Historical Sandbox storage preview: </strong>
                  {selected.sandboxStoragePreview?.status === "estimated" &&
                  selected.sandboxStoragePreview.amountSatang !== null
                    ? baht(selected.sandboxStoragePreview.amountSatang)
                    : "Measured; pricing pending"}
                  . This older experiment uses the proposed storage price and does not change the
                  current storage cap. Target-rated charges appear separately in Billing after the
                  meter inputs and costs are validated.
                </section>
              )}
              {selected?.meterCoverage?.storage !== "measured" && selected && (
                <InlineNotice tone="info" className={styles.notice} role="status">
                  <strong>Storage measurement incomplete.</strong>{" "}
                  {branch
                    ? "Some original files may not appear in this Branch view."
                    : selected.meterCoverage?.unreconciledOriginals
                      ? `${number(selected.meterCoverage.unreconciledOriginals)} original files have not been reconciled with the storage meter. The visible file count and bytes are not a complete total.`
                      : "The visible file count and bytes may not be a complete total."}
                </InlineNotice>
              )}
              {selected && (
                <Card as="section" appearance="standard" className="ui-panel">
                  <h2>
                    {branch
                      ? "Branch breakdown"
                      : `${selected.holdingName} · Company and Branch breakdown`}
                  </h2>
                  <p className={styles.subtle}>
                    {branch
                      ? "Only usage attributed to this Branch is shown."
                      : "Company totals include Branch rows and Company-wide / unassigned items."}
                  </p>
                  {selected.companies.map((company) => (
                    <details key={company.companyId} className={styles.group} open={branch}>
                      <summary>
                        <CollectionRecordLink kind="usage_group" id={company.companyId}>{company.companyName}</CollectionRecordLink>
                        <span>{meterText(company.totals)}</span>
                      </summary>
                      <TableFrame appearance="standard" label="Records" className={styles.tableScroll}>
                        <Table className="business-table">
                          <thead>
                            <tr>
                              <th>Location</th>
                              <th>AI requests</th>
                              <th>Input tokens</th>
                              <th>Output tokens</th>
                              <th>Original files</th>
                            </tr>
                          </thead>
                          <tbody>
                            {company.branches.map((row) => (
                              <tr key={row.branchId}>
                                <td><CollectionRecordLink kind="usage_group" id={row.branchId}>{row.branchName}</CollectionRecordLink></td>
                                <td>{number(row.totals.ai.requests)}</td>
                                <td>{number(row.totals.ai.inputTokens)}</td>
                                <td>{number(row.totals.ai.outputTokens)}</td>
                                <td>
                                  {size(row.totals.storage.bytes)} ·{" "}
                                  {files(row.totals.storage.fileCount)}
                                </td>
                              </tr>
                            ))}
                            {!branch && (
                              <tr>
                                <td><CollectionRecordLink kind="usage_group" id={`${company.companyId}:unassigned`}>Company-wide / unassigned Branch</CollectionRecordLink></td>
                                <td>{number(company.unassigned.ai.requests)}</td>
                                <td>{number(company.unassigned.ai.inputTokens)}</td>
                                <td>{number(company.unassigned.ai.outputTokens)}</td>
                                <td>
                                  {size(company.unassigned.storage.bytes)} ·{" "}
                                  {files(company.unassigned.storage.fileCount)}
                                </td>
                              </tr>
                            )}
                          </tbody>
                        </Table>
                      </TableFrame>
                    </details>
                  ))}
                  {!selected.companies.length && <p>No Company data is available.</p>}
                </Card>
              )}
              {(mode !== "platform" || holdingId) && (
                <Card as="section" appearance="standard" className="ui-panel">
                  <h2>Measured items</h2>
                  <TableFrame appearance="standard" label="Records" className={styles.tableScroll}>
                    <Table className="business-table">
                      <thead>
                        <tr>
                          <th>Type</th>
                          <th>Company</th>
                          <th>Branch</th>
                          <th>Quantity</th>
                          <th>Period</th>
                          <th>Source</th>
                          {!branch && (
                            <>
                              <th>Price used</th>
                              <th>Estimate / status</th>
                            </>
                          )}
                        </tr>
                      </thead>
                      <tbody>
                        {data.items.map((item) => (
                          <tr key={item.id}>
                            <td><CollectionRecordLink kind="usage_item" id={item.id}>{item.type === "ai" ? "AI" : "Original files"}</CollectionRecordLink></td>
                            <td>{item.companyName}</td>
                            <td>{item.branchName ?? "Company-wide / unassigned"}</td>
                            <td>
                              {number(item.quantity)} {item.unit}
                            </td>
                            <td>{item.period}</td>
                            <td>{item.source}</td>
                            {!branch && (
                              <>
                                <td>{itemPriceUsed(item)}</td>
                                <td>{itemStatus(item)}</td>
                              </>
                            )}
                          </tr>
                        ))}
                      </tbody>
                    </Table>
                  </TableFrame>
                  {!data.items.length && (
                    <p>No usage was measured by the currently available sources for this month.</p>
                  )}
                </Card>
              )}
            </>
          )}
        </>
      )}
    </div>
  );
}

function UsageSourceRecords({record,snapshot}:{record:Record<string,unknown>;snapshot:UsageView}){
 const query=useSearchParams(),page=Math.max(1,Number(query.get('detailPage'))||1),all=snapshot.items.filter(row=>row.holdingId===record.holdingId&&(!record.companyId||row.companyId===record.companyId)&&(!Object.hasOwn(record,'branchId')||row.branchId===record.branchId)),rows=all.slice((page-1)*30,page*30);
 const href=(value:number)=>{const q=new URLSearchParams(query.toString());q.set('detailPage',String(value));return `?${q}`;};
 return <div className="ui-stack"><h2>{String(record.name)}</h2><p>{snapshot.month} · {all.length} source records</p><TableFrame appearance="standard" label="Usage sources"><Table><thead><tr><th>Source</th><th>Quantity</th><th>Status</th></tr></thead><tbody>{rows.map(row=><tr key={row.id}><td><CollectionRecordLink kind="usage_item" id={row.id}>{row.source} · {row.companyName} · {row.branchName??'Unassigned'}</CollectionRecordLink></td><td>{number(row.quantity)} {row.unit}</td><td>{itemStatus(row)}</td></tr>)}</tbody></Table></TableFrame><nav aria-label="Usage source pagination">{page>1&&<Link href={href(page-1)}>Previous</Link>}{page*30<all.length&&<Link href={href(page+1)}>Next</Link>}</nav></div>;
}
