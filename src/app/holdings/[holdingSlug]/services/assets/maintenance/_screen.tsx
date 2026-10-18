import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { forbidden, notFound } from "next/navigation";
import { SectionWorkspace } from "@/components/workplace/section-workspace";
import { OrganizationScopeTree } from "@/components/organization/organization-scope-tree";
import { RecordLink as Link } from "@/components/records/record-link";
import { ListFilters } from "@/components/ui/list-filters";
import { Table, TableFrame, Field, Select } from "@/components/ui/foundation";
import { StatusBadge } from "@/components/ui/primitives";
import { MaintenanceConsole } from "@/components/assets/asset-maintenance-console";
import { MaintenancePreventiveConsole } from "@/components/maintenance/maintenance-preventive-console";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { maintenanceWorkspace } from "@/server/modules/maintenance/public/asset-maintenance";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { hasPermission } from "@/lib/identity_access/permissions";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { sectionWorkspaceTopics, assetTopicsFor, workspaceTopicLinks, workspaceSearchContext } from "@/lib/work_projection/section-workspaces";
import { routeContextHref, type RouteSearch } from "@/lib/shared/route-context";

export default async function Assets({ params, searchParams }: {
    params: Promise<{
        holdingSlug: string;
    }>;
    searchParams: Promise<RouteSearch>;
}) {
    const { holdingSlug } = await params, actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor)), query = await searchParams;
    const canRead = hasPermission(actor, 'records.read'), topics = assetTopicsFor(actor.modules, canRead);
    const view = "maintenance";
    if (!topics.some(topic => topic.id === view)) {
        if (!sectionWorkspaceTopics.assets.some(topic => topic.id === view))
            notFound();
        forbidden();
    }
    
    const workData = await maintenanceWorkspace(actor).catch(error => pageError(error, holdingSlug));
    
    const term = typeof query.q === 'string' ? query.q.toLocaleLowerCase() : '', unit = typeof query.unit === 'string' ? query.unit : null, category = typeof query.category === 'string' ? query.category : null, status = typeof query.status === 'string' ? query.status : null;
    const scopeNodes = (workData?.nodes ?? []).filter(node => (!actor.branchId || node.id === actor.branchId || node.branch_id === actor.branchId) && (!actor.departmentId || node.id === actor.departmentId));
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
    const sourceRows: Record<string, unknown>[] = (workData?.items ?? []);
    const rows = sourceRows.filter(row => (!term || [row.code, row.name, row.title, row.asset_code, row.status, row.lifecycle_status].some(value => String(value ?? '').toLocaleLowerCase().includes(term))) && (!unit || nodeIds.has(String(row.branch_id)) || nodeIds.has(String(row.department_id))) && (!category || row.category === category) && (!status || String(row.lifecycle_status ?? row.status) === status));
    return <SectionWorkspace title="Assets & maintenance" selected={view} items={workspaceTopicLinks(base, query, topics)} context={topics.find(topic => topic.id === view)?.title} navigationExtra={<OrganizationScopeTree nodes={scopeNodes.map(node => ({ id: String(node.id), name: String(node.display_name), parentId: node.kind === 'department' && node.branch_id ? String(node.branch_id) : null }))}/>}>
 <ListFilters appearance="standard" action={canonicalUiHref(`${base}/assets`)} query={typeof query.q === 'string' ? query.q : ''} placeholder="Search this view" preserve={workspaceSearchContext({ ...query, category: undefined, status: undefined }, { view })}>
 {false}
 <Field appearance="standard">Status<Select appearance="standard" name="status" defaultValue={status ?? ''}><option value="">All statuses</option>{[...new Set(sourceRows.map(row => String(row.lifecycle_status ?? row.status)))].map(value => <option key={value} value={value}>{value.replaceAll('_', ' ')}</option>)}</Select></Field></ListFilters>
 {false}{workData && true && <MaintenanceConsole base={base} data={workData}/>}{workData && false && <MaintenancePreventiveConsole base={base} data={workData}/>}
 <TableFrame appearance="standard" label={topics.find(topic => topic.id === view)?.title ?? 'Records'}><Table><thead><tr><th>{'Work / plan'}</th><th>Status</th><th>{'Asset / owner'}</th></tr></thead><tbody>{rows.map(row => <tr key={String(row.id)} data-selected={query.detailId === row.id || undefined}><td><Link href={canonicalUiHref(`${base}/${'maintenance'}/${row.id}`)}>{String(row.code ?? row.asset_code ?? '')} · {String(row.name ?? row.title)}</Link></td><td><StatusBadge appearance="standard" status={String(row.lifecycle_status ?? row.status)}/></td><td>{String(row.owner_name ?? row.asset_code ?? '—')}</td></tr>)}</tbody></Table></TableFrame>{!rows.length && <p>No records match this view in your current scope.</p>}{(unit || category) && <Link href={routeContextHref(`${base}/assets`, query, { unit: null, category: null })}>Clear scope filters</Link>}
 </SectionWorkspace>;
}
