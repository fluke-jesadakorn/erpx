import { FinanceRecordWorkspace, FinanceLegacyHeading } from "@/components/finance/finance-record-workspace";
import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import {
EmptyState,
PageHeader,
StatusBadge,
} from "@/components/ui/primitives";
import { MoneyAmount } from "@/components/money/currency-provider";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { listExpenses } from "@/server/expenses";
import Link from "next/link";
export const dynamic = "force-dynamic";
export default async function Expenses({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ q?: string; status?: string }>;
}) {
  const { holdingSlug } = await params,
    actor = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(actor));
  const [expenses, search] = await Promise.all([
    listExpenses(actor),
    searchParams,
  ]);
  const filtered = expenses.filter(
    (x) =>
      (!search.status || x.status === search.status) &&
      (!search.q ||
        `${x.legal_name} ${x.merchant} ${x.receipt_reference} ${x.business_purpose}`
          .toLowerCase()
          .includes(search.q.toLowerCase())),
  );
  return (
    <FeaturePage className="content-stack" data-source="live">
      <FinanceRecordWorkspace base={base} active="receivables" title="Employee expenses" description="Review receipts, accounting and reimbursement evidence." actions={<Link className="button" href={`${base}/expenses/new`}>New expense claim</Link>}>
      <FinanceLegacyHeading><PageHeader
        eyebrow="People · Finance"
        title="Employee expenses"
        description="Claim business costs, review receipts, post the obligation, and track reimbursement."
      /></FinanceLegacyHeading><FeatureTabs/><WidgetGrid pageKey="finance-expenses">
      <FinanceLegacyHeading><Link className="button" href={`${base}/expenses/new`}>
        New expense claim
      </Link></FinanceLegacyHeading>
      <form className="business-form" action={`${base}/expenses`}>
        <label>
          Search claims
          <input
            name="q"
            defaultValue={search.q}
            placeholder="Employee, merchant, receipt or purpose"
          />
        </label>
        <label>
          Claim status
          <select name="status" defaultValue={search.status ?? ""}>
            <option value="">All statuses</option>
            {[
              "draft",
              "in_review",
              "approved",
              "rejected",
              "posted",
              "reimbursed",
              "cancelled",
            ].map((s) => (
              <option key={s} value={s}>
                {s.replaceAll("_", " ")}
              </option>
            ))}
          </select>
        </label>
        <button className="button secondary">Filter claims</button>
      </form>
      <FeatureWidget id="finance-expenses-expense-claims" title="Expense claims">
        <div className="section-head">
          <h2>Expense claims</h2>
          <span className="badge">{filtered.length}</span>
        </div>
        {filtered.length === 0 ? (
          <EmptyState
            title="No matching claims"
            description="Create a claim or change the filters to view expenses in your scope."
          />
        ) : (
          filtered.map((x) => (
            <Link
              className="row"
              href={`${base}/expenses/${x.id}`}
              key={String(x.id)}
            >
              <div className="row-main">
                <strong>
                  {String(x.legal_name || "Company member")} ·{" "}
                  {String(x.merchant || "Draft claim")}
                </strong>
                <div className="muted">
                  {String(x.expense_date || "Date required")} ·{" "}
                  {String(x.receipt_reference ?? "Receipt required")} ·{" "}
                  {String(x.business_purpose || "Details required")}
                </div>
              </div>
              <strong>
                {x.amount_minor
                  ? <MoneyAmount units={(BigInt(String(x.amount_minor)) + BigInt(String(x.tax_minor))).toString()} currency={String(x.currency)} date={String(x.expense_date)} legacyScale={2} />
                  : "Amount required"}
              </strong>
              <StatusBadge status={String(x.status)} />
            </Link>
          ))
        )}
      </FeatureWidget>
    </WidgetGrid></FinanceRecordWorkspace></FeaturePage>
  );
}
