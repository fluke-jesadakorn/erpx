import { createHash } from "node:crypto";
import { afterEach, expect, it, vi } from "vitest";

vi.mock("server-only", () => ({}));

import {
  nativeIdentityIds,
  nativePrincipals,
  nativeLane11Configured,
  setNativeLane11ForTest,
  type NativeLane11Actor,
} from "../../../packages/server-identity_access/src/adapters/native-lane-11/client";
import {
  currentPersonPrincipals,
  personIdentityIds,
} from "../../../packages/server-identity_access/src/adapters/iam-person";

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

afterEach(() => {
  setNativeLane11ForTest(undefined);
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
});

function stubFetch(payload: unknown, status = 200) {
  const fetchMock = vi.fn().mockImplementation(async () => new Response(JSON.stringify(payload), { status }));
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

it("reads native identity ids from the WorkContext-selected Holding with the pinned shape", async () => {
  vi.stubEnv("ERPX_NATIVE_IDENTITY_ACCESS_LANE_11_CONFIG", config());
  const fetchMock = stubFetch(identityIdsPayload);

  expect(await nativeIdentityIds(actor, ["user"])).toEqual(["user", "alias"]);
  expect(fetchMock).toHaveBeenCalledExactlyOnceWith(
    "http://127.0.0.1:3198/api/v1/holdings/acme/identity-access/person/identity-ids",
    expect.objectContaining({ method: "POST", cache: "no-store" }),
  );
  const init = fetchMock.mock.calls[0][1] as RequestInit;
  const headers = init.headers as Headers;
  expect(headers.get("host")).toBe("acme.jesadakorn.com");
  expect(headers.get("x-erpx-context-company")).toBe("acme:co");
  expect(JSON.parse(String(init.body))).toEqual({ ids: ["user"] });
});

it("selects each of the three registered Holdings and never a first-holding default", async () => {
  vi.stubEnv("ERPX_NATIVE_IDENTITY_ACCESS_LANE_11_CONFIG", config());
  const fetchMock = stubFetch(identityIdsPayload);
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
  const fetchMock = stubFetch(identityIdsPayload);
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
  const fetchMock = stubFetch(identityIdsPayload);
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
  const fetchMock = stubFetch(principalsPayload);
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
  setNativeLane11ForTest({ identityIds, principals });

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
  const fetchMock = stubFetch(identityIdsPayload);
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
  const fetchMock = stubFetch(identityIdsPayload);
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
  const fetchMock = stubFetch(identityIdsPayload);
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
  vi.stubGlobal(
    "fetch",
    vi.fn().mockResolvedValue(new Response(JSON.stringify({ identityIds: ["user", 7] }), { status: 200 })),
  );
  await expect(nativeIdentityIds(actor, ["user"])).rejects.toMatchObject({ status: 503 });

  vi.stubGlobal(
    "fetch",
    vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({ error: { code: "IDENTITY_ACCESS_NATIVE_LANE_11_REQUEST_INVALID", message: "bad" } }),
        { status: 422 },
      ),
    ),
  );
  await expect(nativePrincipals(actor, ["old"], false)).rejects.toMatchObject({
    status: 422,
    code: "IDENTITY_ACCESS_NATIVE_LANE_11_REQUEST_INVALID",
  });
  await expect(nativeIdentityIds(actor, [])).rejects.toMatchObject({ status: 422 });
});

it("pins the rendered SQL and result oracles shared with the Rust owner", () => {
  const hash = (value: string) => createHash("sha256").update(value).digest("hex");
  const aliases =
    "SELECT DISTINCT linked.user_id FROM iam_members original JOIN iam_members linked ON linked.company_id=original.company_id AND linked.person_id=original.person_id WHERE original.company_id=? AND original.user_id IN (?,?)";
  const result = JSON.stringify({ identityIds: identityIdsPayload.identityIds, ...principalsPayload });
  expect(hash(aliases)).toBe("a3d0b68ebcfcba00321a62935ca0b82c342bab5ca6a7a88d09ce4a0d4cdd70c1");
  expect(hash(result)).toBe("54e7120defc300cdeb7bf99504f4599e9f69a05506d91ec6cf6a99e8fe729db0");
});
