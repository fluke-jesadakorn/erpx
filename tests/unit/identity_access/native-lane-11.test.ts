import { createHash } from "node:crypto";
import { afterEach, expect, it, vi } from "vitest";

vi.mock("server-only", () => ({}));
const { nativeRequest } = vi.hoisted(() => ({ nativeRequest: vi.fn() }));
vi.mock("@erpx/platform-runtime-server/public/native-http", () => ({ nativeRequest }));

import {
  nativeIdentityIds,
  nativePrincipals,
  nativeDirectoryConfig,
  nativeRole,
  nativeRoles,
  nativeRoleIds,
  nativeLane11Configured,
  setNativeLane11ForTest,
  type NativeLane11Actor,
} from "../../../modules/identity-access/server/src/adapters/native-lane-11/client";
import {
  currentPersonPrincipals,
  personIdentityIds,
} from "../../../modules/identity-access/server/src/adapters/iam-person";

const holding = (holdingId: string, holdingSlug: string, host: string) => ({
  origin: `https://${host}`,
  host,
  holdingId,
  holdingSlug,
});

const config = (holdings: unknown = undefined) =>
  JSON.stringify({
    privateBaseUrl: "http://127.0.0.1:3198",
    timeoutMs: 10_000,
    holdings:
      holdings ??
      [
        holding("h001", "acme", "acme.jesadakorn.com"),
        holding("h002", "northstar", "northstar.jesadakorn.com"),
        holding("h003", "erpx", "erpx-internal.jesadakorn.com"),
      ],
  });

const actor: NativeLane11Actor = { holdingId: "h001", holdingSlug: "acme", companyId: "acme:co" };
const identityIdsPayload = { identityIds: ["user", "alias"] };
const principalsPayload = {
  principals: [
    { original: "old", current: "new" },
    { original: "self", current: "self" },
  ],
};
const rolePayload = {
  id: "employee",
  name: "Employee",
  companyId: "acme:co",
  revision: 2,
  builtIn: true,
  permissions: ["iam.read"],
  grantablePermissions: [],
  scopes: ["holding"],
  status: "active",
};

afterEach(() => {
  setNativeLane11ForTest(undefined);
  vi.unstubAllEnvs();
  nativeRequest.mockReset();
});

function stubTransport(payload: unknown, status = 200) {
  const fetchMock = vi.fn().mockImplementation(async () => new Response(JSON.stringify(payload), { status }));
  nativeRequest.mockImplementation((url: URL, init: { headers: Record<string, string> }) =>
    fetchMock(url.href, { ...init, headers: new Headers(init.headers) }),
  );
  return fetchMock;
}

it("reads native identity ids from the WorkContext-selected Holding with the pinned shape", async () => {
  vi.stubEnv("ERPX_NATIVE_IDENTITY_ACCESS_LANE_11_CONFIG", config());
  const fetchMock = stubTransport(identityIdsPayload);

  expect(await nativeIdentityIds(actor, ["user"])).toEqual(["user", "alias"]);
  expect(fetchMock).toHaveBeenCalledExactlyOnceWith(
    "http://127.0.0.1:3198/api/v1/holdings/acme/identity-access/person/identity-ids",
    expect.objectContaining({ method: "POST" }),
  );
  const init = fetchMock.mock.calls[0][1] as RequestInit;
  const headers = init.headers as Headers;
  expect(nativeRequest).toHaveBeenCalledTimes(1);
  expect(headers.get("host")).toBe("acme.jesadakorn.com");
  expect(headers.get("origin")).toBe("https://acme.jesadakorn.com");
  expect(headers.get("x-erpx-context-company")).toBe("acme:co");
  expect(JSON.parse(String(init.body))).toEqual({ ids: ["user"] });
});

it("selects each of the three registered Holdings and never a first-holding default", async () => {
  vi.stubEnv("ERPX_NATIVE_IDENTITY_ACCESS_LANE_11_CONFIG", config());
  const fetchMock = stubTransport(identityIdsPayload);
  const cases = [
    [{ holdingId: "h002", holdingSlug: "northstar", companyId: "northstar:co" }, "northstar", "northstar.jesadakorn.com"],
    [{ holdingId: "h003", holdingSlug: "erpx", companyId: "erpx:co" }, "erpx", "erpx-internal.jesadakorn.com"],
  ] as const;
  for (const [selected, slug, host] of cases) {
    await nativeIdentityIds(selected, ["user"]);
    const [url, init] = fetchMock.mock.calls.at(-1)!;
    expect(url).toBe(`http://127.0.0.1:3198/api/v1/holdings/${slug}/identity-access/person/identity-ids`);
    expect((init.headers as Headers).get("host")).toBe(host);
  }
  expect(fetchMock).toHaveBeenCalledTimes(2);
});

it("fails closed on an unknown WorkContext target without issuing a request", async () => {
  vi.stubEnv("ERPX_NATIVE_IDENTITY_ACCESS_LANE_11_CONFIG", config());
  const fetchMock = stubTransport(identityIdsPayload);
  await expect(nativeIdentityIds({ ...actor, holdingId: "h999" }, ["user"])).rejects.toMatchObject({ status: 503 });
  await expect(nativeIdentityIds({ ...actor, holdingSlug: "unknown" }, ["user"])).rejects.toMatchObject({ status: 503 });
  await expect(nativeIdentityIds({ ...actor, holdingId: "h002" }, ["user"])).rejects.toMatchObject({ status: 503 });
  expect(fetchMock).not.toHaveBeenCalled();
});

it("rejects a configured company-pin mismatch and malformed configuration", async () => {
  vi.stubEnv(
    "ERPX_NATIVE_IDENTITY_ACCESS_LANE_11_CONFIG",
    config([{ ...holding("h001", "acme", "acme.jesadakorn.com"), companyId: "different:co" }]),
  );
  const fetchMock = stubTransport(identityIdsPayload);
  await expect(nativeIdentityIds(actor, ["user"])).rejects.toMatchObject({ status: 409 });
  expect(fetchMock).not.toHaveBeenCalled();

  vi.stubEnv("ERPX_NATIVE_IDENTITY_ACCESS_LANE_11_CONFIG", config([]));
  await expect(nativeIdentityIds(actor, ["user"])).rejects.toMatchObject({ status: 503 });
  const many = Array.from({ length: 65 }, (_, i) => holding(`h${i}`, `slug-${i}`, `slug-${i}.example.test`));
  vi.stubEnv("ERPX_NATIVE_IDENTITY_ACCESS_LANE_11_CONFIG", config(many));
  await expect(nativeIdentityIds(actor, ["user"])).rejects.toMatchObject({ status: 503 });
});

it("reads native principals and forwards the snapshot flag", async () => {
  vi.stubEnv("ERPX_NATIVE_IDENTITY_ACCESS_LANE_11_CONFIG", config());
  const fetchMock = stubTransport(principalsPayload);
  const result = await nativePrincipals(actor, ["old", "self"], true);
  expect([...result]).toEqual([
    ["old", "new"],
    ["self", "self"],
  ]);
  expect(fetchMock.mock.calls[0][0]).toBe(
    "http://127.0.0.1:3198/api/v1/holdings/acme/identity-access/person/principals",
  );
  expect(JSON.parse(String((fetchMock.mock.calls[0][1] as RequestInit).body))).toEqual({
    ids: ["old", "self"],
    snapshot: true,
  });
});

it("keeps the compatibility adapters as transparent native transports", async () => {
  const identityIds = vi.fn().mockResolvedValue(["user", "alias"]);
  const principals = vi.fn().mockResolvedValue(new Map([["old", "new"]]));
  const directory = vi.fn().mockResolvedValue([{ id: "conn-1", name: "Directory" }]);
  const role = vi.fn().mockResolvedValue(rolePayload);
  const roles = vi.fn().mockResolvedValue([rolePayload]);
  const roleIds = vi.fn().mockResolvedValue(["employee"]);
  setNativeLane11ForTest({ identityIds, principals, directory, role, roles, roleIds });

  expect(await personIdentityIds({} as never, "acme:co", ["user"], actor)).toEqual(["user", "alias"]);
  expect(identityIds).toHaveBeenCalledExactlyOnceWith(actor, ["user"], "acme:co");
  // Legacy callers pass a Company only; the WorkContext stays undefined.
  expect(await personIdentityIds({} as never, "acme:co", ["user"])).toEqual(["user", "alias"]);
  expect(identityIds).toHaveBeenLastCalledWith(undefined, ["user"], "acme:co");
  expect(await personIdentityIds({} as never, "acme:co", [])).toEqual([]);
  expect(identityIds).toHaveBeenCalledTimes(2);

  expect([...(await currentPersonPrincipals({} as never, "acme:co", ["old"], actor))]).toEqual([
    ["old", "new"],
  ]);
  expect(principals).toHaveBeenCalledExactlyOnceWith(actor, ["old"], false, "acme:co");
  expect(await currentPersonPrincipals({} as never, "acme:co", [])).toEqual(new Map());
  expect(principals).toHaveBeenCalledTimes(1);
});

it("resolves a unique legacy Company to its configured Holding", async () => {
  vi.stubEnv(
    "ERPX_NATIVE_IDENTITY_ACCESS_LANE_11_CONFIG",
    config([
      { ...holding("h001", "acme", "acme.jesadakorn.com"), companyId: "acme:co" },
      { ...holding("h002", "northstar", "northstar.jesadakorn.com"), companyId: "northstar:co" },
    ]),
  );
  const fetchMock = stubTransport(identityIdsPayload);
  expect(await personIdentityIds({} as never, "northstar:co", ["user"])).toEqual(["user", "alias"]);
  expect(fetchMock.mock.calls[0][0]).toBe(
    "http://127.0.0.1:3198/api/v1/holdings/northstar/identity-access/person/identity-ids",
  );
  const headers = (fetchMock.mock.calls[0][1] as RequestInit).headers as Headers;
  expect(headers.get("host")).toBe("northstar.jesadakorn.com");
  expect(headers.get("x-erpx-context-company")).toBe("northstar:co");
});

it("fails closed for an unknown or ambiguous legacy Company", async () => {
  vi.stubEnv(
    "ERPX_NATIVE_IDENTITY_ACCESS_LANE_11_CONFIG",
    config([
      { ...holding("h001", "acme", "acme.jesadakorn.com"), companyId: "acme:co" },
      { ...holding("h002", "northstar", "northstar.jesadakorn.com"), companyId: "acme:co" },
    ]),
  );
  const fetchMock = stubTransport(identityIdsPayload);
  await expect(personIdentityIds({} as never, "unknown:co", ["user"])).rejects.toMatchObject({ status: 503 });
  await expect(personIdentityIds({} as never, "acme:co", ["user"])).rejects.toMatchObject({ status: 503 });
  expect(fetchMock).not.toHaveBeenCalled();
});

it("prefers the WorkContext actor over the legacy Company", async () => {
  vi.stubEnv(
    "ERPX_NATIVE_IDENTITY_ACCESS_LANE_11_CONFIG",
    config([
      { ...holding("h001", "acme", "acme.jesadakorn.com"), companyId: "acme:co" },
      { ...holding("h002", "northstar", "northstar.jesadakorn.com"), companyId: "northstar:co" },
    ]),
  );
  const fetchMock = stubTransport(identityIdsPayload);
  const northstarActor: NativeLane11Actor = {
    holdingId: "h002",
    holdingSlug: "northstar",
    companyId: "northstar:co",
  };
  await nativeIdentityIds(northstarActor, ["user"], "acme:co");
  expect(fetchMock.mock.calls[0][0]).toBe(
    "http://127.0.0.1:3198/api/v1/holdings/northstar/identity-access/person/identity-ids",
  );
  const headers = (fetchMock.mock.calls[0][1] as RequestInit).headers as Headers;
  expect(headers.get("host")).toBe("northstar.jesadakorn.com");
  expect(headers.get("x-erpx-context-company")).toBe("northstar:co");
});

it("fails closed without configuration, on malformed replies and on native rejections", async () => {
  expect(nativeLane11Configured({})).toBe(false);
  await expect(nativeIdentityIds(actor, ["user"])).rejects.toMatchObject({ status: 503 });
  // Without a WorkContext the transport fails closed instead of defaulting.
  await expect(personIdentityIds({} as never, "acme:co", ["user"])).rejects.toMatchObject({ status: 503 });

  vi.stubEnv("ERPX_NATIVE_IDENTITY_ACCESS_LANE_11_CONFIG", config());
  stubTransport({ identityIds: ["user", 7] });
  await expect(nativeIdentityIds(actor, ["user"])).rejects.toMatchObject({ status: 503 });

  stubTransport({ error: { code: "IDENTITY_ACCESS_NATIVE_LANE_11_REQUEST_INVALID", message: "bad" } }, 422);
  await expect(nativePrincipals(actor, ["old"], false)).rejects.toMatchObject({
    status: 422,
    code: "IDENTITY_ACCESS_NATIVE_LANE_11_REQUEST_INVALID",
  });
  await expect(nativeIdentityIds(actor, [])).rejects.toMatchObject({ status: 422 });
});

it("reads all four directory kinds from the WorkContext-selected Holding", async () => {
  vi.stubEnv("ERPX_NATIVE_IDENTITY_ACCESS_LANE_11_CONFIG", config());
  const fetchMock = stubTransport({ rows: [{ id: "conn-1", name: "Directory" }] });
  for (const kind of ["connections", "groups", "previews", "identities"] as const) {
    expect(await nativeDirectoryConfig(actor, kind, kind === "connections")).toEqual([
      { id: "conn-1", name: "Directory" },
    ]);
    const [url, init] = fetchMock.mock.calls.at(-1)!;
    expect(url).toBe(`http://127.0.0.1:3198/api/v1/holdings/acme/identity-access/directory/${kind}`);
    const headers = (init as RequestInit).headers as Headers;
    expect(headers.get("host")).toBe("acme.jesadakorn.com");
    expect(headers.get("x-erpx-context-company")).toBe("acme:co");
    expect(JSON.parse(String((init as RequestInit).body))).toEqual({ canManage: kind === "connections" });
  }
  expect(fetchMock).toHaveBeenCalledTimes(4);
});

it("resolves legacy directory reads by a unique configured Company and fails closed otherwise", async () => {
  vi.stubEnv(
    "ERPX_NATIVE_IDENTITY_ACCESS_LANE_11_CONFIG",
    config([
      { ...holding("h001", "acme", "acme.jesadakorn.com"), companyId: "acme:co" },
      { ...holding("h002", "northstar", "northstar.jesadakorn.com"), companyId: "northstar:co" },
    ]),
  );
  const fetchMock = stubTransport({ rows: [{ id: "grp-1" }] });
  expect(await nativeDirectoryConfig(undefined, "groups", false, "northstar:co")).toEqual([
    { id: "grp-1" },
  ]);
  expect(fetchMock.mock.calls[0][0]).toBe(
    "http://127.0.0.1:3198/api/v1/holdings/northstar/identity-access/directory/groups",
  );
  await expect(nativeDirectoryConfig(undefined, "groups", false, "unknown:co")).rejects.toMatchObject({
    status: 503,
  });
  expect(fetchMock).toHaveBeenCalledTimes(1);
});

it("validates directory replies and rejects unknown read kinds", async () => {
  vi.stubEnv("ERPX_NATIVE_IDENTITY_ACCESS_LANE_11_CONFIG", config());
  stubTransport({ rows: [{ id: 1 }] });
  await expect(nativeDirectoryConfig(actor, "connections", true)).rejects.toMatchObject({ status: 503 });
  await expect(nativeDirectoryConfig(actor, "not-a-kind" as never, true)).rejects.toMatchObject({ status: 422 });
});

it("pins the rendered directory SQL oracles shared with the Rust owner", () => {
  const hash = (value: string) => createHash("sha256").update(value).digest("hex");
  const rendered = [
    "SELECT * FROM iam_connections WHERE company_id=? ORDER BY name",
    "SELECT id,name,provider,status,health,last_sync_at,last_error,version FROM iam_connections WHERE company_id=? ORDER BY name",
    "SELECT g.* FROM iam_groups g JOIN iam_connections c ON c.id=g.connection_id WHERE c.company_id=? ORDER BY g.name",
    "SELECT p.* FROM iam_connection_previews p JOIN iam_connections c ON c.id=p.connection_id AND c.version=p.version WHERE c.company_id=?",
    "SELECT * FROM iam_identities WHERE company_id=? ORDER BY name",
  ];
  const expected = [
    "3144f4820586196b219737843f911a07965fdb0113804cdad91d7f6fda977773",
    "209ddd712e3a4b843bdb750a771d4c73b48b9aa00aa8561a6fbdeb1b53792068",
    "7f160d7b5618894adb149c34e65ce7199377ea0163fa6af3cf3bc4ea10ba14ed",
    "782717bcc057fdd4ca3dfa3c9d74ce8331f9135b1f56e915dc4055c24a78eee0",
    "0a39de165486dd1c7d27197770daaedcd368839f4783b0f50fadb6ac7aa45946",
  ];
  rendered.forEach((sql, i) => expect(hash(sql)).toBe(expected[i]));
});

it("reads published roles from the WorkContext-selected Holding", async () => {
  vi.stubEnv("ERPX_NATIVE_IDENTITY_ACCESS_LANE_11_CONFIG", config());
  const fetchMock = stubTransport({ role: rolePayload });
  expect(await nativeRole(actor, "employee")).toEqual(rolePayload);
  expect(fetchMock.mock.calls[0][0]).toBe(
    "http://127.0.0.1:3198/api/v1/holdings/acme/identity-access/role/employee",
  );
  const headers = (fetchMock.mock.calls[0][1] as RequestInit).headers as Headers;
  expect(headers.get("host")).toBe("acme.jesadakorn.com");
  expect(headers.get("x-erpx-context-company")).toBe("acme:co");
  expect(JSON.parse(String((fetchMock.mock.calls[0][1] as RequestInit).body))).toEqual({});
});

it("reads a bounded role list and the published role ids, failing closed on bad replies", async () => {
  vi.stubEnv("ERPX_NATIVE_IDENTITY_ACCESS_LANE_11_CONFIG", config());
  const fetchMock = stubTransport({ roles: [rolePayload, rolePayload], ids: ["employee"] });
  expect(await nativeRoles(actor, ["employee", "employee"])).toEqual([rolePayload]);
  expect(fetchMock.mock.calls[0][0]).toBe(
    "http://127.0.0.1:3198/api/v1/holdings/acme/identity-access/roles",
  );
  expect(JSON.parse(String((fetchMock.mock.calls[0][1] as RequestInit).body))).toEqual({
    ids: ["employee", "employee"],
  });
  expect(await nativeRoleIds(actor)).toEqual(["employee"]);
  expect(fetchMock.mock.calls[1][0]).toBe(
    "http://127.0.0.1:3198/api/v1/holdings/acme/identity-access/role-ids",
  );

  stubTransport({ role: { ...rolePayload, permissions: "bad" } });
  await expect(nativeRole(actor, "employee")).rejects.toMatchObject({ status: 503 });
  await expect(nativeRole(actor, "")).rejects.toMatchObject({ status: 422 });
});

it("resolves legacy published-role reads by a unique configured Company", async () => {
  vi.stubEnv(
    "ERPX_NATIVE_IDENTITY_ACCESS_LANE_11_CONFIG",
    config([
      { ...holding("h001", "acme", "acme.jesadakorn.com"), companyId: "acme:co" },
      { ...holding("h002", "northstar", "northstar.jesadakorn.com"), companyId: "northstar:co" },
    ]),
  );
  const fetchMock = stubTransport({ role: null });
  expect(await nativeRole(undefined, "employee", "northstar:co")).toBeNull();
  expect(fetchMock.mock.calls[0][0]).toBe(
    "http://127.0.0.1:3198/api/v1/holdings/northstar/identity-access/role/employee",
  );
  await expect(nativeRoleIds(undefined, "unknown:co")).rejects.toMatchObject({ status: 503 });
});

it("pins the rendered role SQL oracles shared with the Rust owner", () => {
  const hash = (value: string) => createHash("sha256").update(value).digest("hex");
  expect(
    hash(
      "SELECT r.*,v.permissions_json,v.grantable_json,v.scopes_json FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id IN ('*',?) AND r.id=? AND r.status='active'",
    ),
  ).toBe("aee8e70c4f14551dfe728a88768ec3ca7bbfdde9d8fe5ddb2d958955b48d5115");
  expect(
    hash("SELECT id FROM iam_roles WHERE company_id IN ('*',?) AND status='active' AND published_revision>0 ORDER BY built_in DESC,name"),
  ).toBe("8cd0010d3cbd15edebb0b2e5b1d2fd47ddfc07802f8d2c05d415484202cec765");
});

it("pins the rendered SQL and result oracles shared with the Rust owner", () => {
  const hash = (value: string) => createHash("sha256").update(value).digest("hex");
  const aliases =
    "SELECT DISTINCT linked.user_id FROM iam_members original JOIN iam_members linked ON linked.company_id=original.company_id AND linked.person_id=original.person_id WHERE original.company_id=? AND original.user_id IN (?,?)";
  const result = JSON.stringify({ identityIds: identityIdsPayload.identityIds, ...principalsPayload });
  expect(hash(aliases)).toBe("a3d0b68ebcfcba00321a62935ca0b82c342bab5ca6a7a88d09ce4a0d4cdd70c1");
  expect(hash(result)).toBe("54e7120defc300cdeb7bf99504f4599e9f69a05506d91ec6cf6a99e8fe729db0");
});
