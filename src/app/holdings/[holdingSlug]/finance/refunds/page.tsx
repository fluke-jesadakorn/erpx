import { Table, TableFrame } from "@/components/ui/foundation";
import { FeaturePage } from "@/components/features/feature-page";
import { FinanceWorkspace, financeStyles as styles } from "@/components/finance/finance-workspace";
import Link from "next/link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { listRefunds } from "@/server/refunds";
import { MoneyAmount } from "@/components/money/currency-provider";
import { bookingRefundsForFinance } from "@/server/pms-booking";
import { BookingRefunds } from "@/components/pms/booking-refunds";
import { CounterRefunds } from "@/components/pms/counter-refunds";
import { pmsStripeRefundsForFinance } from "@/server/pms-stripe";
import { hasPermission } from "@/lib/permissions";
import { pageError } from "@/server/page-error";
import { Card } from "@/components/ui/foundation";
export const dynamic = "force-dynamic";
export default async function RefundsPage({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor)),
    [rows, bookingRefunds, counterRefunds] = await Promise.all([
      listRefunds(actor),
      bookingRefundsForFinance(actor),
      pmsStripeRefundsForFinance(actor),
    ]).catch((error) => pageError(error, actor.holdingSlug));
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <FinanceWorkspace
        base={base}
        active="cash"
        title="Refunds"
        description="Review the original return, beneficiary and obligation before preparing a refund instruction."
      >
        <details className={styles.disclosure}>
          <summary>Hotel booking & counter refunds</summary>
          <div>
            <BookingRefunds
              base={base}
              rows={bookingRefunds}
              canApprove={hasPermission(actor, "finance.approve")}
            />
            <CounterRefunds
              base={base}
              rows={counterRefunds}
              canApprove={hasPermission(actor, "finance.approve")}
            />
          </div>
        </details>
        <Card as="section" appearance="standard" className="ui-panel">
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
                  <th>Source return</th>
                  <th>Counterparty</th>
                  <th>Direction</th>
                  <th>Amount</th>
                  <th>Remaining</th>
                  <th>Status</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((row) => (
                  <tr key={String(row.id)}>
                    <td>
                      <Link href={`${base}/finance/refunds/${row.id}`}>
                        {String(row.return_number)}
                      </Link>
                    </td>
                    <td>{String(row.party_name)}</td>
                    <td>
                      {row.direction === "outgoing" ? "Pay customer" : "Receive from supplier"}
                    </td>
                    <td>
                      <MoneyAmount
                        units={String(row.amount_minor)}
                        currency={String(row.currency)}
                        date={String(row.return_date)}
                        legacyScale={2}
                      />
                    </td>
                    <td>
                      <MoneyAmount
                        units={String(row.outstanding_minor)}
                        currency={String(row.currency)}
                        date={String(row.return_date)}
                        legacyScale={2}
                      />
                    </td>
                    <td>{String(row.status)}</td>
                  </tr>
                ))}
              </tbody>
            </Table>
          </TableFrame>
          {!rows.length && (
            <p>No posted returns with a cash refund obligation in this workspace.</p>
          )}
          {rows.length === 200 && <p>Showing the 200 most recently updated obligations.</p>}
        </Card>
      </FinanceWorkspace>
    </FeaturePage>
  );
}
