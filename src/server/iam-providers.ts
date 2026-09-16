import { createRemoteJWKSet, jwtVerify } from "jose";
import "server-only";
import { readFile } from "node:fs/promises";
import { isAbsolute } from "node:path";
import { Client, type Entry } from "ldapts";
import { z } from "zod";
import type { DirectoryConfig, DirectoryProvider } from "@/lib/iam";
import {
  type DirectoryHierarchy,
  validateDirectoryHierarchy,
} from "@/lib/hierarchy";
import { ApiError } from "./http";
import { runtimeEnv } from "./runtime";

export type ProviderUser = {
  externalId: string;
  username: string;
  name: string;
  email: string | null;
  enabled: boolean;
  groupIds: string[];
};
export type ProviderSnapshot = {
  hierarchy?: DirectoryHierarchy;
  users: ProviderUser[];
  groups: { externalId: string; name: string }[];
  complete: true;
};
export type ProviderConnection = {
  id: string;
  provider: DirectoryProvider;
  config: DirectoryConfig;
};
const secretSchema = z.object({
  allowedHosts: z.array(z.string()).min(1),
  connectionIds: z.array(z.string()).min(1),
  bindDn: z.string().optional(),
  bindPassword: z.string().optional(),
  clientSecret: z.string().optional(),
  caPem: z.string().optional(),
  verificationIdToken: z.string().optional(),
});
export async function directorySecret(
  config: DirectoryConfig,
  connectionId: string,
) {
  const file = (await runtimeEnv()).ERPX_IAM_SECRET_PROFILES;
  if (typeof file !== "string" || !isAbsolute(file))
    throw new ApiError(
      409,
      "DIRECTORY_SECRET_UNAVAILABLE",
      "Install the approved directory credential profile on the server.",
    );
  let profiles: Record<string, unknown>;
  try {
    profiles = JSON.parse(await readFile(file, "utf8"));
  } catch {
    throw new ApiError(
      409,
      "DIRECTORY_SECRET_UNAVAILABLE",
      "The directory credential profiles are unavailable.",
    );
  }
  const parsed = secretSchema.safeParse(profiles[config.secretProfile]);
  if (!parsed.success)
    throw new ApiError(
      409,
      "DIRECTORY_SECRET_UNAVAILABLE",
      "The approved credential profile is incomplete.",
    );
  if (!parsed.data.connectionIds.includes(connectionId))
    throw new ApiError(
      403,
      "DIRECTORY_PROFILE_SCOPE",
      "This secret profile is not approved for this connection.",
    );
  return parsed.data;
}
function approvedUrl(
  value: string | undefined,
  hosts: string[],
  protocol: string,
) {
  let url: URL;
  try {
    url = new URL(value ?? "");
  } catch {
    throw new ApiError(
      422,
      "DIRECTORY_URL_INVALID",
      "Enter a valid directory endpoint.",
    );
  }
  if (
    url.protocol !== protocol ||
    url.username ||
    url.password ||
    !hosts.includes(url.hostname.toLowerCase())
  )
    throw new ApiError(
      422,
      "DIRECTORY_ENDPOINT_FORBIDDEN",
      "The endpoint must use a server-approved hostname and secure protocol.",
    );
  return url;
}
export function escapeLdap(value: string) {
  return value.replace(
    /[\\*()\0]/g,
    (c) => `\\${c.charCodeAt(0).toString(16).padStart(2, "0")}`,
  );
}
function values(row: Entry, name: string): string[] {
  const value = row[name];
  return (
    Array.isArray(value) ? value : value === undefined ? [] : [value]
  ).map((v) => (Buffer.isBuffer(v) ? v.toString("utf8") : String(v)));
}
function first(row: Entry, name: string) {
  return values(row, name)[0] ?? "";
}
export function adGuid(value: unknown) {
  const raw = Array.isArray(value) ? value[0] : value;
  if (!Buffer.isBuffer(raw) || raw.length !== 16)
    throw new ApiError(
      502,
      "DIRECTORY_IDENTITY_INVALID",
      "The directory did not provide an immutable identity.",
    );
  const b = Buffer.from(raw);
  return [
    b.subarray(0, 4).reverse().toString("hex"),
    b.subarray(4, 6).reverse().toString("hex"),
    b.subarray(6, 8).reverse().toString("hex"),
    b.subarray(8, 10).toString("hex"),
    b.subarray(10).toString("hex"),
  ].join("-");
}
function immutable(row: Entry, ad: boolean) {
  const id = ad ? adGuid(row.objectGUID) : first(row, "entryUUID");
  if (!id)
    throw new ApiError(
      502,
      "DIRECTORY_IDENTITY_INVALID",
      "The directory did not return immutable identifiers.",
    );
  return id;
}
export function ldapEnabled(row: Entry, ad: boolean, now = Date.now()) {
  if (ad) {
    const expires = first(row, "accountExpires");
    return (
      !(Number(first(row, "userAccountControl")) & 2) &&
      (!expires ||
        expires === "0" ||
        expires === "9223372036854775807" ||
        Number(BigInt(expires) / BigInt(10000) - BigInt(11644473600000)) > now)
    );
  }
  const shadow = Number(first(row, "shadowExpire"));
  return (
    !first(row, "pwdAccountLockedTime") &&
    !first(row, "sambaAcctFlags").includes("D") &&
    (!shadow || shadow < 0 || shadow * 86400000 > now)
  );
}
async function ldapClient(connection: ProviderConnection) {
  const secret = await directorySecret(connection.config, connection.id);
  const url = approvedUrl(
    connection.config.ldapUrl,
    secret.allowedHosts,
    "ldaps:",
  );
  if (!secret.bindDn || !secret.bindPassword)
    throw new ApiError(
      409,
      "DIRECTORY_SECRET_UNAVAILABLE",
      "The LDAP bind profile is incomplete.",
    );
  const client = new Client({
    url: url.href,
    timeout: 15000,
    connectTimeout: 5000,
    tlsOptions: {
      rejectUnauthorized: true,
      ...(secret.caPem ? { ca: secret.caPem } : {}),
    },
  });
  return { client, secret };
}
export async function readLdapDirectory(
  connection: ProviderConnection,
): Promise<ProviderSnapshot> {
  const { client, secret } = await ldapClient(connection),
    ad = connection.provider === "active_directory";
  try {
    await client.bind(secret.bindDn!, secret.bindPassword!);
    const users = await client.search(connection.config.baseDn ?? "", {
      scope: "sub",
      filter:
        connection.config.userFilter ??
        (ad
          ? "(&(objectCategory=person)(objectClass=user))"
          : "(&(objectClass=posixAccount)(uid=*))"),
      attributes: [
        "objectGUID",
        "entryUUID",
        "uid",
        "sAMAccountName",
        "userPrincipalName",
        "displayName",
        "cn",
        "mail",
        "mailPrimaryAddress",
        "userAccountControl",
        "accountExpires",
        "pwdAccountLockedTime",
        "shadowExpire",
        "sambaAcctFlags",
      ],
      explicitBufferAttributes: ["objectGUID"],
      paged: { pageSize: 500 },
      sizeLimit: 10001,
    });
    const groups = await client.search(
      connection.config.groupBaseDn ?? connection.config.baseDn ?? "",
      {
        scope: "sub",
        filter: ad
          ? "(objectClass=group)"
          : "(|(objectClass=posixGroup)(objectClass=univentionGroup)(objectClass=groupOfUniqueNames))",
        attributes: [
          "objectGUID",
          "entryUUID",
          "cn",
          "member",
          "uniqueMember",
          "memberUid",
        ],
        explicitBufferAttributes: ["objectGUID"],
        paged: { pageSize: 500 },
        sizeLimit: 10001,
      },
    );
    if (
      users.searchEntries.length > 10000 ||
      groups.searchEntries.length > 10000
    )
      throw new ApiError(
        502,
        "DIRECTORY_LIMIT_EXCEEDED",
        "Narrow the directory view before synchronization.",
      );
    if (users.searchReferences?.length || groups.searchReferences?.length)
      throw new ApiError(
        502,
        "DIRECTORY_INCOMPLETE",
        "The directory returned referrals. Configure a complete directory view before activation.",
      );
    // AD returns large member lists in ranges. Never interpret an omitted range as removal.
    for (const group of groups.searchEntries) {
      let page = group,
        expected = 0;
      const members = [...values(group, "member")];
      for (let count = 0; count < 100; count++) {
        const key = Object.keys(page).find((k) =>
          k.toLowerCase().startsWith("member;range="),
        );
        if (!key) {
          if (expected > 0)
            throw new ApiError(
              502,
              "DIRECTORY_INCOMPLETE",
              "An AD membership range was missing.",
            );
          break;
        }
        const match = /^member;range=(\d+)-(\d+|\*)$/i.exec(key);
        if (!match || Number(match[1]) !== expected)
          throw new ApiError(
            502,
            "DIRECTORY_INCOMPLETE",
            "AD membership ranges were incomplete.",
          );
        members.push(...values(page, key));
        if (match[2] === "*") {
          group.member = members;
          break;
        }
        expected = Number(match[2]) + 1;
        if (count === 99)
          throw new ApiError(
            502,
            "DIRECTORY_LIMIT_EXCEEDED",
            "The AD group exceeds its supported range limit.",
          );
        const next = await client.search(group.dn, {
          scope: "base",
          filter: "(objectClass=*)",
          attributes: [`member;range=${expected}-*`],
        });
        if (next.searchReferences?.length || next.searchEntries.length !== 1)
          throw new ApiError(
            502,
            "DIRECTORY_INCOMPLETE",
            "AD membership range lookup failed.",
          );
        page = next.searchEntries[0];
      }
    }
    const groupRows = groups.searchEntries.map((g) => ({
      externalId: immutable(g, ad),
      name: first(g, "cn"),
      dn: g.dn.toLowerCase(),
      members: [...values(g, "member"), ...values(g, "uniqueMember")].map((v) =>
        v.toLowerCase(),
      ),
      uids: values(g, "memberUid"),
    }));
    const mapped = users.searchEntries.map((u) => {
      const username = first(u, ad ? "sAMAccountName" : "uid"),
        membership = new Set<string>();
      let frontier = [u.dn.toLowerCase()];
      for (let depth = 0; frontier.length && depth < 64; depth++) {
        const next = [];
        for (const g of groupRows)
          if (
            !membership.has(g.externalId) &&
            (g.members.some((d) => frontier.includes(d)) ||
              (depth === 0 && g.uids.includes(username)))
          ) {
            membership.add(g.externalId);
            next.push(g.dn);
          }
        frontier = next;
        if (depth === 63 && frontier.length)
          throw new ApiError(
            502,
            "DIRECTORY_GROUP_DEPTH",
            "Group nesting exceeds the supported depth.",
          );
      }
      return {
        externalId: immutable(u, ad),
        username,
        name: first(u, "displayName") || first(u, "cn") || username,
        email: first(u, "mailPrimaryAddress") || first(u, "mail") || null,
        enabled: ldapEnabled(u, ad),
        groupIds: [...membership],
      };
    });
    const containers = await client.search(connection.config.baseDn ?? "", {
      scope: "sub",
      filter: "(|(objectClass=organizationalUnit)(objectClass=container))",
      attributes: ["objectGUID", "entryUUID", "ou", "cn", "objectClass"],
      explicitBufferAttributes: ["objectGUID"],
      paged: { pageSize: 500 },
      sizeLimit: 10001,
    });
    if (
      containers.searchReferences?.length ||
      containers.searchEntries.length > 10000
    )
      throw new ApiError(
        502,
        "DIRECTORY_INCOMPLETE",
        "Directory containers were incomplete.",
      );
    const groupBase = connection.config.groupBaseDn,
      userBase = connection.config.baseDn ?? "";
    if (
      groupBase &&
      groupBase.toLowerCase() !== userBase.toLowerCase() &&
      !groupBase.toLowerCase().endsWith(`,${userBase.toLowerCase()}`)
    ) {
      const groupContainers = await client.search(groupBase, {
        scope: "sub",
        filter: "(|(objectClass=organizationalUnit)(objectClass=container))",
        attributes: ["objectGUID", "entryUUID", "ou", "cn", "objectClass"],
        explicitBufferAttributes: ["objectGUID"],
        paged: { pageSize: 500 },
        sizeLimit: 10001,
      });
      if (
        groupContainers.searchReferences?.length ||
        groupContainers.searchEntries.length > 10000
      )
        throw new ApiError(
          502,
          "DIRECTORY_INCOMPLETE",
          "Group containers were incomplete.",
        );
      for (const entry of groupContainers.searchEntries)
        if (
          !containers.searchEntries.some(
            (c) => c.dn.toLowerCase() === entry.dn.toLowerCase(),
          )
        )
          containers.searchEntries.push(entry);
    }
    const containerRows = containers.searchEntries.map((c) => ({
      id: immutable(c, ad),
      dn: c.dn.toLowerCase(),
      row: c,
    }));
    // Split only at unescaped RDN separators (escaped commas may occur in names).
    function parent(dn: string) {
      let escaped = false;
      for (let index = 0; index < dn.length; index++) {
        if (!escaped && dn[index] === ",")
          return (
            containerRows.find(
              (c) =>
                c.dn ===
                dn
                  .slice(index + 1)
                  .trimStart()
                  .toLowerCase(),
            )?.id ?? null
          );
        escaped = dn[index] === "\\" && !escaped;
      }
      return null;
    }
    const hierarchy: DirectoryHierarchy = {
      containers: containerRows.map((c) => ({
        externalId: c.id,
        parentId: parent(c.dn),
        name: first(c.row, "ou") || first(c.row, "cn"),
        kind: values(c.row, "objectClass").some(
          (v) => v.toLowerCase() === "organizationalunit",
        )
          ? "ou"
          : "container",
      })),
      placements: [
        ...users.searchEntries.map((u) => ({
          kind: "identity" as const,
          externalId: immutable(u, ad),
          containerId: parent(u.dn),
        })),
        ...groupRows.map((g) => ({
          kind: "group" as const,
          externalId: g.externalId,
          containerId: parent(g.dn),
        })),
      ],
      groupEdges: groupRows.flatMap((p) =>
        groupRows
          .filter((c) => p.members.includes(c.dn))
          .map((c) => ({ parentId: p.externalId, childId: c.externalId })),
      ),
    };
    validateDirectoryHierarchy(
      hierarchy,
      mapped.map((u) => u.externalId),
      groupRows.map((g) => g.externalId),
    );
    return {
      users: mapped,
      groups: groupRows.map(({ externalId, name }) => ({ externalId, name })),
      hierarchy,
      complete: true,
    };
  } finally {
    await client.unbind().catch(() => {});
  }
}
export async function verifyLdapPassword(
  connection: ProviderConnection,
  username: string,
  password: string,
) {
  if (!username || !password)
    throw new ApiError(
      401,
      "DIRECTORY_LOGIN_FAILED",
      "Directory sign-in failed.",
    );
  const { client, secret } = await ldapClient(connection),
    ad = connection.provider === "active_directory";
  try {
    await client.bind(secret.bindDn!, secret.bindPassword!);
    const entries = await client.search(connection.config.baseDn ?? "", {
      scope: "sub",
      filter: ad
        ? `(&(objectCategory=person)(objectClass=user)(|(sAMAccountName=${escapeLdap(username)})(userPrincipalName=${escapeLdap(username)})))`
        : `(&(objectClass=posixAccount)(uid=${escapeLdap(username)}))`,
      attributes: [
        "objectGUID",
        "entryUUID",
        "userAccountControl",
        "accountExpires",
        "pwdAccountLockedTime",
        "shadowExpire",
        "sambaAcctFlags",
      ],
      explicitBufferAttributes: ["objectGUID"],
      sizeLimit: 2,
    });
    const user =
      entries.searchEntries.length === 1 ? entries.searchEntries[0] : null;
    if (!user || !ldapEnabled(user, ad)) throw new Error("Denied");
    await client.bind(user.dn, password);
    return immutable(user, ad);
  } catch {
    throw new ApiError(
      401,
      "DIRECTORY_LOGIN_FAILED",
      "Directory sign-in failed.",
    );
  } finally {
    await client.unbind().catch(() => {});
  }
}
async function fetchJson(url: string, options: RequestInit = {}) {
  const r = await fetch(url, {
    ...options,
    redirect: "error",
    signal: AbortSignal.timeout(15000),
    cache: "no-store",
  });
  if (!r.ok)
    throw new ApiError(
      502,
      "DIRECTORY_PROVIDER_UNAVAILABLE",
      "The directory request failed. Check the connection and consent.",
    );
  return r.json();
}
export async function readEntraDirectory(
  connection: ProviderConnection,
): Promise<ProviderSnapshot> {
  const secret = await directorySecret(connection.config, connection.id),
    tenant = connection.config.tenantId;
  if (
    !tenant ||
    !/^[a-f0-9-]{36}$/i.test(tenant) ||
    !connection.config.clientId ||
    !secret.clientSecret
  )
    throw new ApiError(
      422,
      "ENTRA_CONFIGURATION_INVALID",
      "Tenant, client, and credential profile are required.",
    );
  approvedUrl(
    "https://login.microsoftonline.com",
    secret.allowedHosts,
    "https:",
  );
  approvedUrl("https://graph.microsoft.com", secret.allowedHosts, "https:");
  const token = await fetchJson(
    `https://login.microsoftonline.com/${tenant}/oauth2/v2.0/token`,
    {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({
        client_id: connection.config.clientId,
        client_secret: secret.clientSecret,
        grant_type: "client_credentials",
        scope: "https://graph.microsoft.com/.default",
      }),
    },
  );
  const headers = { Authorization: `Bearer ${token.access_token}` };
  async function pages(url: string) {
    const out: Record<string, unknown>[] = [];
    let next: string | undefined = url;
    for (let count = 0; next && count < 100; count++) {
      const parsed = new URL(next);
      if (
        parsed.origin !== "https://graph.microsoft.com" ||
        !parsed.pathname.startsWith("/v1.0/")
      )
        throw new ApiError(
          502,
          "DIRECTORY_PAGING_INVALID",
          "The directory returned an invalid continuation.",
        );
      const data = await fetchJson(next, { headers });
      out.push(...data.value);
      if (out.length > 10000)
        throw new ApiError(
          502,
          "DIRECTORY_LIMIT_EXCEEDED",
          "The directory view exceeds its limit.",
        );
      next = data["@odata.nextLink"];
    }
    if (next)
      throw new ApiError(
        502,
        "DIRECTORY_INCOMPLETE",
        "Directory pagination did not complete.",
      );
    return out;
  }
  const users = await pages(
      "https://graph.microsoft.com/v1.0/users?$select=id,displayName,userPrincipalName,mail,accountEnabled",
    ),
    groups = new Map<string, string>();
  const mapped: ProviderUser[] = [];
  for (const user of users) {
    const membership = await pages(
      `https://graph.microsoft.com/v1.0/users/${encodeURIComponent(String(user.id))}/transitiveMemberOf/microsoft.graph.group?$select=id,displayName`,
    );
    for (const g of membership)
      groups.set(String(g.id), String(g.displayName ?? g.id));
    mapped.push({
      externalId: String(user.id),
      username: String(user.userPrincipalName),
      name: String(user.displayName ?? user.userPrincipalName),
      email: typeof user.mail === "string" ? user.mail : null,
      enabled: user.accountEnabled === true,
      groupIds: membership.map((g) => String(g.id)),
    });
  }
  const allGroups = await pages(
    "https://graph.microsoft.com/v1.0/groups?$select=id,displayName",
  );
  for (const g of allGroups)
    groups.set(String(g.id), String(g.displayName ?? g.id));
  const hierarchy: DirectoryHierarchy = {
    containers: [],
    placements: [],
    groupEdges: [],
  };
  for (const childId of groups.keys()) {
    const parents = await pages(
      `https://graph.microsoft.com/v1.0/groups/${encodeURIComponent(childId)}/memberOf`,
    );
    for (const p of parents.filter(
      (p) => p["@odata.type"] === "#microsoft.graph.group",
    )) {
      const parentId = String(p.id);
      if (!groups.has(parentId))
        throw new ApiError(
          502,
          "DIRECTORY_INCOMPLETE",
          "A nested group was absent from the directory snapshot.",
        );
      hierarchy.groupEdges.push({ parentId, childId });
    }
  }
  validateDirectoryHierarchy(
    hierarchy,
    mapped.map((u) => u.externalId),
    [...groups.keys()],
  );
  return {
    users: mapped,
    groups: [...groups].map(([externalId, name]) => ({ externalId, name })),
    hierarchy,
    complete: true,
  };
}
export const readDirectory = (connection: ProviderConnection) =>
  connection.provider === "entra_id"
    ? readEntraDirectory(connection)
    : readLdapDirectory(connection);
export async function testDirectory(connection: ProviderConnection) {
  const snapshot = await readDirectory(connection);
  if (connection.config.mode === "oidc") {
    const secret = await directorySecret(connection.config, connection.id),
      issuer = connection.config.issuer;
    approvedUrl(issuer, secret.allowedHosts, "https:");
    if (!secret.clientSecret || !connection.config.clientId)
      throw new ApiError(
        422,
        "OIDC_CONFIGURATION_INVALID",
        "A client ID and external client-secret profile are required.",
      );
    const discovery = await fetchJson(
      `${issuer!.replace(/\/$/, "")}/.well-known/openid-configuration`,
    );
    if (
      discovery.issuer !== issuer ||
      !discovery.jwks_uri ||
      !discovery.token_endpoint
    )
      throw new ApiError(
        502,
        "OIDC_DISCOVERY_INVALID",
        "The issuer did not return valid OIDC discovery.",
      );
    for (const endpoint of [
      discovery.jwks_uri,
      discovery.token_endpoint,
      discovery.authorization_endpoint,
    ])
      approvedUrl(endpoint, secret.allowedHosts, "https:");

    if (connection.provider === "univention") {
      if (!secret.verificationIdToken)
        throw new ApiError(
          409,
          "UCS_IDENTITY_MAPPING_REQUIRED",
          "Install a current Keycloak verification ID token in the external test profile to prove the immutable entryUUID mapping.",
        );
      try {
        const { payload } = await jwtVerify(
          secret.verificationIdToken,
          createRemoteJWKSet(new URL(discovery.jwks_uri)),
          {
            issuer,
            audience: connection.config.clientId,
            algorithms: [
              "RS256",
              "PS256",
              "ES256",
              "RS384",
              "ES384",
              "RS512",
              "ES512",
            ],
          },
        );
        if (
          typeof payload.entryUUID !== "string" ||
          !payload.sub ||
          !snapshot.users.some((u) => u.externalId === payload.entryUUID)
        )
          throw new Error("Invalid mapping");
      } catch {
        throw new ApiError(
          409,
          "UCS_IDENTITY_MAPPING_INVALID",
          "The Keycloak test identity must have a verified immutable entryUUID matching the LDAP directory.",
        );
      }
    }
  }
  return {
    userCount: snapshot.users.length,
    groupCount: snapshot.groups.length,
    snapshot,
  };
}
