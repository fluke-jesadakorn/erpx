import "server-only";
import { request } from "node:https";
import { randomBytes, createHash } from "node:crypto";
import { z } from "zod";
import type {
  DirectoryAction,
  DirectoryInventory,
  DirectoryObject,
  ConnectorField,
} from "@/lib/directory-operations";
import { compileAdministrativePolicies } from "@/lib/directory-operations";
import { compilePreferences } from "@/lib/windows-preferences";
import {
  directorySecret,
  assertEmptyUcsContainer,
  type ProviderConnection,
} from "./iam-providers";
import { ApiError } from "./http";

type Secret = Awaited<ReturnType<typeof directorySecret>>;
type Json = Record<string, unknown>;
const inventorySchema = z.object({
  objects: z
    .array(
      z.object({
        id: z.string().min(1),
        dn: z.string().min(1),
        name: z.string(),
        kind: z.enum(["account", "group", "ou", "computer", "gpo"]),
        parentId: z.string().nullable(),
        version: z.string().min(1),
        module: z.string(),
        properties: z.record(z.string(), z.unknown()),
        memberIds: z.array(z.string()),
      }),
    )
    .max(10000),
  fields: z.array(
    z.object({
      name: z.string(),
      type: z.enum(["text", "checkbox"]),
      read: z.boolean(),
      write: z.boolean(),
    }),
  ),
  providerVersion: z.string().min(1),
  observedAt: z.number().finite(),
  complete: z.literal(true),
});
function parsedInventory(value: unknown): DirectoryInventory {
  const parsed = inventorySchema.safeParse(value);
  if (
    !parsed.success ||
    new Set(parsed.data.objects.map((o) => o.id)).size !==
      parsed.data.objects.length
  )
    providerFailure(
      "DIRECTORY_INCOMPLETE",
      "The provider returned an invalid inventory.",
      502,
    );
  return parsed.data;
}
export function canonicalInventory(
  inventory: DirectoryInventory,
  aliases: Record<string, string>,
) {
  const canonical = (id: string | null) =>
    id === null ? null : (aliases[id.toLowerCase()] ?? id);
  const remapPolicy = (value: unknown) => {
    const policy = object(value);
    return {
      ...policy,
      targetOuId:
        typeof policy.targetOuId === "string"
          ? canonical(policy.targetOuId)
          : policy.targetOuId,
      filterGroupId:
        typeof policy.filterGroupId === "string"
          ? canonical(policy.filterGroupId)
          : policy.filterGroupId,
      computerTargetId:
        typeof policy.computerTargetId === "string"
          ? canonical(policy.computerTargetId)
          : policy.computerTargetId,
    };
  };
  const objects = inventory.objects.map((item) => {
    const providerId = item.id,
      id = canonical(providerId)!;
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
                  gpoId:
                    typeof value.gpoId === "string"
                      ? canonical(value.gpoId)
                      : value.gpoId,
                };
              }),
            }
          : {}),
        ...(Array.isArray(item.properties.securityFilterGroupIds)
          ? {
              securityFilterGroupIds:
                item.properties.securityFilterGroupIds.map((group) =>
                  canonical(String(group)),
                ),
            }
          : {}),
        ...(item.properties.definition
          ? { definition: remapPolicy(item.properties.definition) }
          : {}),
        ...(item.properties.definitions
          ? {
              definitions: Object.fromEntries(
                Object.entries(object(item.properties.definitions)).map(
                  ([scope, policy]) => [scope, remapPolicy(policy)],
                ),
              ),
            }
          : {}),
      },
    };
  });
  if (
    new Set(objects.map((item) => item.id)).size !==
    objects.length
  )
    providerFailure(
      "DIRECTORY_IDENTITY_ALIAS_COLLISION",
      "The installed UCS to Samba identity mapping is not one-to-one.",
    );
  return {...inventory,objects};
}
export function providerFailure(
  code: string,
  message: string,
  status = 409,
): never {
  throw new ApiError(status, code, message);
}
export function approvedProviderUrl(value: string, secret: Secret) {
  const url = new URL(value);
  if (
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    !secret.allowedHosts.includes(url.hostname.toLowerCase())
  )
    providerFailure(
      "DIRECTORY_ENDPOINT_FORBIDDEN",
      "Use an approved HTTPS directory endpoint.",
      403,
    );
  return url;
}
/** TLS verification, bounded bodies and no redirects, including provider-returned links. */
export async function providerJson(
  url: URL,
  secret: Secret,
  method = "GET",
  body?: unknown,
  headers: Record<string, string> = {},
): Promise<{ status: number; headers: Record<string, string>; data: Json }> {
  approvedProviderUrl(url.href, secret);
  return new Promise((resolve, reject) => {
    const data = body === undefined ? undefined : JSON.stringify(body);
    const req = request(
      url,
      {
        method,
        ca: secret.caPem,
        servername: secret.tlsServerName,
        rejectUnauthorized: true,
        timeout: 20000,
        headers: {
          Accept: "application/json",
          ...(data
            ? {
                "Content-Type": "application/json",
                "Content-Length": String(Buffer.byteLength(data)),
              }
            : {}),
          ...headers,
        },
      },
      (res) => {
        const chunks: Buffer[] = [];
        let size = 0;
        res.on("data", (chunk: Buffer) => {
          size += chunk.length;
          if (size > 12_000_000) res.destroy(Error("DIRECTORY_RESPONSE_LIMIT"));
          else chunks.push(chunk);
        });
        res.on("error", () =>
          reject(
            new ApiError(
              502,
              "DIRECTORY_UNAVAILABLE",
              "The directory response was interrupted.",
            ),
          ),
        );
        res.on("end", () => {
          const raw = Buffer.concat(chunks).toString("utf8");
          let parsed: Json = {};
          try {
            parsed = raw ? JSON.parse(raw) : {};
          } catch {
            return reject(
              new ApiError(
                502,
                "DIRECTORY_RESPONSE_INVALID",
                "The provider returned an invalid response.",
              ),
            );
          }
          resolve({
            status: res.statusCode ?? 502,
            headers: Object.fromEntries(
              Object.entries(res.headers).map(([k, v]) => [
                k,
                Array.isArray(v) ? v[0] : String(v ?? ""),
              ]),
            ),
            data: parsed,
          });
        });
      },
    );
    req.on("timeout", () => req.destroy());
    req.on("error", () =>
      reject(
        new ApiError(
          502,
          "DIRECTORY_UNAVAILABLE",
          "The approved directory endpoint is unavailable.",
        ),
      ),
    );
    if (data) req.write(data);
    req.end();
  });
}
function checked<T extends { status: number }>(r: T): T {
  if (r.status === 412)
    providerFailure(
      "DIRECTORY_VERSION_CHANGED",
      "The directory object changed. Refresh and preview again.",
    );
  if (r.status >= 400 && r.status < 500 && r.status !== 429)
    providerFailure(
      "DIRECTORY_VALIDATION_FAILED",
      "The directory rejected the request (HTTP " +
        r.status +
        "). Check names, permissions and supported field values.",
      422,
    );
  if (r.status < 200 || r.status >= 300)
    providerFailure(
      "DIRECTORY_REQUEST_FAILED",
      "The directory returned HTTP " + r.status + ".",
      502,
    );
  return r;
}
function object(v: unknown): Json {
  return v && typeof v === "object" && !Array.isArray(v) ? (v as Json) : {};
}
function string(v: unknown) {
  return typeof v === "string" ? v : "";
}
const fields: ConnectorField[] = [
  "firstname",
  "lastname",
  "displayName",
  "description",
  "mailPrimaryAddress",
].map((name) => ({ name, type: "text", read: true, write: true }));
const modules = [
  ["users/user", "account"],
  ["groups/group", "group"],
  ["container/ou", "ou"],
  ["container/cn", "ou"],
  ["computers/windows", "computer"],
] as const;
async function udm(connection: ProviderConnection) {
  const secret = await directorySecret(connection.config, connection.id);
  if (!secret.udmUrl || !secret.udmUsername || !secret.udmPassword)
    providerFailure(
      "UDM_PROFILE_REQUIRED",
      "Install the UDM REST credential profile.",
    );
  const root = approvedProviderUrl(secret.udmUrl, secret);
  if (!root.pathname.endsWith("/univention/udm/"))
    providerFailure(
      "UDM_ENDPOINT_INVALID",
      "The UDM endpoint must end with /univention/udm/.",
    );
  const auth = {
    Authorization:
      "Basic " +
      Buffer.from(secret.udmUsername + ":" + secret.udmPassword).toString(
        "base64",
      ),
  };
  const call = (
    path: string,
    method = "GET",
    body?: unknown,
    headers: Record<string, string> = {},
  ) => {
    const url = new URL(path, root);
    if (url.origin !== root.origin || !url.pathname.startsWith(root.pathname))
      providerFailure(
        "DIRECTORY_LINK_FORBIDDEN",
        "A provider link left the approved API.",
        502,
      );
    return providerJson(url, secret, method, body, { ...auth, ...headers });
  };
  return { secret, root, call };
}
function toObject(
  raw: Json,
  module: string,
  kind: DirectoryObject["kind"],
  etag: string,
): DirectoryObject {
  const props = object(raw.properties),
    id = string(raw.uuid),
    dn = string(raw.dn);
  if (!id || !dn || !etag)
    providerFailure(
      "DIRECTORY_OBJECT_INVALID",
      "An immutable ID, DN and version are required.",
      502,
    );
  return {
    id,
    dn,
    module,
    kind,
    name:
      string(props.username) ||
      string(props.name) ||
      string(props.displayName) ||
      dn,
    parentId: null,
    version: etag,
    properties: Object.fromEntries(
      Object.entries(props).filter(([k]) =>
        [
          ...fields.map((f) => f.name),
          "disabled",
          "username",
          "name",
          "users",
          "hosts",
          "nestedGroup",
          "sambaRID",
          "sambaSID",
          "samba4ObjectGUID",
        ].includes(k),
      ),
    ),
    memberIds: [],
  };
}
async function runner(
  connection: ProviderConnection,
  route: string,
  payload: unknown,
) {
  const secret = await directorySecret(connection.config, connection.id);
  if (
    !(secret.sambaRunnerUrl ?? secret.windowsRunnerUrl) ||
    !(secret.sambaRunnerToken ?? secret.windowsRunnerToken)
  )
    providerFailure(
      "DIRECTORY_RUNNER_REQUIRED",
      "Install and configure the UCS Samba or Windows runner.",
    );
  const root = approvedProviderUrl(
      (secret.sambaRunnerUrl ?? secret.windowsRunnerUrl)!,
      secret,
    ),
    url = new URL(route, root.href.endsWith("/") ? root.href : root.href + "/");
  const result = await providerJson(url, secret, "POST", payload, {
    Authorization:
      "Bearer " + (secret.sambaRunnerToken ?? secret.windowsRunnerToken),
  });
  if (result.status === 409) {
    const code = object(result.data.error).code;
    if (typeof code === "string" && /^DIRECTORY_[A-Z_]+$/.test(code))
      providerFailure(
        code,
        "The runner stopped this operation. Read the inventory and preview again.",
      );
  }
  return checked(result).data;
}
export async function readAutomationInventory(
  connection: ProviderConnection,
): Promise<DirectoryInventory> {
  if (connection.provider === "active_directory") {
    const secret = await directorySecret(connection.config, connection.id);
    return canonicalInventory(
      parsedInventory(
      await runner(connection, "inventory", {
        connectionId: connection.id,
        baseDn: connection.config.baseDn,
      }),
      ),
      secret.identityAliases,
    );
  }
  if (connection.provider !== "univention")
    providerFailure(
      "PROVIDER_UNSUPPORTED",
      "This connector does not support directory preparation.",
    );
  const { call, secret } = await udm(connection),
    objects: DirectoryObject[] = [];
  const schema = checked(await call("openapi.json"));
  for (const [module, kind] of modules) {
    let path: string | null = module + "/?limit=1000";
    const seen = new Set<string>();
    while (path) {
      if (seen.has(path) || seen.size >= 30)
        providerFailure(
          "DIRECTORY_INCOMPLETE",
          "Directory pagination did not complete.",
          502,
        );
      seen.add(path);
      const response = await call(path);
      if (response.status === 404 && kind === "computer") break;
      checked(response);
      const rows =
        object(response.data._embedded)["udm:object"] ??
        (response.data.results === 0 ? [] : undefined);
      if (!Array.isArray(rows))
        providerFailure(
          "DIRECTORY_INCOMPLETE",
          "The directory returned an incomplete collection.",
          502,
        );
      for (const item of rows) {
        const raw = object(item),
          dn = string(raw.dn);
        const base = connection.config.baseDn?.toLowerCase();
        if (
          !base ||
          !(dn.toLowerCase() === base || dn.toLowerCase().endsWith("," + base))
        )
          continue;
        const detail = checked(
          await call(module + "/" + encodeURIComponent(dn)),
        );
        objects.push(toObject(detail.data, module, kind, detail.headers.etag));
        if (objects.length > 10000)
          providerFailure(
            "DIRECTORY_LIMIT_EXCEEDED",
            "Narrow the directory inventory.",
            502,
          );
      }
      const next = object(object(response.data._links).next).href;
      path = typeof next === "string" ? next : null;
    }
  }
  const byDn = new Map(objects.map((o) => [o.dn.toLowerCase(), o.id]));
  for (const o of objects) {
    o.parentId =
      byDn.get(o.dn.replace(/^(?:\\.|[^,])+,/, "").toLowerCase()) ?? null;
    o.memberIds = ["users", "hosts", "nestedGroup"].flatMap((k) =>
      Array.isArray(o.properties[k])
        ? (o.properties[k] as string[])
            .map((d) => byDn.get(d.toLowerCase()))
            .filter((v): v is string => !!v)
        : [],
    );
  }
  const template = checked(await call("users/user/add")),
    available = object(template.data.properties);
  const supported = fields.filter((f) => Object.hasOwn(available, f.name));
  if (secret.sambaRunnerUrl || secret.windowsRunnerUrl) {
    const win = parsedInventory(
      await runner(connection, "inventory", {
        connectionId: connection.id,
        baseDn: connection.config.baseDn,
      }),
    );
    const mapped = new Map<string, string>();
    for (const w of win.objects) {
      const local = objects.find(
        (o) =>
          (secret.sambaRunnerUrl
            ? w.properties.udmUuid === o.id
            : o.dn.toLowerCase() === w.dn.toLowerCase()) && o.kind === w.kind,
      );
      if (local) {
        if (secret.sambaRunnerUrl) {
          const proof = w.properties.udmUuid;
          if (proof !== local.id)
            providerFailure(
              "DIRECTORY_IDENTITY_MAPPING_UNVERIFIED",
              "Verify the S4 connector mapping before GPO operations.",
            );
        }
        local.properties.gpoLinks = w.properties.gpoLinks;
        local.properties.blockInheritance = w.properties.blockInheritance;
        local.properties.windowsIdentity = {
          id: w.id,
          dn: w.dn,
          version: w.version,
        };
        mapped.set(w.id, local.id);
      } else if (w.kind === "gpo") mapped.set(w.id, w.id);
    }
    for (const o of objects)
      if (Array.isArray(o.properties.gpoLinks))
        o.properties.gpoLinks = (
          o.properties.gpoLinks as { gpoId: string }[]
        ).map((l) => ({ ...l, gpoId: mapped.get(l.gpoId) ?? l.gpoId }));
    for (const o of win.objects)
      if (o.kind === "gpo") {
        if (o.memberIds.some((id) => !mapped.has(id)))
          providerFailure(
            "DIRECTORY_GPO_SCOPE_INCOMPLETE",
            "The GPO has linked targets outside this connector inventory.",
          );
        const mapPolicy = (value: unknown) => {
          const policy = object(value);
          return {
            ...policy,
            targetOuId:
              mapped.get(string(policy.targetOuId)) ?? policy.targetOuId,
            filterGroupId:
              mapped.get(string(policy.filterGroupId)) ?? policy.filterGroupId,
            computerTargetId:
              mapped.get(string(policy.computerTargetId)) ??
              policy.computerTargetId,
          };
        };
        objects.push({
          ...o,
          properties: {
            ...o.properties,
            ...(o.properties.definition
              ? { definition: mapPolicy(o.properties.definition) }
              : {}),
            ...(o.properties.definitions
              ? {
                  definitions: Object.fromEntries(
                    Object.entries(object(o.properties.definitions)).map(
                      ([scope, value]) => [scope, mapPolicy(value)],
                    ),
                  ),
                }
              : {}),
            securityFilterGroupIds: Array.isArray(
              o.properties.securityFilterGroupIds,
            )
              ? o.properties.securityFilterGroupIds.map(
                  (id) => mapped.get(String(id)) ?? id,
                )
              : [],
          },
          memberIds: o.memberIds.map((id) => mapped.get(id)!),
        });
      }
  }
  return {
    objects,
    fields: supported,
    providerVersion:
      "UDM " +
      (string(object(schema.data.info).version) ||
        createHash("sha256")
          .update(JSON.stringify(schema.data))
          .digest("hex")
          .slice(0, 16)),
    observedAt: Date.now(),
    complete: true,
  };
}
export function validateObjectTarget(
  action: DirectoryAction,
  inventory: DirectoryInventory,
) {
  const byId = (id: string) => {
    const o = inventory.objects.find((o) => o.id === id);
    if (!o)
      providerFailure(
        "DIRECTORY_OBJECT_MISSING",
        "Refresh the directory inventory.",
      );
    return o;
  };
  if ("objectId" in action && action.objectId) byId(action.objectId);
  if ("parentId" in action && byId(action.parentId).kind !== "ou")
    providerFailure(
      "DIRECTORY_TARGET_INVALID",
      "Choose a directory OU or container.",
    );
  if (action.kind === "account.update") {
    if (byId(action.objectId).kind !== "account")
      providerFailure("DIRECTORY_TARGET_INVALID", "Choose an account.");
    if (
      !Object.keys(action.fields).length ||
      Object.keys(action.fields).some(
        (k) => !inventory.fields.some((f) => f.name === k && f.write),
      )
    )
      providerFailure(
        "DIRECTORY_FIELD_UNSUPPORTED",
        "Choose fields supported for writing by this connector.",
        422,
      );
  }
  if (
    ["account.disable", "account.enable"].includes(action.kind) &&
    "objectId" in action &&
    byId(action.objectId!).kind !== "account"
  )
    providerFailure("DIRECTORY_TARGET_INVALID", "Choose an account.");
  if (
    action.kind === "object.move" &&
    !["account", "computer", "group", "ou"].includes(byId(action.objectId).kind)
  )
    providerFailure(
      "DIRECTORY_TARGET_INVALID",
      "Choose an account, computer, group or OU.",
    );
  if (action.kind === "object.move") {
    const seen = new Set<string>();
    let target: DirectoryObject | undefined = byId(action.parentId);
    while (target) {
      if (target.id === action.objectId || seen.has(target.id))
        providerFailure(
          "DIRECTORY_OU_CYCLE",
          "An OU cannot be moved into itself or a descendant.",
        );
      seen.add(target.id);
      target = target.parentId ? byId(target.parentId) : undefined;
    }
  }
  if (
    action.kind === "ou.update" ||
    action.kind === "gpo.order" ||
    action.kind === "gpo.inheritance"
  ) {
    if (byId(action.objectId).kind !== "ou")
      providerFailure("DIRECTORY_TARGET_INVALID", "Choose an OU.");
  }
  if (action.kind === "gpo.order") {
    const links = (byId(action.objectId).properties.gpoLinks ?? []) as {
      gpoId: string;
    }[];
    if (
      new Set(action.gpoIds).size !== action.gpoIds.length ||
      links.length !== action.gpoIds.length ||
      links.some((l) => !action.gpoIds.includes(l.gpoId))
    )
      providerFailure(
        "DIRECTORY_LINKS_CHANGED",
        "Include every current GPO link exactly once.",
      );
  }
  if (action.kind === "gpo.filter") {
    if (
      byId(action.objectId).kind !== "gpo" ||
      action.groupIds.some((id) => byId(id).kind !== "group")
    )
      providerFailure(
        "DIRECTORY_TARGET_INVALID",
        "Choose a GPO and security groups.",
      );
  }
  if (action.kind === "group.members") {
    if (byId(action.objectId).kind !== "group")
      providerFailure("DIRECTORY_TARGET_INVALID", "Choose a group.");
    for (const id of action.memberIds) {
      if (!["account", "group", "computer"].includes(byId(id).kind))
        providerFailure(
          "DIRECTORY_TARGET_INVALID",
          "Choose an account, computer, or nested group.",
        );
      if (action.action === "remove") continue;
      const seen = new Set<string>();
      const todo = [id];
      while (todo.length) {
        const next = todo.pop()!;
        if (next === action.objectId)
          providerFailure(
            "DIRECTORY_GROUP_CYCLE",
            "Nested groups cannot form a cycle.",
          );
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
      if (
        policy?.scope === "computer" &&
        (!policy.computerTargetId ||
          !["ou", "group"].includes(byId(policy.computerTargetId).kind))
      )
        providerFailure(
          "DIRECTORY_TARGET_INVALID",
          "Choose an explicit computer group or OU.",
        );
      return; // A settings patch preserves existing links and security filtering.
    }
    const target = byId(
      action.kind === "gpo.link"
        ? action.parentId
        : action.definition.targetOuId,
    );
    const filterId =
      action.kind === "gpo.link"
        ? action.filterGroupId
        : action.definition.filterGroupId;
    const filter = filterId ? byId(filterId) : null;
    if (target.kind !== "ou" || (filter && filter.kind !== "group"))
      providerFailure(
        "DIRECTORY_TARGET_INVALID",
        "Choose an OU and a security filter group.",
      );
    if (
      policy?.computerTargetId &&
      !["ou", "group"].includes(byId(policy.computerTargetId).kind)
    )
      providerFailure(
        "DIRECTORY_TARGET_INVALID",
        "Choose an explicit computer group or OU.",
      );
    if (
      policy?.scope === "computer" &&
      ![target.id, filter?.id].includes(policy.computerTargetId!)
    )
      providerFailure(
        "DIRECTORY_TARGET_INVALID",
        "The computer target must be the linked OU or security filter group.",
      );
  }
}
function assertWriteBoundary(secret: Secret, objects: DirectoryObject[]) {
  if (!secret.writeBaseDns.length)
    providerFailure(
      "DIRECTORY_WRITE_SCOPE_REQUIRED",
      "Install approved write DNs in the provider profile.",
      403,
    );
  for (const o of objects)
    if (
      o.kind !== "gpo" &&
      !secret.writeBaseDns.some(
        (base) =>
          o.dn.toLowerCase() === base.toLowerCase() ||
          o.dn.toLowerCase().endsWith("," + base.toLowerCase()),
      )
    )
      providerFailure(
        "DIRECTORY_WRITE_OUTSIDE_SCOPE",
        "The object is outside the provider's approved write scope.",
        403,
      );
}
export async function applyDirectoryAction(
  connection: ProviderConnection,
  operationId: string,
  action: DirectoryAction,
  before: DirectoryInventory,
) {
  const secret = await directorySecret(connection.config, connection.id);
  if (!secret.writeOperations.includes(action.kind))
    providerFailure(
      "DIRECTORY_WRITE_NOT_APPROVED",
      "This operation is not allowed by the installed provider profile.",
      403,
    );
  validateObjectTarget(action, before);
  const find = (id: string) => before.objects.find((o) => o.id === id)!;
  const targets: DirectoryObject[] = [];
  if ("objectId" in action && action.objectId)
    targets.push(find(action.objectId));
  if ("parentId" in action) targets.push(find(action.parentId));
  if (action.kind === "gpo.publish") {
    if (!action.objectId)
      targets.push(
        find(action.definition.targetOuId),
        find(action.definition.filterGroupId),
      );
    if (action.definition.computerTargetId)
      targets.push(find(action.definition.computerTargetId));
  }
  if (action.kind === "group.members")
    targets.push(...action.memberIds.map(find));
  if (action.kind === "gpo.filter") targets.push(...action.groupIds.map(find));
  if (action.kind === "gpo.order") targets.push(...action.gpoIds.map(find));
  if (action.kind === "gpo.link" && action.filterGroupId)
    targets.push(find(action.filterGroupId));
  assertWriteBoundary(secret, targets);
  if (
    connection.provider === "active_directory" ||
    action.kind === "gpo.publish" ||
    action.kind === "gpo.link" ||
    action.kind === "gpo.filter" ||
    action.kind === "gpo.order" ||
    action.kind === "gpo.inheritance"
  )
    return runner(connection, "apply", {
      connectionId: connection.id,
      operationId,
      action,
      before,
      registry:
        action.kind === "gpo.publish"
          ? compileAdministrativePolicies(action.definition)
          : [],
      preferences:
        action.kind === "gpo.publish"
          ? compilePreferences(
              action.definition,
              new Date(before.observedAt)
                .toISOString()
                .slice(0, 19)
                .replace("T", " "),
            )
          : [],
    });
  const { call } = await udm(connection);
  const get = async (o: DirectoryObject, checkVersion = false) => {
    const r = checked(await call(o.module + "/" + encodeURIComponent(o.dn)));
    if (r.data.uuid !== o.id)
      providerFailure(
        "DIRECTORY_IDENTITY_CHANGED",
        "The immutable identity changed.",
      );
    if (checkVersion && r.headers.etag !== o.version)
      providerFailure(
        "DIRECTORY_VERSION_CHANGED",
        "A referenced directory object changed.",
      );
    return r;
  };
  if (
    action.kind === "account.create" ||
    action.kind === "group.create" ||
    action.kind === "ou.create"
  ) {
    const moduleName =
        action.kind === "account.create"
          ? "users/user"
          : action.kind === "ou.create"
            ? "container/ou"
            : "groups/group",
      parent = find(action.parentId);
    const name =
        action.kind === "account.create" ? action.username : action.name,
      marker = "ERPX operation " + operationId;
    const dn =
      (action.kind === "account.create"
        ? "uid"
        : action.kind === "ou.create"
          ? "ou"
          : "cn") +
      "=" +
      name +
      "," +
      parent.dn;
    const old = await call(moduleName + "/" + encodeURIComponent(dn));
    if (old.status === 200) {
      if (object(old.data.properties).description !== marker)
        providerFailure(
          "DIRECTORY_OBJECT_EXISTS",
          "This name belongs to another directory object.",
        );
      return {
        verified: true,
        objectId: old.data.uuid,
        dn,
        version: old.headers.etag,
      };
    }
    if (old.status !== 404) checked(old);
    await get(parent, true);
    const properties =
      action.kind === "account.create"
        ? {
            username: action.username,
            firstname: action.firstName,
            lastname: action.lastName,
            password: randomBytes(36).toString("base64url"),
            disabled: true,
            description: marker,
          }
        : { name: action.name, description: marker };
    checked(
      await call(moduleName + "/", "POST", { position: parent.dn, properties }),
    );
    const after = checked(
      await call(moduleName + "/" + encodeURIComponent(dn)),
    );
    if (object(after.data.properties).description !== marker)
      providerFailure(
        "DIRECTORY_READBACK_FAILED",
        "The created object did not match the operation.",
      );
    return {
      verified: true,
      objectId: after.data.uuid,
      dn,
      version: after.headers.etag,
    };
  }
  if (!("objectId" in action) || !action.objectId)
    providerFailure("DIRECTORY_TARGET_INVALID", "Choose a directory object.");
  const target = find(action.objectId);
  if (action.kind === "object.move") {
    const parent = find(action.parentId),
      newDn = target.dn.match(/^(?:\\.|[^,])+/)![0] + "," + parent.dn;
    await get(parent);
    const moved = await call(target.module + "/" + encodeURIComponent(newDn));
    if (moved.status === 200 && moved.data.uuid === target.id)
      return {
        verified: true,
        objectId: target.id,
        dn: newDn,
        version: moved.headers.etag,
      };
    if (moved.status !== 404 && moved.status !== 200) checked(moved);
    await get(parent, true);
  }
  const current = await get(target),
    properties = object(current.data.properties);
  let patch: Json = {},
    position: string | undefined;
  if (action.kind === "account.disable") patch = { disabled: true };
  else if (action.kind === "account.enable") patch = { disabled: false };
  else if (action.kind === "account.update") patch = action.fields;
  else if (action.kind === "ou.update")
    patch = { description: action.description };
  else if (action.kind === "object.move") position = find(action.parentId).dn;
  else if (action.kind === "group.members") {
    for (const id of action.memberIds) await get(find(id));
    const pending = [...action.memberIds],
      seen = new Set<string>();
    while (pending.length) {
      const id = pending.pop()!;
      if (seen.has(id)) continue;
      seen.add(id);
      const member = find(id);
      if (member.kind === "group") {
        await get(member, true);
        pending.push(...member.memberIds);
      }
    }
    for (const kind of ["account", "group", "computer"] as const) {
      const property = {
        account: "users",
        group: "nestedGroup",
        computer: "hosts",
      }[kind];
      const desired = action.memberIds
        .map(find)
        .filter((o) => o.kind === kind)
        .map((o) => o.dn);
      if (!desired.length) continue;
      const existing = Array.isArray(properties[property])
        ? (properties[property] as string[])
        : [];
      patch[property] =
        action.action === "add"
          ? [...new Set([...existing, ...desired])]
          : existing.filter(
              (d) => !desired.some((v) => v.toLowerCase() === d.toLowerCase()),
            );
    }
  }
  const equal = (p: Json) =>
    Object.entries(patch).every(
      ([key, v]) => JSON.stringify(p[key]) === JSON.stringify(v),
    );
  if (
    (!position ||
      string(current.data.position).toLowerCase() === position.toLowerCase()) &&
    equal(properties)
  )
    return {
      verified: true,
      objectId: target.id,
      dn: target.dn,
      version: current.headers.etag,
    };
  if (current.headers.etag !== target.version)
    providerFailure(
      "DIRECTORY_VERSION_CHANGED",
      "The directory object changed after preview.",
    );
  let path = target.module + "/" + encodeURIComponent(target.dn);
  if (position) {
    if (target.kind === "ou")
      await assertEmptyUcsContainer(connection, target.dn, target.id);
    const { properties: allProperties, options, policies } = current.data;
    let moved = checked(
      await call(
        path,
        "PUT",
        { properties: allProperties, options, policies, position },
        { "If-Match": current.headers.etag },
      ),
    );
    // UDM moves a populated container asynchronously (202 -> progress -> 303).
    // Follow only links within the approved API, never repeat the mutation.
    const deadline = Date.now() + 60_000;
    while ([202, 301, 302, 303].includes(moved.status)) {
      if (
        !moved.headers.location ||
        moved.data.errors ||
        Date.now() >= deadline
      )
        providerFailure(
          "DIRECTORY_PARTIAL_UNCERTAIN",
          "The directory move could not be confirmed. Read inventory and preview again.",
        );
      const delay = Math.min(
        3000,
        Math.max(250, Number(moved.headers["retry-after"] || 0) * 1000),
      );
      await new Promise((resolve) =>
        setTimeout(resolve, Number.isFinite(delay) ? delay : 500),
      );
      moved = await call(moved.headers.location);
      if (![202, 301, 302, 303].includes(moved.status)) checked(moved);
    }
    if (moved.data.errors)
      providerFailure(
        "DIRECTORY_PARTIAL_UNCERTAIN",
        "The directory reported an incomplete move. Read inventory and preview again.",
      );
    path =
      target.module +
      "/" +
      encodeURIComponent(
        target.dn.match(/^(?:\\.|[^,])+/)![0] + "," + position,
      );
  } else
    checked(
      await call(
        path,
        "PATCH",
        { properties: patch },
        { "If-Match": current.headers.etag },
      ),
    );
  const after = checked(await call(path));
  if (after.data.uuid !== target.id)
    providerFailure(
      "DIRECTORY_IDENTITY_CHANGED",
      "The directory changed the permanent identity during the operation. Read inventory and reconcile before continuing.",
    );
  if (
    !equal(object(after.data.properties)) ||
    (position &&
      string(after.data.position).toLowerCase() !== position.toLowerCase())
  )
    providerFailure(
      "DIRECTORY_READBACK_FAILED",
      "The directory read-back did not match the requested change.",
    );
  return {
    verified: true,
    objectId: target.id,
    dn: after.data.dn,
    version: after.headers.etag,
  };
}
