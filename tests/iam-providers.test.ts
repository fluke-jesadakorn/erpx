import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { generateKeyPair, exportJWK, SignJWT } from "jose";
const mocks = vi.hoisted(() => ({
  bind: vi.fn(),
  search: vi.fn(),
  unbind: vi.fn(),
  construct: vi.fn(),
  file: vi.fn(),
}));
vi.mock("node:fs/promises", () => ({ readFile: mocks.file }));
vi.mock("ldapts", () => ({
  Client: class {
    constructor(o: unknown) {
      mocks.construct(o);
    }
    bind = mocks.bind;
    search = mocks.search;
    unbind = mocks.unbind;
  },
}));
vi.mock("../src/server/runtime", () => ({
  runtimeEnv: async () => ({ ERPX_IAM_SECRET_PROFILES: "/approved/iam.json" }),
}));
import {
  readLdapDirectory,
  verifyLdapPassword,
  readEntraDirectory,
  testDirectory,
  directorySecret,
} from "../src/server/iam-providers";
import type { ProviderConnection } from "../src/server/iam-providers";
const id = "ac803e63-8b98-4e73-adea-68c7b148fabc",
  tenant = "aa803e63-8b98-4e73-adea-68c7b148fabc";
const ucs: ProviderConnection = {
  id,
  provider: "univention",
  config: {
    mode: "ldaps",
    secretProfile: "test",
    authority: "corp.test",
    ldapUrl: "ldaps://ucs.corp.test",
    baseDn: "dc=corp,dc=test",
  },
};
const profile = {
  connectionIds: [id],
  allowedHosts: [
    "ucs.corp.test",
    "sso.corp.test",
    "graph.microsoft.com",
    "login.microsoftonline.com",
  ],
  bindDn: "cn=reader,dc=corp,dc=test",
  bindPassword: "fixture-bind",
  clientSecret: "fixture-client",
};
const user = {
  dn: "uid=staff,dc=corp,dc=test",
  uid: "staff",
  entryUUID: "99112233-4455-6677-8899-aabbccddeeff",
  cn: "Staff",
  mailPrimaryAddress: "duplicate@corp.test",
};
beforeEach(() => {
  vi.resetAllMocks();
  mocks.file.mockResolvedValue(JSON.stringify({ test: profile }));
  mocks.bind.mockResolvedValue(undefined);
  mocks.unbind.mockResolvedValue(undefined);
  mocks.search.mockResolvedValue({ searchEntries: [] });
});
afterEach(() => vi.unstubAllGlobals());
it.each(["active_directory", "univention"] as const)("retains %s real containers, placement and escaped commas", async provider => {
  const guid = Buffer.from("33221100554477668899aabbccddeeff", "hex"), ouGuid = Buffer.from("44221100554477668899aabbccddeeff", "hex"), groupGuid = Buffer.from("55221100554477668899aabbccddeeff", "hex");
  const dn = "ou=People,dc=corp,dc=test";
  mocks.search.mockResolvedValueOnce({ searchEntries: [{...user,dn:`uid=Last\\, First,${dn}`,objectGUID:guid,sAMAccountName:"staff"}] }).mockResolvedValueOnce({searchEntries:[{dn:`cn=Staff,${dn}`,entryUUID:"group",objectGUID:groupGuid,cn:"Staff"}]}).mockResolvedValueOnce({searchEntries:[{dn,entryUUID:"ou",objectGUID:ouGuid,ou:"People",objectClass:["organizationalUnit"]}]});
  const snapshot = await readLdapDirectory({...ucs,provider});
  expect(snapshot.hierarchy?.containers[0]).toMatchObject({name:"People",kind:"ou",parentId:null});
  expect(snapshot.hierarchy?.placements.map(p=>p.containerId)).toEqual([snapshot.hierarchy?.containers[0].externalId,snapshot.hierarchy?.containers[0].externalId]);
});
it("does not return a complete snapshot when container discovery returns referrals", async () => {
  mocks.search.mockResolvedValueOnce({searchEntries:[user]}).mockResolvedValueOnce({searchEntries:[]}).mockResolvedValueOnce({searchEntries:[],searchReferences:["ldaps://other.test"]});
  await expect(readLdapDirectory(ucs)).rejects.toMatchObject({code:"DIRECTORY_INCOMPLETE"});
});
it("reads UCS nested groups by immutable entryUUID and validates LDAPS certificates", async () => {
  mocks.search
    .mockResolvedValueOnce({ searchEntries: [user] })
    .mockResolvedValueOnce({
      searchEntries: [
        {
          dn: "cn=child,dc=corp,dc=test",
          entryUUID: "child",
          cn: "Child",
          uniqueMember: [user.dn],
        },
        {
          dn: "cn=parent,dc=corp,dc=test",
          entryUUID: "parent",
          cn: "Parent",
          uniqueMember: ["cn=child,dc=corp,dc=test"],
        },
      ],
    });
  const result = await readLdapDirectory(ucs);
  expect(result.users[0].groupIds).toEqual(["child", "parent"]);
  expect(result.users[0].externalId).toBe(user.entryUUID);
  expect(result.complete).toBe(true);
  expect(result.hierarchy?.groupEdges).toEqual([{ parentId: "parent", childId: "child" }]);
  expect(mocks.construct).toHaveBeenCalledWith(
    expect.objectContaining({ tlsOptions: { rejectUnauthorized: true } }),
  );
  expect(mocks.unbind).toHaveBeenCalled();
});
it.each(["active_directory", "univention"] as const)(
  "authenticates %s by bind and never returns passwords",
  async (provider) => {
    const guid = Buffer.from("33221100554477668899aabbccddeeff", "hex");
    mocks.search.mockResolvedValue({
      searchEntries: [{ ...user, objectGUID: guid, userAccountControl: "512" }],
    });
    const result = await verifyLdapPassword(
      { ...ucs, provider },
      "staff*)(uid=*)",
      "fixture-password",
    );
    expect(result).toBe(
      provider === "active_directory"
        ? "00112233-4455-6677-8899-aabbccddeeff"
        : user.entryUUID,
    );
    expect(mocks.bind).toHaveBeenLastCalledWith(user.dn, "fixture-password");
    expect(mocks.search.mock.calls[0][1].filter).toContain(
      "staff\\2a\\29\\28uid=\\2a\\29",
    );
  },
);
it("rejects certificate failure and incomplete LDAP searches instead of producing an empty directory", async () => {
  mocks.bind.mockRejectedValueOnce(new Error("certificate expired"));
  await expect(readLdapDirectory(ucs)).rejects.toThrow("certificate expired");
  mocks.search.mockRejectedValueOnce(new Error("size limit exceeded"));
  await expect(readLdapDirectory(ucs)).rejects.toThrow("size limit exceeded");
});
it("prevents using another connection’s approved external secret profile", async () => {
  await expect(
    directorySecret(ucs.config, "another-company"),
  ).rejects.toMatchObject({ code: "DIRECTORY_PROFILE_SCOPE" });
  expect(mocks.construct).not.toHaveBeenCalled();
});
it("uses tenant Graph pagination and transitive memberships even when token groups are absent", async () => {
  const fetchMock = vi.fn(async (url: string) => {
    if (url.includes("/token"))
      return Response.json({ access_token: "fixture-graph-token" });
    if (url.includes("transitiveMemberOf"))
      return Response.json({
        value: [{ id: "nested-group", displayName: "Nested group" }],
      });
    if (url.includes("/memberOf")) return Response.json({ value: [] });
    if (url.includes("/groups?")) return Response.json({ value: [{ id: "nested-group", displayName: "Nested group" }] });
    return Response.json({
      value: [
        {
          id: user.entryUUID,
          userPrincipalName: "staff@corp.test",
          displayName: "Staff",
          accountEnabled: true,
          mail: "duplicate@corp.test",
        },
      ],
    });
  });
  vi.stubGlobal("fetch", fetchMock);
  const result = await readEntraDirectory({
    id,
    provider: "entra_id",
    config: {
      mode: "oidc",
      authority: tenant,
      tenantId: tenant,
      clientId: "test-client",
      secretProfile: "test",
    },
  });
  expect(result.users[0].groupIds).toEqual(["nested-group"]);
  expect(result.hierarchy?.containers).toEqual([]);
  expect(
    fetchMock.mock.calls
      .map((c) => String(c[0]))
      .some((u) => u.includes(`/${tenant}/oauth2/v2.0/token`)),
  ).toBe(true);
});
it("rejects Graph continuation redirects outside the approved API", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) =>
      url.includes("/token")
        ? Response.json({ access_token: "fixture" })
        : Response.json({
            value: [],
            "@odata.nextLink": "https://unapproved.test/collect",
          }),
    ),
  );
  await expect(
    readEntraDirectory({
      id,
      provider: "entra_id",
      config: {
        mode: "oidc",
        authority: tenant,
        tenantId: tenant,
        clientId: "test",
        secretProfile: "test",
      },
    }),
  ).rejects.toMatchObject({ code: "DIRECTORY_PAGING_INVALID" });
});
it("requires a signed UCS OIDC entryUUID correlation and rejects a mismatched audience", async () => {
  const issuer = "https://sso.corp.test/realms/ucs",
    keys = await generateKeyPair("RS256"),
    jwk = await exportJWK(keys.publicKey);
  jwk.kid = "fixture";
  async function token(aud: string) {
    return new SignJWT({ entryUUID: user.entryUUID })
      .setProtectedHeader({ alg: "RS256", kid: "fixture" })
      .setIssuer(issuer)
      .setAudience(aud)
      .setSubject("immutable-subject")
      .setIssuedAt()
      .setExpirationTime("5m")
      .sign(keys.privateKey);
  }
  const connection = {
    ...ucs,
    config: {
      ...ucs.config,
      mode: "oidc" as const,
      issuer,
      clientId: "erpx-test",
    },
  };
  mocks.search.mockImplementation(
    async (_base: string, options: { filter: string }) => ({
      searchEntries: options.filter.includes("uid=") ? [user] : [],
    }),
  );
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: unknown) =>
      String(url).endsWith("/keys")
        ? Response.json({ keys: [jwk] })
        : Response.json({
            issuer,
            jwks_uri: issuer + "/keys",
            token_endpoint: issuer + "/token",
            authorization_endpoint: issuer + "/authorize",
          }),
    ),
  );
  mocks.file.mockResolvedValue(
    JSON.stringify({
      test: { ...profile, verificationIdToken: await token("erpx-test") },
    }),
  );
  expect((await testDirectory(connection)).userCount).toBe(1);
  mocks.file.mockResolvedValue(
    JSON.stringify({
      test: { ...profile, verificationIdToken: await token("another-client") },
    }),
  );
  await expect(testDirectory(connection)).rejects.toMatchObject({
    code: "UCS_IDENTITY_MAPPING_INVALID",
  });
});
