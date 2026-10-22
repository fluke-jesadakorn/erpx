import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { forbidden, notFound } from "next/navigation";
import { SectionWorkspace } from "@erpx/workplace/components/section-workspace";
import { OrganizationScopeTree } from "@erpx/feature-organization/components/organization-scope-tree";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { ListFilters } from "@erpx/ui/components/list-filters";
import { Table, TableFrame, Field, Select } from "@erpx/ui/components/foundation";
import { StatusBadge } from "@erpx/ui/components/primitives";
import { AssetConsole } from "@erpx/feature-assets/components/asset-maintenance-console";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import { assetWorkspace } from "@erpx/server-assets/public/asset-maintenance";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { sectionWorkspaceTopics, assetTopicsFor, workspaceTopicLinks, workspaceSearchContext } from "@erpx/feature-work_projection/lib/section-workspaces";
import { routeContextHref, type RouteSearch } from "@erpx/feature-shared/lib/route-context";

export default async function Assets({ params, searchParams }: {
    params: Promise<{
        holdingSlug: string;
    }>;
    searchParams: Promise<RouteSearch>;
}) {
    const { holdingSlug } = await params, actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor)), query = await searchParams;
    const canRead = hasPermission(actor, 'records.read'), topics = assetTopicsFor(actor.modules, canRead);
    const view = "register";
    if (!topics.some(topic => topic.id === view)) {
        if (!sectionWorkspaceTopics.assets.some(topic => topic.id === view))
            notFound();
        forbidden();
    }
    const assetData = await assetWorkspace(actor).catch(error => pageError(error, holdingSlug));
    
    
    const term = typeof query.q === 'string' ? query.q.toLocaleLowerCase() : '', unit = typeof query.unit === 'string' ? query.unit : null, category = typeof query.category === 'string' ? query.category : null, status = typeof query.status === 'string' ? query.status : null;
    const scopeNodes = (assetData?.nodes ?? []).filter(node => (!actor.branchId || node.id === actor.branchId || node.branch_id === actor.branchId) && (!actor.departmentId || node.id === actor.departmentId));
    const nodeIds = new Set(unit ? [unit] : []);
    let growing = true;
    while (growing) {
        growing = false;
        for (const node of scopeNodes) {
            if (node.branch_id && nodeIds.has(String(node.branch_id)) && !nodeIds.has(String(node.id))) {
                nodeIds.add(String(node.id));
                growing = true;
            }
        }
    }
    const sourceRows: Record<string, unknown>[] = (assetData?.items ?? []);
    const rows = sourceRows.filter(row => (!term || [row.code, row.name, row.title, row.asset_code, row.status, row.lifecycle_status].some(value => String(value ?? '').toLocaleLowerCase().includes(term))) && (!unit || nodeIds.has(String(row.branch_id)) || nodeIds.has(String(row.department_id))) && (!category || row.category === category) && (!status || String(row.lifecycle_status ?? row.status) === status));
    return <SectionWorkspace title="Assets & maintenance" selected={view} items={workspaceTopicLinks(base, query, topics)} context={topics.find(topic => topic.id === view)?.title} navigationExtra={<OrganizationScopeTree nodes={scopeNodes.map(node => ({ id: String(node.id), name: String(node.display_name), parentId: node.kind === 'department' && node.branch_id ? String(node.branch_id) : null }))}/>}>
 <ListFilters appearance="standard" action={canonicalUiHref(`${base}/assets`)} query={typeof query.q === 'string' ? query.q : ''} placeholder="Search this view" preserve={workspaceSearchContext({ ...query, category: undefined, status: undefined }, { view })}>
 {assetData && <Field appearance="standard">Category<Select appearance="standard" name="category" defaultValue={category ?? ''}><option value="">All categories</option>{[...new Set(assetData.items.map(row => String(row.category)))].map(value => <option key={value} value={value}>{value}</option>)}</Select></Field>}
 <Field appearance="standard">Status<Select appearance="standard" name="status" defaultValue={status ?? ''}><option value="">All statuses</option>{[...new Set(sourceRows.map(row => String(row.lifecycle_status ?? row.status)))].map(value => <option key={value} value={value}>{value.replaceAll('_', ' ')}</option>)}</Select></Field></ListFilters>
 {assetData && <AssetConsole base={base} data={assetData}/>}{false}{false}
 <TableFrame appearance="standard" label={topics.find(topic => topic.id === view)?.title ?? 'Records'}><Table><thead><tr><th>{'Asset'}</th><th>Status</th><th>{'Condition'}</th></tr></thead><tbody>{rows.map(row => <tr key={String(row.id)} data-selected={query.detailId === row.id || undefined}><td><Link href={canonicalUiHref(`${base}/${'assets'}/${row.id}`)}>{String(row.code ?? row.asset_code ?? '')} · {String(row.name ?? row.title)}</Link></td><td><StatusBadge appearance="standard" status={String(row.lifecycle_status ?? row.status)}/></td><td>{String(row.condition)}</td></tr>)}</tbody></Table></TableFrame>{!rows.length && <p>No records match this view in your current scope.</p>}{(unit || category) && <Link href={routeContextHref(`${base}/assets`, query, { unit: null, category: null })}>Clear scope filters</Link>}
 </SectionWorkspace>;
}
