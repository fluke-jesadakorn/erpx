import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { Button, Field, Select, TextInput } from "@/components/ui/foundation";
import {
  FeaturePage,
  FeatureTabs,
  FeatureWidget,
  WidgetGrid,
} from "@/components/features/feature-page";
import { ListPagination } from "@/components/ui/list-pagination";
import { PageHeader, StatusBadge } from "@/components/ui/primitives";
import { CountEditor } from "@/components/workplace/work-editors";
import { minorToDecimal } from "@/lib/finance-money";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { canPrepareCounts, countOptions, countsPage } from "@/server/inventory-counts";
import Link from "next/link";
import { pageError } from "@/server/page-error";
import { FilterBar } from "@/components/ui/foundation";
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
  const [page, options] = await Promise.all([
    countsPage(actor, search).catch((error) => pageError(error, holdingSlug)),
    search.new === "1" && canPrepareCounts(actor)
      ? countOptions(actor).catch((error) => pageError(error, holdingSlug))
      : null,
  ]);
  const filtered = page.items;
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <PageHeader appearance="standard"
        eyebrow="Inventory · Finance"
        title="Physical stock counts"
        description="Count actual stock, review differences, and post the quantity and value adjustment together."
        action={
          <UiActionLink appearance="standard" variant="secondary" href={`${base}/commerce?view=stock`}>
            Stock and movements
          </UiActionLink>
        }
      />
      <FeatureTabs appearance="standard" />
      <WidgetGrid pageKey="operations-counts">
        {canPrepareCounts(actor) &&
          (options ? (
            <FeatureWidget appearance="standard" id="operations-counts-stock-count" title="Stock count">
              <CountEditor base={base} options={options} initialOpen />
            </FeatureWidget>
          ) : (
            <UiActionLink appearance="standard" variant="primary" href="?new=1">
              New stock count
            </UiActionLink>
          ))}
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
              <Link className="ui-record-item" key={String(c.id)} href={`${base}/stock-counts/${c.id}`}>
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
    </FeaturePage>
  );
}
