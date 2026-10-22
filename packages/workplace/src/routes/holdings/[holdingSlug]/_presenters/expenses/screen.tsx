import {peopleFinancialSourcePresentation} from "@erpx/server-people_payroll/public/financial-product-presentation-request";
import {ExpenseNewWork} from "@erpx/feature-people_payroll/components/expense-purchase-context";
import {ExpenseEnvironment} from "@erpx/feature-people_payroll/components/expense-environment";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";
import { Button, Field, Select, TextInput } from "@erpx/ui/components/foundation";
import {
  FinanceRecordWorkspace,
  FinanceLegacyHeading,
} from "@erpx/feature-finance/components/finance-record-workspace";
import {
  FeaturePage,
  FeatureTabs,
  FeatureWidget,
  WidgetGrid,
} from "@erpx/feature-shared/components/features/feature-page";
import { EmptyState, PageHeader, StatusBadge } from "@erpx/ui/components/primitives";
import { MoneyAmount } from "@erpx/feature-finance/components/money/currency-provider";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { listExpenses } from "@erpx/server-people_payroll/public/expenses";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { FilterBar } from "@erpx/ui/components/foundation";
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
  const [expenses, search, financialSource] = await Promise.all([listExpenses(actor), searchParams, peopleFinancialSourcePresentation(actor,"expense")]);
  const filtered = expenses.filter(
    (x) =>
      (!search.status || x.status === search.status) &&
      (!search.q ||
        `${x.legal_name} ${x.merchant} ${x.receipt_reference} ${x.business_purpose}`
          .toLowerCase()
          .includes(search.q.toLowerCase())),
  );
  return (
    <ExpenseEnvironment actor={actor}><FeaturePage className="ui-page-content" data-source="live">
      <FinanceRecordWorkspace
        base={base}
        active="finance-expenses"
        title="Employee expenses"
        description="Review receipts, accounting and reimbursement evidence."
        actions={
          !financialSource&&<ExpenseNewWork><UiActionLink appearance="standard" variant="primary" href={canonicalUiHref(`${base}/expenses/new`)}>
            New expense claim
          </UiActionLink></ExpenseNewWork>
        }
      >
        <FinanceLegacyHeading>
          <PageHeader appearance="standard"
            eyebrow="People · Finance"
            title="Employee expenses"
            description="Claim business costs, review receipts, post the obligation, and track reimbursement."
          />
        </FinanceLegacyHeading>
        <FeatureTabs appearance="standard" />
        <WidgetGrid pageKey="finance-expenses">
          <FinanceLegacyHeading>
            {!financialSource&&<ExpenseNewWork><UiActionLink appearance="standard" variant="primary" href={canonicalUiHref(`${base}/expenses/new`)}>
              New expense claim
            </UiActionLink></ExpenseNewWork>}
          </FinanceLegacyHeading>
          <FilterBar className="ui-filter-bar" action={canonicalUiHref(`${base}/expenses`)}>
            <Field appearance="standard">
              Search claims
              <TextInput appearance="standard"
                name="q"
                defaultValue={search.q}
                placeholder="Employee, merchant, receipt or purpose"
              />
            </Field>
            <Field appearance="standard">
              Claim status
              <Select appearance="standard" name="status" defaultValue={search.status ?? ""}>
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
              </Select>
            </Field>
            <Button appearance="standard" type="submit" variant="secondary">
              Filter claims
            </Button>
          </FilterBar>
          <FeatureWidget appearance="standard" id="finance-expenses-expense-claims" title="Expense claims">
            <div className="ui-section-header">
              <h2>Expense claims</h2>
              <span className="ui-badge">{filtered.length}</span>
            </div>
            {filtered.length === 0 ? (
              <EmptyState
                title="No matching claims"
                description="Create a claim or change the filters to view expenses in your scope."
              />
            ) : (
              filtered.map((x) => (
                <Link className="ui-record-item" href={canonicalUiHref(`${base}/expenses/${x.id}`)} key={String(x.id)}>
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
                    {x.amount_minor ? (
                      <MoneyAmount
                        units={(
                          BigInt(String(x.amount_minor)) + BigInt(String(x.tax_minor))
                        ).toString()}
                        currency={String(x.currency)}
                        date={String(x.expense_date)}
                        legacyScale={2}
                      />
                    ) : (
                      "Amount required"
                    )}
                  </strong>
                  <StatusBadge appearance="standard" status={String(x.status)} />
                </Link>
              ))
            )}
          </FeatureWidget>
        </WidgetGrid>
      </FinanceRecordWorkspace>
    </FeaturePage></ExpenseEnvironment>
  );
}
