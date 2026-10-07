import { z } from 'zod';
import type { DirectoryAction, DirectoryInventory, DirectoryObject } from '@/lib/directory-operations';
import type { DirectorySecret as Secret } from './providers';
type Json = Record<string, unknown>;
const inventorySchema = z.object({
    objects: z
        .array(z.object({
        id: z.string().min(1),
        dn: z.string().min(1),
        name: z.string(),
        kind: z.enum(["account", "group", "ou", "computer", "gpo"]),
        parentId: z.string().nullable(),
        version: z.string().min(1),
        module: z.string(),
        properties: z.record(z.string(), z.unknown()),
        memberIds: z.array(z.string()),
    }))
        .max(10000),
    fields: z.array(z.object({
        name: z.string(),
        type: z.enum(["text", "checkbox"]),
        read: z.boolean(),
        write: z.boolean(),
    })),
    providerVersion: z.string().min(1),
    observedAt: z.number().finite(),
    complete: z.literal(true),
});
export function createDirectoryPolicy(error: (status: number, code: string, message: string) => Error) {
    function providerFailure(code: string, message: string, status = 409): never { throw error(status, code, message); }
    function parsedInventory(value: unknown): DirectoryInventory {
        const parsed = inventorySchema.safeParse(value);
        if (!parsed.success ||
            new Set(parsed.data.objects.map((o) => o.id)).size !==
                parsed.data.objects.length)
            providerFailure("DIRECTORY_INCOMPLETE", "The provider returned an invalid inventory.", 502);
        return parsed.data;
    }
    function canonicalInventory(inventory: DirectoryInventory, aliases: Record<string, string>) {
        const canonical = (id: string | null) => id === null ? null : (aliases[id.toLowerCase()] ?? id);
        const remapPolicy = (value: unknown) => {
            const policy = object(value);
            return {
                ...policy,
                targetOuId: typeof policy.targetOuId === "string"
                    ? canonical(policy.targetOuId)
                    : policy.targetOuId,
                filterGroupId: typeof policy.filterGroupId === "string"
                    ? canonical(policy.filterGroupId)
                    : policy.filterGroupId,
                computerTargetId: typeof policy.computerTargetId === "string"
                    ? canonical(policy.computerTargetId)
                    : policy.computerTargetId,
            };
        };
        const objects = inventory.objects.map((item) => {
            const providerId = item.id, id = canonical(providerId)!;
            return {
                ...item,
                id,
                parentId: canonical(item.parentId),
                memberIds: item.memberIds.map((member) => canonical(member)!),
                properties: {
                    ...item.properties,
                    ...(id !== providerId
                        ? {
                            windowsIdentity: {
                                id: providerId,
                                dn: item.dn,
                                version: item.version,
                            },
                        }
                        : {}),
                    ...(Array.isArray(item.properties.gpoLinks)
                        ? {
                            gpoLinks: item.properties.gpoLinks.map((link) => {
                                const value = object(link);
                                return {
                                    ...value,
                                    gpoId: typeof value.gpoId === "string"
                                        ? canonical(value.gpoId)
                                        : value.gpoId,
                                };
                            }),
                        }
                        : {}),
                    ...(Array.isArray(item.properties.securityFilterGroupIds)
                        ? {
                            securityFilterGroupIds: item.properties.securityFilterGroupIds.map((group) => canonical(String(group))),
                        }
                        : {}),
                    ...(item.properties.definition
                        ? { definition: remapPolicy(item.properties.definition) }
                        : {}),
                    ...(item.properties.definitions
                        ? {
                            definitions: Object.fromEntries(Object.entries(object(item.properties.definitions)).map(([scope, policy]) => [scope, remapPolicy(policy)])),
                        }
                        : {}),
                },
            };
        });
        if (new Set(objects.map((item) => item.id)).size !==
            objects.length)
            providerFailure("DIRECTORY_IDENTITY_ALIAS_COLLISION", "The installed UCS to Samba identity mapping is not one-to-one.");
        return { ...inventory, objects };
    }
    function approvedProviderUrl(value: string, secret: Secret) {
        const url = new URL(value);
        if (url.protocol !== "https:" ||
            url.username ||
            url.password ||
            !secret.allowedHosts.includes(url.hostname.toLowerCase()))
            providerFailure("DIRECTORY_ENDPOINT_FORBIDDEN", "Use an approved HTTPS directory endpoint.", 403);
        return url;
    }
    function object(v: unknown): Json {
        return v && typeof v === "object" && !Array.isArray(v) ? (v as Json) : {};
    }
    function validateObjectTarget(action: DirectoryAction, inventory: DirectoryInventory) {
        const byId = (id: string) => {
            const o = inventory.objects.find((o) => o.id === id);
            if (!o)
                providerFailure("DIRECTORY_OBJECT_MISSING", "Refresh the directory inventory.");
            return o;
        };
        if ("objectId" in action && action.objectId)
            byId(action.objectId);
        if ("parentId" in action && byId(action.parentId).kind !== "ou")
            providerFailure("DIRECTORY_TARGET_INVALID", "Choose a directory OU or container.");
        if (action.kind === "account.update") {
            if (byId(action.objectId).kind !== "account")
                providerFailure("DIRECTORY_TARGET_INVALID", "Choose an account.");
            if (!Object.keys(action.fields).length ||
                Object.keys(action.fields).some((k) => !inventory.fields.some((f) => f.name === k && f.write)))
                providerFailure("DIRECTORY_FIELD_UNSUPPORTED", "Choose fields supported for writing by this connector.", 422);
        }
        if (["account.disable", "account.enable"].includes(action.kind) &&
            "objectId" in action &&
            byId(action.objectId!).kind !== "account")
            providerFailure("DIRECTORY_TARGET_INVALID", "Choose an account.");
        if (action.kind === "object.move" &&
            !["account", "computer", "group", "ou"].includes(byId(action.objectId).kind))
            providerFailure("DIRECTORY_TARGET_INVALID", "Choose an account, computer, group or OU.");
        if (action.kind === "object.move") {
            const seen = new Set<string>();
            let target: DirectoryObject | undefined = byId(action.parentId);
            while (target) {
                if (target.id === action.objectId || seen.has(target.id))
                    providerFailure("DIRECTORY_OU_CYCLE", "An OU cannot be moved into itself or a descendant.");
                seen.add(target.id);
                target = target.parentId ? byId(target.parentId) : undefined;
            }
        }
        if (action.kind === "ou.update" ||
            action.kind === "gpo.order" ||
            action.kind === "gpo.inheritance") {
            if (byId(action.objectId).kind !== "ou")
                providerFailure("DIRECTORY_TARGET_INVALID", "Choose an OU.");
        }
        if (action.kind === "gpo.order") {
            const links = (byId(action.objectId).properties.gpoLinks ?? []) as {
                gpoId: string;
            }[];
            if (new Set(action.gpoIds).size !== action.gpoIds.length ||
                links.length !== action.gpoIds.length ||
                links.some((l) => !action.gpoIds.includes(l.gpoId)))
                providerFailure("DIRECTORY_LINKS_CHANGED", "Include every current GPO link exactly once.");
        }
        if (action.kind === "gpo.filter") {
            if (byId(action.objectId).kind !== "gpo" ||
                action.groupIds.some((id) => byId(id).kind !== "group"))
                providerFailure("DIRECTORY_TARGET_INVALID", "Choose a GPO and security groups.");
        }
        if (action.kind === "group.members") {
            if (byId(action.objectId).kind !== "group")
                providerFailure("DIRECTORY_TARGET_INVALID", "Choose a group.");
            for (const id of action.memberIds) {
                if (!["account", "group", "computer"].includes(byId(id).kind))
                    providerFailure("DIRECTORY_TARGET_INVALID", "Choose an account, computer, or nested group.");
                if (action.action === "remove")
                    continue;
                const seen = new Set<string>();
                const todo = [id];
                while (todo.length) {
                    const next = todo.pop()!;
                    if (next === action.objectId)
                        providerFailure("DIRECTORY_GROUP_CYCLE", "Nested groups cannot form a cycle.");
                    if (!seen.has(next)) {
                        seen.add(next);
                        todo.push(...byId(next).memberIds);
                    }
                }
            }
        }
        if (action.kind === "gpo.publish" || action.kind === "gpo.link") {
            const policy = action.kind === "gpo.publish" ? action.definition : null;
            if (action.objectId && byId(action.objectId).kind !== "gpo")
                providerFailure("DIRECTORY_TARGET_INVALID", "Choose a Windows GPO.");
            if (action.kind === "gpo.publish" && action.objectId) {
                if (policy?.scope === "computer" &&
                    (!policy.computerTargetId ||
                        !["ou", "group"].includes(byId(policy.computerTargetId).kind)))
                    providerFailure("DIRECTORY_TARGET_INVALID", "Choose an explicit computer group or OU.");
                return; // A settings patch preserves existing links and security filtering.
            }
            const target = byId(action.kind === "gpo.link"
                ? action.parentId
                : action.definition.targetOuId);
            const filterId = action.kind === "gpo.link"
                ? action.filterGroupId
                : action.definition.filterGroupId;
            const filter = filterId ? byId(filterId) : null;
            if (target.kind !== "ou" || (filter && filter.kind !== "group"))
                providerFailure("DIRECTORY_TARGET_INVALID", "Choose an OU and a security filter group.");
            if (policy?.computerTargetId &&
                !["ou", "group"].includes(byId(policy.computerTargetId).kind))
                providerFailure("DIRECTORY_TARGET_INVALID", "Choose an explicit computer group or OU.");
            if (policy?.scope === "computer" &&
                ![target.id, filter?.id].includes(policy.computerTargetId!))
                providerFailure("DIRECTORY_TARGET_INVALID", "The computer target must be the linked OU or security filter group.");
        }
    }
    function assertWriteBoundary(secret: Secret, objects: DirectoryObject[]) {
        if (!secret.writeBaseDns.length)
            providerFailure("DIRECTORY_WRITE_SCOPE_REQUIRED", "Install approved write DNs in the provider profile.", 403);
        for (const o of objects)
            if (o.kind !== "gpo" &&
                !secret.writeBaseDns.some((base) => o.dn.toLowerCase() === base.toLowerCase() ||
                    o.dn.toLowerCase().endsWith("," + base.toLowerCase())))
                providerFailure("DIRECTORY_WRITE_OUTSIDE_SCOPE", "The object is outside the provider's approved write scope.", 403);
    }
    return { parsedInventory, canonicalInventory, approvedProviderUrl, validateObjectTarget, assertWriteBoundary };
}
export function directoryActionTargets(action: DirectoryAction, before: DirectoryInventory) {
    const find = (id: string) => before.objects.find((o) => o.id === id)!;
    const targets: DirectoryObject[] = [];
    if ("objectId" in action && action.objectId)
        targets.push(find(action.objectId));
    if ("parentId" in action)
        targets.push(find(action.parentId));
    if (action.kind === "gpo.publish") {
        if (!action.objectId)
            targets.push(find(action.definition.targetOuId), find(action.definition.filterGroupId));
        if (action.definition.computerTargetId)
            targets.push(find(action.definition.computerTargetId));
    }
    if (action.kind === "group.members")
        targets.push(...action.memberIds.map(find));
    if (action.kind === "gpo.filter")
        targets.push(...action.groupIds.map(find));
    if (action.kind === "gpo.order")
        targets.push(...action.gpoIds.map(find));
    if (action.kind === "gpo.link" && action.filterGroupId)
        targets.push(find(action.filterGroupId));
    return targets;
}
