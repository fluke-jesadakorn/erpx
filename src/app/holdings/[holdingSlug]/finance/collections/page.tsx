import { FeaturePage } from "@/components/features/feature-page";
import { FinanceWorkspace } from "@/components/finance/finance-workspace";
import { FinanceCollectionControls } from "@/components/finance/finance-collection-controls";
import {
  FormLayout,
  Field,
  Select,
  TextInput,
  Button,
} from "@/components/ui/foundation";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { hasPermission } from "@/lib/permissions";
import { ledgerWorkspace } from "@/server/ledger";
import {
  collectionWorkspace,
  cashDueForecast,
} from "@/server/finance-collections";
import { financeCompanyTools } from "@/server/finance-company-tools";
export const dynamic = "force-dynamic";
export default async function CollectionsPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ chart?: string; asOf?: string; invoiceCursor?: string; caseCursor?: string; q?: string }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor)),
    query = await searchParams,
    ledger = await ledgerWorkspace(actor, query.chart),
    asOf = query.asOf ?? new Date().toISOString().slice(0, 10),
    [data, forecast, tools] = await Promise.all([
      collectionWorkspace(actor, asOf, query),
      cashDueForecast(actor, { chartId: String(ledger.chart.id), asOf }),
      financeCompanyTools(actor),
    ]),
    enabled = tools.some((r) => r.tool === "collections" && r.enabled === 1);
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <FinanceWorkspace
        base={base}
        active="cash"
        title="Collections and cash forecast"
        description="Review overdue obligations and cash due dates"
        context={{
          chartId: String(ledger.chart.id),
          chartName: String(ledger.chart.name),
          currency: String(ledger.chart.base_currency),
        }}
      >
        <FormLayout className="ui-form-grid">
          <Field appearance="standard">
            Book
            <Select
              appearance="standard"
              name="chart"
              defaultValue={String(ledger.chart.id)}
            >
              {ledger.charts.map((c) => (
                <option key={String(c.id)} value={String(c.id)}>
                  {String(c.name)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">Customer, invoice or message<TextInput appearance="standard" name="q" defaultValue={query.q ?? ""}/></Field>
          <Field appearance="standard">
            As of
            <TextInput
              appearance="standard"
              name="asOf"
              type="date"
              defaultValue={asOf}
            />
          </Field>
          <Button appearance="standard" type="submit">
            Show forecast
          </Button>
        </FormLayout>
        <FinanceCollectionControls
          base={base}
          data={data}
          query={query}
          forecast={forecast}
          canWrite={hasPermission(actor, "finance.write") && enabled}
          canApprove={hasPermission(actor, "finance.approve")}
          userId={actor.userId}
        />
      </FinanceWorkspace>
    </FeaturePage>
  );
}
