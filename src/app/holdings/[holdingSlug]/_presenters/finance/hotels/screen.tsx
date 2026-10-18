import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { HotelTransferReviews } from "@/components/finance/hotel-transfer-reviews";
import { Table, TableFrame } from "@/components/ui/foundation";
import { RecordLink as Link } from "@/components/records/record-link";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { pmsFinanceOverview } from "@/server/modules/pms/public/pms/finance";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { FinanceWorkspace, financeStyles as styles } from "@/components/finance/finance-workspace";
import { MoneyAmount } from "@/components/finance/money/currency-provider";

export const dynamic = "force-dynamic";
export default async function HotelAccounting({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ property?: string }>;
}) {
  const { holdingSlug } = await params,
    actor = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(actor));
  const { property } = await searchParams;
  const data = await pmsFinanceOverview(actor, property).catch((error) =>
    pageError(error, holdingSlug),
  );
  return (
    <FinanceWorkspace
      base={base}
      active="hotels"
      title="Hotel source reconciliation"
      description="Compare hotel receivables, advances and deposits to posted GL. Open the original record, bank evidence and document."
    >
      <div className={styles.sectionHead}>
        <Link href={canonicalUiHref(`${base}/finance/hotels`)}>All accessible hotels</Link>
        <Link href={canonicalUiHref(`${base}/finance/hospitality-settings`)}>Finance setup review →</Link>
      </div>
      {new Set(data.hotels.map((hotel) => hotel.businessDate)).size > 1 && (
        <p className="alert">
          Business dates differ across hotels. Each row is as of its hotel&apos;s business date.
        </p>
      )}
      <section className={styles.section}>
        <h2>PMS → General ledger</h2>
        <TableFrame appearance="standard"
          label="Records"
          className="business-table-wrap"
          tabIndex={0}
          role="region"
          aria-label="Financial table — scroll for more columns"
        >
          <Table className="business-table">
            <thead>
              <tr>
                <th>Hotel / Business date</th>
                <th>Account</th>
                <th>PMS</th>
                <th>Posted GL</th>
                <th>Difference</th>
                <th>Review</th>
              </tr>
            </thead>
            <tbody>
              {data.hotels.flatMap((hotel) =>
                (["receivable", "advance", "deposit"] as const).map((account) => (
                  <tr key={`${hotel.id}-${account}`}>
                    <td>
                      <Link
                        href={canonicalUiHref(`${base}/finance/hotels?property=${encodeURIComponent(hotel.id)}`)}
                      >
                        {hotel.name}
                      </Link>
                      <br />
                      <small>
                        {hotel.businessDate} · {hotel.currency}
                      </small>
                    </td>
                    <td>
                      {account === "receivable"
                        ? "Receivables"
                        : account === "advance"
                          ? "Advances"
                          : "Security deposits"}
                    </td>
                    <td>
                      <MoneyAmount
                        units={hotel[account] ?? 0}
                        currency={hotel.currency}
                        date={hotel.businessDate}
                        legacyScale={2}
                      />
                    </td>
                    <td>
                      <MoneyAmount
                        units={hotel.ledger[account]}
                        currency={hotel.currency}
                        date={hotel.businessDate}
                        legacyScale={2}
                      />
                    </td>
                    <td>
                      <strong data-tone={hotel.differences[account] ? "danger" : "good"}>
                        <MoneyAmount
                          units={hotel.differences[account]}
                          currency={hotel.currency}
                          date={hotel.businessDate}
                          legacyScale={2}
                        />
                      </strong>
                    </td>
                    <td>
                      <Link href={canonicalUiHref(`${base}/finance/hotels?property=${encodeURIComponent(hotel.id)}`)}>
                        Reconciliation →
                      </Link>
                      <br />
                      <Link href={canonicalUiHref(`${base}/finance/hotel-import?property=${encodeURIComponent(hotel.id)}`)}>
                        Cutover preview →
                      </Link>
                    </td>
                  </tr>
                )),
              )}
            </tbody>
          </Table>
        </TableFrame>
        {!data.hotels.length && <p>No accessible hotels.</p>}
      </section>
      <section className={styles.section}>
        <h2>Online payment liabilities</h2>
        <TableFrame appearance="standard" label="Online payment liabilities"><Table className="business-table"><thead><tr><th>Hotel</th><th>Account</th><th>PMS original entries</th><th>Posted GL</th><th>Difference</th></tr></thead><tbody>{data.hotels.flatMap(hotel=>(['refundPayable','unallocated'] as const).map(account=><tr key={`${hotel.id}-${account}`}><td><Link href={canonicalUiHref(`${base}/finance/hotels?property=${encodeURIComponent(hotel.id)}`)}>{hotel.name}</Link></td><td>{account==='refundPayable'?'Refunds payable':'Unallocated guest receipts'}</td><td><MoneyAmount units={hotel.online[account]} currency={hotel.currency} date={hotel.businessDate} legacyScale={2}/></td><td><MoneyAmount units={hotel.onlineLedger[account]} currency={hotel.currency} date={hotel.businessDate} legacyScale={2}/></td><td><strong data-tone={hotel.onlineDifferences[account]?'danger':'good'}><MoneyAmount units={hotel.onlineDifferences[account]} currency={hotel.currency} date={hotel.businessDate} legacyScale={2}/></strong></td></tr>))}</tbody></Table></TableFrame>
      </section>
      <HotelTransferReviews base={base} reviews={data.reviews}/>
      <section className={styles.section}>
        <h2>Bank transfers</h2>
        <p className="muted">
          Pending transfers have no receipt or GL entry. A different Finance person must match an
          unused bank transaction before settlement.
        </p>
        <TableFrame appearance="standard"
          label="Records"
          className="business-table-wrap"
          tabIndex={0}
          role="region"
          aria-label="Financial table — scroll for more columns"
        >
          <Table className="business-table">
            <thead>
              <tr>
                <th>Reference</th>
                <th>Amount</th>
                <th>State</th>
                <th>Trace</th>
              </tr>
            </thead>
            <tbody>
              {data.reviews.map((review) => (
                <tr key={review.id}>
                  <td>{review.reference}</td>
                  <td>
                    <MoneyAmount
                      units={review.amount_minor}
                      currency={review.currency}
                      legacyScale={2}
                    />
                  </td>
                  <td>{review.state}</td>
                  <td>
                    <Link
                      href={canonicalUiHref(`${base}/finance/hotel-accounts/${review.guest_account_id}`)}
                    >
                      Hotel review
                    </Link>
                    {review.finance_payment_id && (
                      <>
                        {" "}
                        ·{" "}
                        <Link href={canonicalUiHref(`${base}/finance/payments/${review.finance_payment_id}`)}>
                          Receipt in Finance
                        </Link>
                      </>
                    )}
                    {review.bank_transaction_id && (
                      <>
                        {" "}
                        ·{" "}
                        <Link href={canonicalUiHref(`${base}/finance/bank?match=${review.bank_transaction_id}`)}>
                          Bank evidence
                        </Link>
                      </>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </Table>
        </TableFrame>
        {!data.reviews.length && <p>No bank transfer records.</p>}
      </section>
      <section className={styles.section}>
        <h2>Unposted source entries</h2>
        <p className="muted">
          Nonzero PMS entries without a posted originating journal need investigation.
        </p>
        {data.exceptions.map((entry) => (
          <p key={entry.id}>
            <Link
              href={canonicalUiHref(`${base}/finance/hotel-accounts/${entry.guest_account_id}`)}
            >
              {entry.description} · {entry.kind} →
            </Link>
          </p>
        ))}
        {!data.exceptions.length && <p>No missing posted journals in the selected hotels.</p>}
      </section>
      <section className={styles.section}>
        <h2>Original documents</h2>
        <TableFrame appearance="standard"
          label="Records"
          className="business-table-wrap"
          tabIndex={0}
          role="region"
          aria-label="Financial table — scroll for more columns"
        >
          <Table className="business-table">
            <thead>
              <tr>
                <th>Document</th>
                <th>Guest account</th>
                <th>Finance</th>
                <th>Journal</th>
              </tr>
            </thead>
            <tbody>
              {data.documents.map((doc) => (
                <tr key={doc.id}>
                  <td>
                    <Link href={canonicalUiHref(`${base}/finance/hotel-documents/${doc.id}`)}>
                      {doc.kind} · {doc.id.slice(-8)}
                    </Link>
                  </td>
                  <td>
                    <Link
                      href={canonicalUiHref(`${base}/finance/hotel-accounts/${doc.guest_account_id}`)}
                    >
                      Open guest account
                    </Link>
                  </td>
                  <td>
                    {doc.finance_id ? (
                      <Link href={canonicalUiHref(`${base}/finance/${doc.finance_kind}/${doc.finance_id}`)}>
                        Open Finance record
                      </Link>
                    ) : (
                      "—"
                    )}
                  </td>
                  <td>
                    {doc.journal_id ? (
                      <Link href={canonicalUiHref(`${base}/finance/journals/${doc.journal_id}`)}>Open journal</Link>
                    ) : (
                      "See source entries"
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </Table>
        </TableFrame>
        <p className="muted">
          Latest 100 documents and review records. Filter by hotel to inspect its evidence. PMS
          accounting is THB; displayed currency conversions are reference amounts.
        </p>
      </section>
    </FinanceWorkspace>
  );
}
