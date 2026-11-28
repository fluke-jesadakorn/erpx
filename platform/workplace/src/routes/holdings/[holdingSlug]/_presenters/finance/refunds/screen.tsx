import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { Table, TableFrame } from "@erpx/platform-ui/components/foundation";
import { FeaturePage } from "@erpx/platform-shared-ui/components/features/feature-page";
import { FinanceWorkspace, financeStyles as styles } from "@erpx/finance-ui/components/finance-workspace";
import { RecordLink as Link } from "@erpx/records-ui/components/record-link";
import { requireWorkContext, publicScope } from "@erpx/identity-access-server/public/authorization";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { listRefunds } from "@erpx/finance-server/public/refunds";
import { MoneyAmount } from "@erpx/finance-ui/components/money/currency-provider";
import { bookingRefundsForFinance } from "@erpx/pms-server/public/pms-booking";
import { BookingRefunds } from "@erpx/pms-ui/components/booking-refunds";
import { CounterRefunds } from "@erpx/pms-ui/components/counter-refunds";
import { pmsStripeRefundsForFinance } from "@erpx/pms-server/public/pms-stripe";
import { hasPermission } from "@erpx/identity-access-ui/lib/permissions";
import { pageError } from "@erpx/work-projection-server/public/page-error";
import { Card } from "@erpx/platform-ui/components/foundation";
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
      <FinanceWorkspace actor={actor}
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
              canRecoverReceipt={hasPermission(actor,"finance.approve")}
              canApprove={hasPermission(actor, "pms.approve")}
            />
            <CounterRefunds
              base={base}
              rows={counterRefunds}
              canApprove={hasPermission(actor, "pms.approve")}
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
                      <Link href={canonicalUiHref(`${base}/finance/refunds/${row.id}`)}>
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
