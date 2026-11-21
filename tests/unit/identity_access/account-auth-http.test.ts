import { expect, it, vi } from "vitest";
import { createAccountAuthHttpPort } from "@erpx/feature-identity_access/adapters/http/account-auth";
import {createAccountSecurityHttpPort} from '@erpx/feature-identity_access/adapters/http/account-security';

const user = { id: "synthetic-user", name: "Synthetic User", email: "synthetic@example.test", emailVerified: true, image: null };
const handle = "00000000-0000-4000-8000-000000000001";
const devices = { initialized: true, revision: 12, maximum: 5, accounts: [{ handle, name: user.name, email: user.email, image: null, current: true }] };
const response = (value: unknown, status = 200) => Response.json(value, { status });

it('binds MFA setup and exact-session verification to the existing typed same-origin transport without storing secrets',async()=>{
 const secret='GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ',enrollment={secret,uri:`otpauth://totp/ERPX:synthetic-user?secret=${secret}&issuer=ERPX&algorithm=SHA1&digits=6&period=30`};
 const fetcher=vi.fn<(input:string,init:RequestInit)=>Promise<Response>>(async()=>response({data:enrollment}));const client=createAccountSecurityHttpPort(fetcher);
 expect(await client.enroll()).toEqual(enrollment);expect(fetcher.mock.calls[0]).toEqual(['/api/v1/account/iam/mfa/enroll',expect.objectContaining({method:'POST',credentials:'include',cache:'no-store',redirect:'error',body:'{}'})]);
 fetcher.mockImplementation(async()=>response({data:{verifiedUntil:1800000300000}}));expect(await client.verify({code:'123456'})).toEqual({verifiedUntil:1800000300000});expect(fetcher.mock.calls[1][0]).toBe('/api/v1/account/iam/mfa/verify');expect(fetcher.mock.calls[1][1].body).toBe('{"code":"123456"}');
});
it('rejects MFA bearer extensions, unsafe deadlines and modified authenticator parameters',async()=>{
 const secret='GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ',uri=`otpauth://totp/ERPX:synthetic-user?secret=${secret}&issuer=ERPX&algorithm=SHA1&digits=6&period=30`;
 const fetcher=vi.fn<(input:string,init:RequestInit)=>Promise<Response>>();const client=createAccountSecurityHttpPort(fetcher);
 for(const value of [{data:{secret,uri,token:'private'}},{data:{secret,uri:uri.replace('period=30','period=60')}},{data:{secret,uri},sessionToken:'private'},{data:{secret,uri:'https://foreign.example.test'}}]){fetcher.mockResolvedValueOnce(response(value));await expect(client.enroll()).rejects.toThrow('contract');}
 for(const value of [{data:{verifiedUntil:Number.MAX_SAFE_INTEGER+1}},{data:{verifiedUntil:1800000300000,sessionId:'private'}}]){fetcher.mockResolvedValueOnce(response(value));await expect(client.verify({code:'123456'})).rejects.toThrow('contract');}
});
it('reads only closed personal inventory and preserves MFA/throttle failures without fabricating a successful result',async()=>{
 const seed={accountKey:'a'.repeat(64),revision:'b'.repeat(64),mfaEnabled:true,mfaFreshUntil:null,families:[],legacySessions:0};
 const fetcher=vi.fn<(input:string,init:RequestInit)=>Promise<Response>>(async()=>response({data:seed}));const client=createAccountSecurityHttpPort(fetcher);expect(await client.read()).toEqual(seed);expect(fetcher.mock.calls[0]).toEqual(['/api/v1/account/security',expect.objectContaining({method:'GET',cache:'no-store',credentials:'include'})]);expect(fetcher.mock.calls[0][1].body).toBeUndefined();
 fetcher.mockResolvedValueOnce(response({data:{...seed,userId:'foreign'}}));await expect(client.read()).rejects.toThrow('contract');
 fetcher.mockResolvedValueOnce(response({error:{code:'IAM_RATE_LIMIT',message:'Wait five minutes.'}},429));await expect(client.verify({code:'bad'})).rejects.toThrow('Wait five minutes.');
 fetcher.mockRejectedValueOnce(Error('Offline'));await expect(client.enroll()).rejects.toThrow('Offline');
});

it("keeps invitation inspection and activation behind exact typed envelopes without accepting credential extensions", async () => {
  const fetcher = vi.fn<(input: string, init: RequestInit) => Promise<Response>>(async () => response({ data: { name: user.name, email: user.email, existingAccount: false } }));
  const client = createAccountAuthHttpPort(fetcher);
  expect((await client.invitations.inspect("synthetic proof/?&")).data?.data.email).toBe(user.email);
  expect(fetcher.mock.calls[0][0]).toBe("/api/v1/account-invitations?token=synthetic%20proof%2F%3F%26");
  fetcher.mockImplementation(async () => response({ data: { activated: true } }));
  expect((await client.invitations.accept({ token: "synthetic-proof", password: "synthetic-password" })).data?.data.activated).toBe(true);
  expect(JSON.parse(String(fetcher.mock.calls[1][1].body))).toEqual({ token: "synthetic-proof", password: "synthetic-password" });
  for (const value of [{ data: { activated: false } }, { data: { activated: true, token: "private" } }, { activated: true }]) {
    fetcher.mockImplementation(async () => response(value));
    expect((await client.invitations.accept({ token: "synthetic-proof", password: "synthetic-password" })).error?.code).toBe("ACCOUNT_RESPONSE_INVALID");
  }
});

it("preserves email/reset/verification call shapes with explicit credentialed HTTP requests", async () => {
  const fetcher = vi.fn<(input: string, init: RequestInit) => Promise<Response>>(async () => response({ status: true }));
  const client = createAccountAuthHttpPort(fetcher);
  await client.signUp.email({ name: user.name, email: user.email, password: "synthetic-password", callbackURL: "/onboarding" });
  await client.requestPasswordReset({ email: user.email, redirectTo: "/reset-password" });
  await client.resetPassword({ newPassword: "synthetic-password", token: "synthetic-reset-proof" });
  await client.sendVerificationEmail({ email: user.email, callbackURL: "/" });
  await client.signOut();
  expect(fetcher.mock.calls.map(call => call[0])).toEqual(["/api/auth/sign-up/email", "/api/auth/request-password-reset", "/api/auth/reset-password", "/api/auth/send-verification-email", "/api/auth/sign-out"]);
  for (const [, init] of fetcher.mock.calls) expect(init).toMatchObject({ method: "POST", credentials: "include", cache: "no-store" });
  expect(JSON.parse(String(fetcher.mock.calls[0][1].body))).toEqual({ name: user.name, email: user.email, password: "synthetic-password", callbackURL: "/onboarding" });
});

it("accepts the Rust sign-in user contract and rejects credential-shaped response extensions", async () => {
  const fetcher = vi.fn<(input: string, init: RequestInit) => Promise<Response>>(async () => response({ user }));
  const client = createAccountAuthHttpPort(fetcher);
  expect(await client.signIn.email({ email: user.email, password: "synthetic-password", revision: 12, rememberMe: false })).toEqual({ data: { user }, error: null });
  expect(JSON.parse(String(fetcher.mock.calls[0][1].body))).toEqual({ email: user.email, password: "synthetic-password", revision: 12, rememberMe: false });
  fetcher.mockImplementation(async () => response({ user, token: "synthetic-private-proof" }));
  expect((await client.signIn.email({ email: user.email, password: "synthetic-password" })).error?.code).toBe("ACCOUNT_RESPONSE_INVALID");
});

it("selects only opaque saved handles with the captured revision and keeps stale-selection errors", async () => {
  const fetcher = vi.fn<(input: string, init: RequestInit) => Promise<Response>>(async () => response({ data: devices }));
  const client = createAccountAuthHttpPort(fetcher);
  expect((await client.multiSession.listDeviceSessions()).data).toEqual(devices);
  expect((await client.multiSession.setActive({ handle, revision: 12 })).data).toEqual(devices);
  expect(fetcher.mock.calls[1][0]).toBe("/api/v1/account/device-accounts/active");
  expect(JSON.parse(String(fetcher.mock.calls[1][1].body))).toEqual({ handle, revision: 12 });
  fetcher.mockImplementation(async () => response({ error: { code: "DEVICE_ACCOUNTS_CHANGED", message: "Review current accounts." } }, 409));
  expect(await client.multiSession.setActive({ handle, revision: 12 })).toEqual({ data: null, error: { code: "DEVICE_ACCOUNTS_CHANGED", message: "Review current accounts." } });
});

it("rejects lossy revisions before sending commands and rejects unsafe revisions from the server", async () => {
  const fetcher = vi.fn<(input: string, init: RequestInit) => Promise<Response>>(async () => response({ data: devices }));
  const client = createAccountAuthHttpPort(fetcher);
  for (const revision of [-1, 1.5, Number.MAX_SAFE_INTEGER + 1, Number.POSITIVE_INFINITY]) {
    expect((await client.multiSession.setActive({ handle, revision })).error?.code).toBe("INVALID_DEVICE_SELECTION");
    expect((await client.signIn.email({ email: user.email, password: "synthetic-password", revision })).error?.code).toBe("INVALID_DEVICE_REVISION");
  }
  expect(fetcher).not.toHaveBeenCalled();
  fetcher.mockImplementation(async () => response({ data: { ...devices, revision: Number.MAX_SAFE_INTEGER + 1 } }));
  expect((await client.multiSession.listDeviceSessions()).error?.code).toBe("ACCOUNT_RESPONSE_INVALID");
});

it("rejects raw-token device rows, duplicate handles and multiple active accounts", async () => {
  const fetcher = vi.fn<(input: string, init: RequestInit) => Promise<Response>>(async () => response({ data: { ...devices, accounts: [{ ...devices.accounts[0], token: "synthetic-private-proof" }] } }));
  const client = createAccountAuthHttpPort(fetcher);
  expect((await client.multiSession.listDeviceSessions()).error?.code).toBe("ACCOUNT_RESPONSE_INVALID");
  fetcher.mockImplementation(async () => response({ data: { ...devices, accounts: [devices.accounts[0], devices.accounts[0]] } }));
  expect((await client.multiSession.listDeviceSessions()).error?.code).toBe("ACCOUNT_RESPONSE_INVALID");
});

it("accepts nullable sessions and exact string timestamps without a bearer token", async () => {
  const fetcher = vi.fn<(input: string, init: RequestInit) => Promise<Response>>(async () => response(null));
  const client = createAccountAuthHttpPort(fetcher);
  expect(await client.getSession()).toEqual({ data: null, error: null });
  const session = { id: "synthetic-session", userId: user.id, expiresAt: "2027-01-15T20:00:00Z", createdAt: "2027-01-15T08:00:00Z", updatedAt: "2027-01-15T08:00:00Z" };
  fetcher.mockImplementation(async () => response({ user, session }));
  expect((await client.getSession()).data).toEqual({ user, session });
  fetcher.mockImplementation(async () => response({ user, session: { ...session, token: "synthetic-private-proof" } }));
  expect((await client.getSession()).error?.code).toBe("ACCOUNT_RESPONSE_INVALID");
});

it("reports unavailable or malformed HTTP responses without fabricating authentication success", async () => {
  const fetcher = vi.fn<(input: string, init: RequestInit) => Promise<Response>>(async (): Promise<Response> => { throw new Error("synthetic transport unavailable"); });
  const client = createAccountAuthHttpPort(fetcher);
  expect((await client.signOut()).error?.code).toBe("ACCOUNT_NETWORK_UNAVAILABLE");
  fetcher.mockImplementation(async () => new Response("Not found.", { status: 404 }));
  expect((await client.signUp.email({ name: user.name, email: user.email, password: "synthetic-password" })).data).toBeNull();
  fetcher.mockImplementation(async () => response({ status: false }));
  expect((await client.signOut()).error?.code).toBe("ACCOUNT_RESPONSE_INVALID");
});

it("uses typed verification and password-change contracts without a client authority field", async () => {
  const fetcher = vi.fn<(input: string, init: RequestInit) => Promise<Response>>(async () => response({ user }));
  const client = createAccountAuthHttpPort(fetcher);
  expect((await client.verifyEmail({ token: "synthetic-email-proof", revision: 12, callbackURL: "/onboarding" })).data).toEqual({ user });
  expect(fetcher.mock.calls[0][0]).toBe("/api/auth/verify-email");
  expect(JSON.parse(String(fetcher.mock.calls[0][1].body))).toEqual({ token: "synthetic-email-proof", revision: 12, callbackURL: "/onboarding" });
  fetcher.mockImplementation(async () => response({ status: true }));
  expect((await client.changePassword({ currentPassword: "current-synthetic-password", newPassword: "new-synthetic-password" })).error).toBeNull();
  expect(fetcher.mock.calls[1][0]).toBe("/api/auth/change-password");
  expect(JSON.parse(String(fetcher.mock.calls[1][1].body))).toEqual({ currentPassword: "current-synthetic-password", newPassword: "new-synthetic-password" });
});

it("requires the existing device facade envelope and unwraps only its validated display data", async () => {
  const fetcher = vi.fn<(input: string, init: RequestInit) => Promise<Response>>(async () => response(devices));
  const client = createAccountAuthHttpPort(fetcher);
  expect((await client.multiSession.listDeviceSessions()).error?.code).toBe("ACCOUNT_RESPONSE_INVALID");
  for (const value of [{ data: devices, token: "synthetic-private-proof" }, { data: { ...devices, token: "synthetic-private-proof" } }, { data: null }]) {
    fetcher.mockImplementation(async () => response(value));
    expect((await client.multiSession.listDeviceSessions()).error?.code).toBe("ACCOUNT_RESPONSE_INVALID");
  }
  fetcher.mockImplementation(async () => response({ data: devices }));
  expect(await client.multiSession.listDeviceSessions()).toEqual({ data: devices, error: null });
});

it("carries the read-only lost-group revision into initialization without accepting bearer or authority metadata", async () => {
  const recovery = { ...devices, initialized: false, revision: 13 };
  const fetcher = vi.fn<(input: string, init: RequestInit) => Promise<Response>>(async () => response({ data: recovery }));
  const client = createAccountAuthHttpPort(fetcher);
  expect((await client.multiSession.listDeviceSessions()).data).toEqual(recovery);
  fetcher.mockImplementation(async () => response({ data: { ...recovery, initialized: true, revision: 14 } }));
  expect((await client.multiSession.initialize({ revision: recovery.revision })).data?.revision).toBe(14);
  expect(fetcher.mock.calls[1][0]).toBe("/api/v1/account/device-accounts/initialize");
  expect(fetcher.mock.calls[1][1]).toMatchObject({ method: "POST", credentials: "include", cache: "no-store", redirect: "error" });
  expect(JSON.parse(String(fetcher.mock.calls[1][1].body))).toEqual({ revision: 13 });
  for (const revision of [-1, 1.5, Number.MAX_SAFE_INTEGER + 1, Number.POSITIVE_INFINITY]) expect((await client.multiSession.initialize({ revision })).error?.code).toBe("INVALID_DEVICE_REVISION");
  expect(fetcher).toHaveBeenCalledTimes(2);
  fetcher.mockImplementation(async () => response({ data: { ...devices, rawGroup: "synthetic-private-proof" } }));
  expect((await client.multiSession.initialize({ revision: 13 })).error?.code).toBe("ACCOUNT_RESPONSE_INVALID");
});

it('uses the typed Directory wire without adopting response credentials or hiding native throttle and CAS failures',async()=>{
 const {createAccountDirectoryHttpPort}=await import('@erpx/feature-identity_access/public/client');
 const fetcher=vi.fn<(input:string,init:RequestInit)=>Promise<Response>>(async()=>response({success:true}));const directory=createAccountDirectoryHttpPort(fetcher);
 const input={connectionId:'11111111-1111-4111-8111-111111111111',username:'alice',password:'synthetic-directory-only',revision:12};expect(await directory.signIn(input)).toEqual({data:{success:true},error:null});expect(fetcher.mock.calls[0]).toEqual(['/api/auth/directory/sign-in',expect.objectContaining({method:'POST',credentials:'include',cache:'no-store',redirect:'error',body:JSON.stringify(input)})]);
 for(const value of [{success:true,token:'untrusted'},{success:false},{data:{success:true}},null]){fetcher.mockResolvedValueOnce(response(value));expect((await directory.signIn(input)).error?.code).toBe('ACCOUNT_RESPONSE_INVALID');}
 for(const [status,code]of [[429,'IAM_RATE_LIMIT'],[409,'DEVICE_ACCOUNTS_CHANGED']]as const){fetcher.mockResolvedValueOnce(response({error:{code,message:'Actual current denial'}},status));expect((await directory.signIn(input)).error).toEqual({code,message:'Actual current denial'});}
 const calls=fetcher.mock.calls.length;expect((await directory.signIn({...input,revision:Number.MAX_SAFE_INTEGER+1})).error?.code).toBe('INVALID_DEVICE_REVISION');expect(fetcher.mock.calls).toHaveLength(calls);
});
