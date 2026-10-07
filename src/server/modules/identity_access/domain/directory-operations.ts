import { z } from 'zod';
import type { ConnectionRow } from './directories';
import type { DirectoryAction, DirectoryInventory, DirectoryChangeInput, DirectoryPreview } from '@/lib/directory-operations';
export type State = {
    connection_id: string;
    company_id: string;
    version: number;
    tested_version: number | null;
    write_enabled: boolean;
    inventory: DirectoryInventory | null;
    definition: Record<string, unknown>;
};
export const connectorMappingInput = z.object({
    attributeId: z.string().min(1),
    field: z.string().min(1),
    direction: z.enum(["read", "write"]),
});
export function canonical(v: unknown): unknown {
    return Array.isArray(v)
        ? v.map(canonical)
        : v && typeof v === "object"
            ? Object.fromEntries(Object.entries(v)
                .sort(([a], [b]) => a.localeCompare(b))
                .map(([k, x]) => [k, canonical(x)]))
            : v;
}
export function affectedObjects(action: DirectoryAction, inventory: DirectoryInventory) {
    const ids = new Set<string>();
    if ("objectId" in action && action.objectId)
        ids.add(action.objectId);
    if ("parentId" in action)
        ids.add(action.parentId);
    if (action.kind === "group.members")
        action.memberIds.forEach((id) => ids.add(id));
    if (action.kind === "gpo.publish") {
        if (!action.objectId) {
            ids.add(action.definition.targetOuId);
            ids.add(action.definition.filterGroupId);
        }
        if (action.definition.computerTargetId)
            ids.add(action.definition.computerTargetId);
    }
    if (action.kind === "gpo.link" && action.filterGroupId)
        ids.add(action.filterGroupId);
    if (action.kind === "gpo.filter")
        action.groupIds.forEach((id) => ids.add(id));
    if (action.kind === "gpo.order")
        action.gpoIds.forEach((id) => ids.add(id));
    for (let changed = true; changed;) {
        changed = false;
        for (const o of inventory.objects)
            if (ids.has(o.id)) {
                for (const id of [
                    ...o.memberIds,
                    ...(o.kind === "gpo" &&
                        Array.isArray(o.properties.securityFilterGroupIds)
                        ? (o.properties.securityFilterGroupIds as string[])
                        : []),
                ])
                    if (!ids.has(id)) {
                        ids.add(id);
                        changed = true;
                    }
            }
            else if ((o.parentId && ids.has(o.parentId)) ||
                (o.kind === "group" && o.memberIds.some((id) => ids.has(id)))) {
                ids.add(o.id);
                changed = true;
            }
    }
    return inventory.objects.filter((o) => ids.has(o.id));
}
export type ConnectorMapping = z.infer<typeof connectorMappingInput>;
export type AutomationConfiguration = {
    connectionId: string;
    expectedVersion: number;
    writeEnabled: boolean;
    mappings?: ConnectorMapping[];
};
export type AutomationConnectionFact = ConnectionRow & {
    state_version: number | null;
    automation_tested: number | null;
    write_enabled: boolean | null;
    inventory: DirectoryInventory | null;
    definition: {
        mappings?: ConnectorMapping[];
    } | null;
};
export type AutomationCaseFact = {
    id: string;
    name: string;
    kind: string;
    effective_at: number;
    account_ids: string[];
};
export type IdentityLinkFact = {
    id: string;
    external_id: string;
    user_id: string | null;
    person_id: string | null;
    company_id: string;
    config: Record<string, unknown>;
};
export type LifecycleFact = {
    id: string;
    person_id: string;
    active: boolean;
    status: string;
    kind: string;
    effective_at: number;
};
export type PositionFact = {
    id: string;
    company_id: string;
    title: string;
};
export type OperationEvidence = {
    connectionVersion: number;
    stateVersion: number;
    inventory: DirectoryInventory;
    objects: string[];
    companies: string[];
    identityLinks: [
        string,
        string | null,
        string
    ][];
};
export type PreviewPreparation = {
    result: DirectoryPreview;
    evidence: OperationEvidence;
    state: State;
    row: ConnectionRow;
};
export type OperationStep = {
    index: number;
    status: string;
    result: Record<string, unknown>;
};
export type DirectoryOperationJob = {
    id: string;
    company_id: string;
    connection_id: string;
    connection_version: number;
    state_version: number;
    definition: DirectoryChangeInput;
    evidence: OperationEvidence;
    provision_id: string | null;
    created_by: string;
    assignment_id: string;
    approval_mode: string;
    mfa_session_id: string;
    review_id: string | null;
    steps: OperationStep[] | null;
};
export type ProvisionPlan = {
    id: string;
    engagementVersion: number;
    oldIds: string[];
    definition: {
        connectionId: string;
        directoryGroupIds: string[];
    };
    directoryMembers: string[];
};
export type ProvisionEvidence = {
    plans: ProvisionPlan[];
};
export type ProvisionOperation = {
    status: string;
    evidence: ProvisionEvidence;
    definition: {
        restoreAccess?: boolean;
        revokeOnly?: boolean;
    };
};
export type MembershipSource = {
    member_id: string;
    application_id: string | null;
    package_owned: boolean;
    gpo_owned: boolean;
};
export type DueOperation = {
    id: string;
    connection_id: string;
};
export type GroupMembershipAction = Extract<DirectoryAction, {
    kind: 'group.members';
}>;
export type LifecycleAction = Extract<DirectoryAction, {
    kind: 'account.enable' | 'account.disable';
}>;
