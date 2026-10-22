import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { descendants, emptyDirectoryHierarchy, type HierarchyNode, type DirectoryHierarchy } from "@erpx/feature-organization/lib/hierarchy";
import { BusinessRuleError, type Clock } from '@erpx/server-shared/index';
import type { IamActor } from '@erpx/server-identity_access/public/iam-context';
import type { iamOverview } from '@erpx/server-identity_access/public/iam-service';
import type { AccessEntry } from '@erpx/server-people_payroll/domain/people-access';
export interface PeopleAccessPorts {
    clock: Clock;
    authorize(actor: IamActor, permission: string): unknown;
    overview(actor: IamActor): ReturnType<typeof iamOverview>;
    nodes(companyId: string): Promise<Record<string, unknown>[]>;
    people(companyId: string): Promise<Record<string, unknown>[]>;
    identities(companyId: string): Promise<Record<string, unknown>[]>;
    hierarchies(companyId: string): Promise<{
        connection_id: string;
        hierarchy_json: DirectoryHierarchy;
    }[]>;
    encode(value: string): string;
    decode(value: string): string;
}
export type PeopleAccessData = Awaited<ReturnType<ReturnType<typeof createPeopleAccessApplication>['peopleAccessData']>>;
export function createPeopleAccessApplication(ports: PeopleAccessPorts) {
    const key = (kind: string, ...ids: string[]) => `${kind}:${ids.map(encodeURIComponent).join(":")}`;
    const text = (v: unknown) => (v == null ? null : String(v));
    const missing = () => new BusinessRuleError(404, "ACCESS_OBJECT_NOT_FOUND", "This object is unavailable in your selected work context.");
    /** Every endpoint builds its visibility boundary from the selected work context, never client scope input. */
    async function peopleAccessData(actor: IamActor) {
        ports.authorize(actor, "iam.read");
        const fullDirectory = actor.scopeKind === "holding" &&
            hasPermission(actor, "iam.directory.manage");
        const overview = await ports.overview(actor);
        const org = await ports.nodes(actor.companyId);
        const allNodes: HierarchyNode[] = org.map((n) => ({
            id: String(n.id),
            parentId: text(n.parent_id),
            kind: String(n.kind),
            label: String(n.display_name),
            source: "ERPX",
            status: String(n.status),
            childCount: 0,
        }));
        const visible = actor.scopeKind === "holding"
            ? new Set(allNodes.map((n) => n.id))
            : descendants(allNodes, actor.scopeNodeId ?? "");
        const tree: HierarchyNode[] = allNodes
            .filter((n) => visible.has(n.id))
            .map((n) => ({
            ...n,
            parentId: n.parentId && visible.has(n.parentId) ? n.parentId : null,
        }));
        if (!tree.length)
            throw missing();
        const root = tree.find((n) => !n.parentId)!.id;
        const people = (await ports.people(actor.companyId)).filter((p) => actor.scopeKind === "holding" ||
            visible.has(String(p.department_id)) ||
            visible.has(String(p.branch_id)));
        const entries: AccessEntry[] = [];
        for (const p of people) {
            const old = entries.find((e) => e.personId === p.id);
            const nodes = [text(p.department_id), text(p.branch_id)].filter((n): n is string => !!n && visible.has(n));
            if (old) {
                old.nodes = [...new Set([...old.nodes, ...nodes])];
                continue;
            }
            const account = overview.users.find((u) => u.id === p.user_id);
            entries.push({
                id: key("person", String(p.id)),
                kind: "person",
                label: String(p.legal_name),
                email: account?.email ?? text(p.email),
                userId: text(p.user_id),
                personId: String(p.id),
                connectionId: null,
                externalId: null,
                source: "ERPX",
                status: String(p.status),
                account: p.user_id ? "ERPX account" : "No ERPX account",
                sync: "Local record",
                nodes: nodes.length ? nodes : [root],
            });
        }
        // Include workspace accounts still awaiting an employee profile (including the owner).
        for (const u of overview.users)
            if (!entries.some((e) => e.userId === u.id))
                entries.push({
                    id: key("account", u.id),
                    kind: "person",
                    label: u.name,
                    email: u.email,
                    userId: u.id,
                    personId: null,
                    connectionId: null,
                    externalId: null,
                    source: "ERPX",
                    status: "Profile not linked",
                    account: "ERPX account",
                    sync: "Local record",
                    nodes: [root],
                });
        const visibleUsers = new Set(entries.flatMap((e) => (e.userId ? [e.userId] : []))), visiblePeople = new Set(entries.flatMap((e) => (e.personId ? [e.personId] : [])));
        const identities = (await ports.identities(actor.companyId)).filter((i) => fullDirectory ||
            visibleUsers.has(String(i.user_id)) ||
            visiblePeople.has(String(i.person_id)));
        const snapshots = fullDirectory ? await ports.hierarchies(actor.companyId) : [];
        const groups = fullDirectory ? overview.groups.filter((g) => !g.preview) : [];
        for (const c of fullDirectory ? overview.connections : []) {
            const connectionId = String(c.id), connectionRoot = key("connection", connectionId), h = snapshots.find((h) => h.connection_id === c.id)?.hierarchy_json ??
                emptyDirectoryHierarchy();
            const status = c.status !== "active"
                ? String(c.status)
                : c.health !== "healthy" ||
                    Number(c.last_sync_at) <= ports.clock.now() - 300000
                    ? "stale"
                    : "synced";
            tree.push({
                id: connectionRoot,
                parentId: null,
                kind: "connection",
                source: String(c.provider),
                label: `${c.name} · connection root`,
                status,
                childCount: 0,
            });
            for (const n of h.containers)
                tree.push({
                    id: key("container", connectionId, n.externalId),
                    parentId: n.parentId
                        ? key("container", connectionId, n.parentId)
                        : connectionRoot,
                    kind: n.kind,
                    source: String(c.provider),
                    label: n.name,
                    status,
                    childCount: 0,
                });
            for (const g of groups.filter((g) => g.connection_id === c.id)) {
                const externalId = String(g.external_id), id = key("group", connectionId, externalId), container = h.placements.find((p) => p.kind === "group" && p.externalId === externalId)?.containerId;
                tree.push({
                    id,
                    parentId: container
                        ? key("container", connectionId, container)
                        : connectionRoot,
                    kind: "group",
                    source: String(c.provider),
                    label: String(g.name),
                    status,
                    childCount: 0,
                });
                entries.push({
                    id,
                    kind: "group",
                    label: String(g.name),
                    email: null,
                    userId: null,
                    personId: null,
                    connectionId,
                    externalId,
                    source: String(c.provider),
                    status: "Read-only directory group",
                    account: "Scoped ERPX mappings",
                    sync: status,
                    nodes: [id],
                });
            }
            // Alias leaves represent direct membership without pretending a group's OU is its parent group.
            for (const edge of h.groupEdges) {
                const child = groups.find((g) => g.connection_id === c.id && g.external_id === edge.childId);
                if (child)
                    tree.push({
                        id: key("membership", connectionId, edge.parentId, edge.childId),
                        parentId: key("group", connectionId, edge.parentId),
                        kind: "nested-group",
                        source: String(c.provider),
                        label: String(child.name),
                        status,
                        childCount: 0,
                    });
            }
        }
        for (const i of identities) {
            const person = entries.find((e) => e.kind === "person" &&
                ((i.user_id && e.userId === i.user_id) ||
                    (i.person_id && e.personId === i.person_id)));
            const h = snapshots.find((h) => h.connection_id === i.connection_id)?.hierarchy_json;
            const container = h?.placements.find((p) => p.kind === "identity" && p.externalId === i.external_id)?.containerId;
            const nodes = fullDirectory
                ? [
                    container
                        ? key("container", String(i.connection_id), container)
                        : key("connection", String(i.connection_id)),
                    ...(i.groups_json as string[]).map((g) => key("group", String(i.connection_id), g)),
                ]
                : (person?.nodes ?? []);
            const sync = i.connection_status !== "active"
                ? "Connection disabled"
                : i.health !== "healthy" || Number(i.observed_at) <= ports.clock.now() - 300000
                    ? "Stale evidence"
                    : "Synced";
            if (person) {
                person.source = `ERPX · ${i.provider}`;
                person.sync = sync;
            }
            entries.push({
                id: key("identity", String(i.id)),
                kind: "identity",
                label: String(i.name),
                email: text(i.email),
                userId: text(i.user_id),
                personId: text(i.person_id),
                connectionId: String(i.connection_id),
                externalId: String(i.external_id),
                source: String(i.provider),
                status: i.enabled ? "Enabled" : "Disabled",
                account: i.user_id ? "Linked ERPX account" : "No ERPX account",
                sync,
                nodes,
            });
        }
        for (const n of tree)
            n.childCount = tree.filter((c) => c.parentId === n.id).length;
        const assignments = overview.assignments.filter((a) => actor.scopeKind === "holding" || visible.has(String(a.scope_node_id)));
        for (const e of entries)
            if (e.kind !== "group") {
                const grants = assignments.filter((a) => a.user_id === e.userId);
                e.accessStatus = grants.some((a) => a.identity_status === "suspended")
                    ? "ERPX access suspended"
                    : grants.some((a) => a.active)
                        ? "Active ERPX access"
                        : "No active ERPX access";
            }
        return {
            tree,
            entries,
            assignments,
            overview,
            identities,
            snapshots,
            fullDirectory,
            root,
            actor,
        };
    }
    function accessEntries(data: PeopleAccessData, params: URLSearchParams) {
        const node = params.get("node") ?? data.root, selected = data.tree.find((n) => n.id === node);
        if (!selected)
            throw missing();
        const subtree = descendants(data.tree, node), q = (params.get("q") ?? "").slice(0, 200).toLowerCase();
        // A nested-group alias selects the actual child group, preserving effective member browsing.
        if (selected.kind === "nested-group") {
            const bits = node.split(":");
            subtree.add(key("group", decodeURIComponent(bits[1]), decodeURIComponent(bits[3])));
        }
        const rows = data.entries
            .filter((e) => e.nodes.some((n) => subtree.has(n)) &&
            `${e.label} ${e.email ?? ""}`.toLowerCase().includes(q))
            .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
        let after = "";
        if (params.get("cursor")) {
            try {
                const cursor = JSON.parse(ports.decode(params.get("cursor")!));
                if (cursor.node !== node ||
                    cursor.q !== q ||
                    typeof cursor.id !== "string")
                    throw Error();
                after = cursor.id;
            }
            catch {
                throw new BusinessRuleError(422, "INVALID_CURSOR", "Restart this search to load results.");
            }
        }
        const remaining = rows.filter((e) => e.id > after), items = remaining.slice(0, 30);
        return {
            items,
            total: rows.length,
            nextCursor: remaining.length > 30
                ? ports.encode(JSON.stringify({ node, q, id: items.at(-1)!.id }))
                : null,
        };
    }
    function accessDetail(data: PeopleAccessData, id: string) {
        const entry = data.entries.find((e) => e.id === id);
        if (!entry)
            throw missing();
        const assignments = entry.userId
            ? data.assignments.filter((a) => a.user_id === entry.userId)
            : [];
        const decisions = data.overview.decisions.filter((d) => assignments.some((a) => a.id === d.assignmentId));
        const linked = data.entries.filter((e) => e.kind === "identity" &&
            ((entry.userId && e.userId === entry.userId) ||
                (entry.personId && e.personId === entry.personId)));
        const groups = entry.connectionId && entry.externalId
            ? (data.snapshots
                .find((s) => s.connection_id === entry.connectionId)
                ?.hierarchy_json.groupEdges.filter((e) => e.parentId === entry.externalId || e.childId === entry.externalId) ?? [])
            : [];
        const memberships = data.identities
            .filter((i) => linked.some((l) => l.id === key("identity", String(i.id))) ||
            entry.id === key("identity", String(i.id)))
            .flatMap((i) => (i.groups_json as string[]).map((g) => ({
            connectionId: String(i.connection_id),
            groupId: g,
            label: data.fullDirectory
                ? String(data.overview.groups.find((r) => r.connection_id === i.connection_id && r.external_id === g)?.name ?? g)
                : "Linked directory membership",
        })));
        return {
            entry,
            assignments,
            decisions,
            linked,
            memberships,
            groups,
            mappings: entry.kind === "group"
                ? data.overview.mappings.filter((m) => m.connection_id === entry.connectionId &&
                    m.group_id === entry.externalId)
                : [],
            catalog: data.overview.catalog,
        };
    }
    function accessComparison(data: PeopleAccessData, ids: string[]) {
        if (!ids.length || ids.length > 4 || new Set(ids).size !== ids.length)
            throw new BusinessRuleError(422, "COMPARISON_LIMIT", "Choose one assignment for each of up to four people.");
        const selected = ids.map((id) => {
            const a = data.assignments.find((a) => a.id === id);
            if (!a)
                throw missing();
            return a;
        });
        const people = selected.map((a) => a.person_id ??
            data.entries.find((e) => e.kind === "person" && e.userId === a.user_id)
                ?.personId ??
            String(a.user_id));
        if (new Set(people).size !== people.length)
            throw new BusinessRuleError(422, "COMPARISON_CONTEXT", "Choose only one work assignment per person.");
        return {
            assignments: selected,
            decisions: data.overview.decisions.filter((d) => ids.includes(d.assignmentId)),
            catalog: data.overview.catalog,
        };
    }
    function accessMetadata(data: PeopleAccessData) {
        return {
            tree: data.tree,
            root: data.root,
            fullDirectory: data.fullDirectory,
            canManage: hasPermission(data.actor, "iam.manage"),
            currentUserId: data.actor.userId,
            roles: data.overview.roles.filter((r) => r.permissions.every((p) => data.overview.grantablePermissions.includes(p)) &&
                r.grantablePermissions.every((p) => data.overview.grantablePermissions.includes(p))),
            scopeKind: data.actor.scopeKind,
            scopeNodeId: data.actor.scopeNodeId,
        };
    }
    return { peopleAccessData, accessEntries, accessDetail, accessComparison, accessMetadata };
}
