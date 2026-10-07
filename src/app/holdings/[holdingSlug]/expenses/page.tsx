import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { Button, Field, Select, TextInput } from "@/components/ui/foundation";
import {
  FinanceRecordWorkspace,
  FinanceLegacyHeading,
} from "@/components/finance/finance-record-workspace";
import {
  FeaturePage,
  FeatureTabs,
  FeatureWidget,
  WidgetGrid,
} from "@/components/features/feature-page";
import { EmptyState, PageHeader, StatusBadge } from "@/components/ui/primitives";
import { MoneyAmount } from "@/components/money/currency-provider";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { listExpenses } from "@/server/modules/people_payroll/public/expenses";
import { RecordLink as Link } from "@/components/records/record-link";
import { FilterBar } from "@/components/ui/foundation";
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
  const [expenses, search] = await Promise.all([listExpenses(actor), searchParams]);
  const filtered = expenses.filter(
    (x) =>
      (!search.status || x.status === search.status) &&
      (!search.q ||
        `${x.legal_name} ${x.merchant} ${x.receipt_reference} ${x.business_purpose}`
          .toLowerCase()
          .includes(search.q.toLowerCase())),
  );
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <FinanceRecordWorkspace
        base={base}
        active="receivables"
        title="Employee expenses"
        description="Review receipts, accounting and reimbursement evidence."
        actions={
          <UiActionLink appearance="standard" variant="primary" href={`${base}/expenses/new`}>
            New expense claim
          </UiActionLink>
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
            <UiActionLink appearance="standard" variant="primary" href={`${base}/expenses/new`}>
              New expense claim
            </UiActionLink>
          </FinanceLegacyHeading>
          <FilterBar className="ui-filter-bar" action={`${base}/expenses`}>
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
                <Link className="ui-record-item" href={`${base}/expenses/${x.id}`} key={String(x.id)}>
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
    </FeaturePage>
  );
}
