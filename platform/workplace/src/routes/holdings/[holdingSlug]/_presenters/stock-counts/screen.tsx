import {countPurchaseControls} from "@erpx/inventory-server/public/count-purchases";
import {CountEnvironment} from "@erpx/inventory-ui/components/count-purchase-context";
import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { ActionLink as UiActionLink } from "@erpx/platform-ui/components/foundation";
import { Button, Field, Select, TextInput } from "@erpx/platform-ui/components/foundation";
import {
  FeaturePage,
  FeatureTabs,
  FeatureWidget,
  WidgetGrid,
} from "@erpx/platform-shared-ui/components/features/feature-page";
import { ListPagination } from "@erpx/platform-ui/components/list-pagination";
import { PageHeader, StatusBadge } from "@erpx/platform-ui/components/primitives";
import { CountEditor } from "@erpx/inventory-ui/components/lazy-count-editor";
import { minorToDecimal } from "@erpx/finance-ui/lib/finance-money";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { canPrepareCounts, countOptions, countsPage } from "@erpx/inventory-server/public/inventory-counts";
import { RecordLink as Link } from "@erpx/records-ui/components/record-link";
import { pageError } from "@erpx/work-projection-server/public/page-error";
import { FilterBar } from "@erpx/platform-ui/components/foundation";
export const dynamic = "force-dynamic";
export default async function Counts({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ q?: string; status?: string; cursor?: string; new?: string }>;
}) {
  const { holdingSlug } = await params,
    actor = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(actor));
  const search = await searchParams;
  const [page, options, purchase] = await Promise.all([
    countsPage(actor, search).catch((error) => pageError(error, holdingSlug)),
    search.new === "1" && canPrepareCounts(actor)
      ? countOptions(actor).catch((error) => pageError(error, holdingSlug))
      : null,
    countPurchaseControls(actor),
  ]);
  const filtered = page.items;
  return (
    <CountEnvironment purchase={purchase}><FeaturePage className="ui-page-content" data-source="live">
      <PageHeader appearance="standard"
        eyebrow="Inventory · Finance"
        title="Physical stock counts"
        description="Count actual stock, review differences, and post the quantity and value adjustment together."
        action={
          <UiActionLink appearance="standard" variant="secondary" href={canonicalUiHref(`${base}/commerce?view=stock`)}>
            Stock and movements
          </UiActionLink>
        }
      />
      <FeatureTabs appearance="standard" />
      <WidgetGrid pageKey="operations-counts">
        {canPrepareCounts(actor) &&
          (options ? (
            <FeatureWidget appearance="standard" id="operations-counts-stock-count" title="Stock count">
              <CountEditor drawer base={base} options={options} initialOpen />
            </FeatureWidget>
          ) : purchase?.newWork !== false ? (
            <UiActionLink appearance="standard" variant="primary" href="?new=1">
              New stock count
            </UiActionLink>
          ) : null)}
        <FilterBar className="ui-filter-bar">
          <Field appearance="standard">
            Search stock counts
            <TextInput appearance="standard" name="q" defaultValue={search.q} placeholder="Count, warehouse or reason" />
          </Field>
          <Field appearance="standard">
            Count status
            <Select appearance="standard" name="status" defaultValue={search.status ?? ""}>
              <option value="">All statuses</option>
              {["draft", "submitted", "rejected", "posted", "cancelled"].map((s) => (
                <option key={s} value={s}>
                  {s}
                </option>
              ))}
            </Select>
          </Field>
          <Button appearance="standard" type="submit" variant="secondary">
            Filter counts
          </Button>
        </FilterBar>
        <FeatureWidget appearance="standard" id="operations-counts-count-register" title="Count register">
          <div className="ui-section-header">
            <h2>Count register</h2>
            <span className="ui-badge">{filtered.length}</span>
          </div>
          {!filtered.length ? (
            <p className="muted">No physical counts match this view.</p>
          ) : (
            filtered.map((c) => (
              <Link className="ui-record-item" key={String(c.id)} href={canonicalUiHref(`${base}/stock-counts/${c.id}`)}>
                <div className="row-main">
                  <strong>{String(c.number)}</strong>
                  <div className="muted">
                    {String(c.warehouse_name)} · {String(c.count_date)}
                    <br />
                    {String(c.reason)}
                  </div>
                </div>
                <strong>
                  {String(c.currency)} {minorToDecimal(Number(c.net_value_minor))}
                </strong>
                <StatusBadge appearance="standard" status={String(c.status)} />
              </Link>
            ))
          )}
        </FeatureWidget>
        <ListPagination {...page} query={search} count={filtered.length} />
      </WidgetGrid>
    </FeaturePage></CountEnvironment>
  );
}
