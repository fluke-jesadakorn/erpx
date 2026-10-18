import {afterEach, expect, it, vi} from "vitest";
import {createRequire} from "node:module";
import {readFileSync} from "node:fs";
import * as React from "react";
import {renderToStaticMarkup} from "react-dom/server";
import {createGuestBookingChangeClient, guestBookingCheckoutContinuation, type GuestBookingChangeScope} from "@/lib/pms/guest-booking-change-command";
import {GuestBookingManagementWorkspace} from "@/components/pms/guest-booking-management-workspace";
import {GuestBookingShell} from "@/components/pms/guest-booking-shell";
import {AppearanceSwitcher} from "@/components/brand/appearance-switcher";

const source = (): GuestBookingChangeScope => ({base: "/api/public/booking/original", token: "original.private-token", epoch: 1, id: "original-reservation", number: "BK-001"});
const receipt = {id: "original-reservation", number: "BK-001", state: "confirmed", checkoutUrl: null};
const history = {...receipt, restored: true};
const reply = (data: unknown) => Response.json({data});
const delayed = (data: unknown, during: () => void) => {
  const response = reply(data), json = response.json.bind(response);
  response.json = async () => {during(); return json();}; return response;
};
afterEach(() => {vi.unstubAllGlobals(); vi.unstubAllEnvs();});

it("retains the exact original signed proposal, body and key after a lost confirmation and blocks a different change", async () => {
  const live = source(), send = vi.fn<typeof fetch>().mockRejectedValueOnce(new TypeError("Lost after commit")), key = vi.fn(() => "original-key");
  const commands = createGuestBookingChangeClient<typeof history>(() => live, send, key);
  await expect(commands.confirm("cancel", "reviewed-proposal", 700)).rejects.toThrow("Lost after commit");
  expect(commands.hasPending()).toBe(true);
  await expect(commands.confirm("change", "different-proposal", 0)).rejects.toThrow("Recover the original");
  expect(send).toHaveBeenCalledTimes(1);
  send.mockResolvedValueOnce(reply(receipt)).mockResolvedValueOnce(reply(history));
  const outcome = await commands.recover();
  expect(send.mock.calls[1]).toEqual(send.mock.calls[0]);
  expect(JSON.parse(String(send.mock.calls[1][1]?.body))).toEqual({proposalToken: "reviewed-proposal"});
  expect(send.mock.calls[0][1]?.headers).toEqual({"Content-Type": "application/json", Authorization: "Bearer original.private-token", "Idempotency-Key": "original-key"});
  expect(outcome.refund).toBe(700); expect(outcome.booking).toEqual(history);
  expect(commands.hasPending()).toBe(true); expect(outcome.acknowledge()).toBe(true); expect(commands.hasPending()).toBe(false); expect(key).toHaveBeenCalledTimes(1);
});
it("refreshes only history after the original confirmation was decoded but its required read failed", async () => {
  const send = vi.fn<typeof fetch>().mockResolvedValueOnce(reply(receipt)).mockRejectedValueOnce(Error("History unavailable"));
  const commands = createGuestBookingChangeClient<typeof history>(source, send);
  await expect(commands.confirm("change", "proposal", 0)).rejects.toThrow("History unavailable");
  expect(commands.needsHistory()).toBe(true);
  send.mockResolvedValueOnce(reply(history)); const outcome = await commands.recover(); expect(outcome.acknowledge()).toBe(true);
  expect(send.mock.calls.map(call => call[0])).toEqual(["/api/public/booking/original/change-confirm", "/api/public/booking/original/manage", "/api/public/booking/original/manage"]);
});
it.each(["confirmation", "history"])("rechecks the original guest authority after ORIGINAL %s JSON decoding", async phase => {
  const live = source(), send = vi.fn<typeof fetch>();
  if (phase === "confirmation") send.mockResolvedValueOnce(delayed(receipt, () => {live.token = "other-token";}));
  else send.mockResolvedValueOnce(reply(receipt)).mockResolvedValueOnce(delayed(history, () => {live.epoch++;}));
  const commands = createGuestBookingChangeClient<typeof history>(() => live, send);
  await expect(commands.confirm("cancel", "proposal", 100)).rejects.toMatchObject({name: "AbortError"});
  expect(commands.hasPending()).toBe(true);
  const count = send.mock.calls.length; await expect(commands.recover()).rejects.toMatchObject({name: "AbortError"}); expect(send).toHaveBeenCalledTimes(count);
});
it("keeps the original operation when later native rejection follows an uncertain commit", async () => {
  const send = vi.fn<typeof fetch>().mockRejectedValueOnce(Error("Unknown original outcome"));
  const commands = createGuestBookingChangeClient<typeof history>(source, send);
  await expect(commands.confirm("cancel", "proposal", 0)).rejects.toThrow("Unknown");
  send.mockResolvedValueOnce(Response.json({error: {code: "FORBIDDEN", message: "Authority expired"}}, {status: 403}));
  await expect(commands.recover()).rejects.toThrow("Authority expired"); expect(commands.hasPending()).toBe(true); expect(send.mock.calls[1]).toEqual(send.mock.calls[0]);
});
it.each([{status: 403, code: "PURCHASE_CAPABILITY_FORBIDDEN"}, {status: 409, code: "PMS_GUEST_EMAIL_REQUIRED"}])("retains the first native $status/$code because Checkout may reject after the confirmation transaction committed", async ({status, code}) => {
  const send = vi.fn<typeof fetch>().mockResolvedValueOnce(Response.json({error: {code, message: "Checkout cannot continue"}}, {status})), commands = createGuestBookingChangeClient<typeof history>(source, send, () => "committed-original-key");
  await expect(commands.confirm("change", "committed-original-proposal", 0)).rejects.toThrow("Checkout cannot continue");
  expect(commands.hasPending()).toBe(true); send.mockResolvedValueOnce(reply(receipt)).mockResolvedValueOnce(reply(history)); const recovered = await commands.recover();
  expect(send.mock.calls[1]).toEqual(send.mock.calls[0]); expect(recovered.acknowledge()).toBe(true);
});
it("allows a new reviewed proposal only after a fresh native command was definitively refused", async () => {
  const send = vi.fn<typeof fetch>().mockResolvedValueOnce(Response.json({error: {code: "PMS_PROPOSAL_STALE", message: "Review a new proposal"}}, {status: 409}));
  const key = vi.fn().mockReturnValueOnce("first-key").mockReturnValueOnce("second-key"), commands = createGuestBookingChangeClient<typeof history>(source, send, key);
  await expect(commands.confirm("cancel", "stale", 0)).rejects.toThrow("new proposal"); expect(commands.hasPending()).toBe(false);
  send.mockResolvedValueOnce(reply(receipt)).mockResolvedValueOnce(reply(history)); const result = await commands.confirm("cancel", "fresh", 0); expect(result.acknowledge()).toBe(true);
  expect((send.mock.calls[1][1]?.headers as Record<string, string>)["Idempotency-Key"]).toBe("second-key");
});
it.each([408, 429, 500, 503])("retains an original confirmation after HTTP %s", async status => {
  const send = vi.fn<typeof fetch>().mockResolvedValueOnce(Response.json({error: {code: "RETRY", message: "Try again"}}, {status})), commands = createGuestBookingChangeClient(source, send);
  await expect(commands.confirm("cancel", "proposal", 0)).rejects.toThrow("Try again"); expect(commands.hasPending()).toBe(true);
});
it.each(["unreadable", "foreign-booking", "missing-state"])("retains an uncertain %s confirmation instead of refreshing or clearing it", async kind => {
  const send = vi.fn<typeof fetch>().mockResolvedValueOnce(kind === "unreadable" ? new Response("truncated") : reply(kind === "foreign-booking" ? {...receipt, id: "other"} : {id: receipt.id, number: receipt.number}));
  const commands = createGuestBookingChangeClient(source, send); await expect(commands.confirm("change", "proposal", 0)).rejects.toBeInstanceOf(Error); expect(commands.hasPending()).toBe(true); expect(send).toHaveBeenCalledTimes(1);
});
it("prevents concurrent consumption and fences the explicit final acknowledgement", async () => {
  let release!: (response: Response) => void;
  const send = vi.fn<typeof fetch>().mockReturnValueOnce(new Promise(resolve => {release = resolve;})), live = source(), commands = createGuestBookingChangeClient<typeof history>(() => live, send);
  const first = commands.confirm("cancel", "proposal", 0); await expect(commands.recover()).rejects.toThrow("Wait"); expect(send).toHaveBeenCalledTimes(1);
  send.mockResolvedValueOnce(reply(history)); release(reply(receipt)); const outcome = await first;
  live.id = "other"; expect(() => outcome.acknowledge()).toThrow(DOMException); expect(commands.hasPending()).toBe(true);
});
it("does not let an older history result consume a newer recovery", async () => {
  const send = vi.fn<typeof fetch>().mockResolvedValueOnce(reply(receipt)).mockResolvedValueOnce(reply(history)), commands = createGuestBookingChangeClient<typeof history>(source, send);
  const old = await commands.confirm("change", "proposal", 0); send.mockResolvedValueOnce(reply(history)); const next = await commands.recover();
  expect(old.acknowledge()).toBe(false); expect(commands.hasPending()).toBe(true); expect(next.acknowledge()).toBe(true); expect(commands.hasPending()).toBe(false);
});
it("server-renders the real management entry without disclosing a guest snapshot or mounting a token read", () => {
  const send = vi.fn<typeof fetch>(); vi.stubGlobal("fetch", send);
  const html = renderToStaticMarkup(<GuestBookingManagementWorkspace slug="original"/>);
  expect(html).toContain("Find your booking"); expect(html).toContain('type="email"'); expect(html).toContain("Email a private link"); expect(html).not.toContain("original.private-token"); expect(html).not.toContain("BK-001"); expect(send).not.toHaveBeenCalled();
});
it("renders Booking language and theme labels together while preserving the shared switcher's original default", () => {
  const html = renderToStaticMarkup(<GuestBookingShell slug="original" initialLanguage="th" initialTheme="dark"><GuestBookingManagementWorkspace slug="original"/></GuestBookingShell>);
  expect(html).toContain("ค้นหาการจองของคุณ"); expect(html).toContain("เลขยืนยันการจอง"); expect(html).toContain("ตามระบบ"); expect(html).toContain("สว่าง"); expect(html).toContain("มืด"); expect(html).not.toContain(">Theme<");
  const shared = renderToStaticMarkup(<AppearanceSwitcher initialTheme="system"/>); expect(shared).toContain(">Theme<"); expect(shared).toContain(">System<");
});

/** Execute the actual mounted handler and hook state. Only rendering primitives
 * and browser plumbing are replaced; original fetch/Response decoding runs. */
function mountedManager(send: typeof fetch) {
  const require = createRequire(import.meta.url), ts = require("typescript") as typeof import("typescript"), states: unknown[] = [], effects = new Map<number, {deps?: unknown[]; cleanup?: () => void}>(), dirtyChecks = new Set<() => boolean>(); let cursor = 0;
  const useState = (initial: unknown) => {const i = cursor++; if (!(i in states)) states[i] = typeof initial === "function" ? initial() : initial; return [states[i], (value: unknown) => {states[i] = typeof value === "function" ? value(states[i]) : value;}];};
  const useRef = (initial: unknown) => {const [ref] = useState(() => ({current: initial})); return ref;};
  const effect = (run: () => void | (() => void), deps?: unknown[]) => {const i = cursor++, old = effects.get(i); if (!old || !deps || deps.some((v, index) => v !== old.deps?.[index])) {old?.cleanup?.(); effects.set(i, {deps, cleanup: run() || undefined});}};
  const primitives = new Proxy({}, {get: (_target, name) => (props: Record<string, unknown>) => React.createElement(({Button: "button", Field: "label", Select: "select", TextInput: "input", FormLayout: "form"} as Record<string, string>)[String(name)] ?? "div", props, props.children as React.ReactNode)});
  let forcedDirty = false;
  const context = {registerUnsavedWork: (check: () => boolean) => {dirtyChecks.add(check); return () => {dirtyChecks.delete(check);};}, contextIsSwitching: () => false, hasUnsavedWork: () => forcedDirty || [...dirtyChecks].some(check => check())};
  const evaluated = {exports: {} as Record<string, unknown>}, path = process.env.ERPX_GUEST_BOOKING_SOURCE ?? "src/components/pms/guest-booking-management-workspace.tsx";
  const localRequire = (name: string) => {
    if (name === "react") return {...React, useState, useRef, useEffect: effect, useLayoutEffect: effect};
    if (name === "react/jsx-runtime") return require(name);
    if (name.endsWith("/foundation")) return primitives;
    if (name.endsWith("/destination-registry")) return {canonicalUiHref: (href: string) => href};
    if (name.endsWith("/draft-form-drawer")) return {DraftFormDrawer: (props: Record<string, unknown>) => props.open ? React.createElement("aside", props, props.children as React.ReactNode) : null};
    if (name.endsWith("/pms-model")) return {money: (value: number) => String(value), plusDays: (date: string) => date};
    if (name === "@/lib/shared/front-language") return {frontText: (value: string) => value};
    if (name.endsWith("/front-language")) return {useFrontLanguage: () => "en", FrontLocalizedTree: ({children}: {children: React.ReactNode}) => children};
    if (name.endsWith("/guest-booking-change-command")) return {guestBookingCheckoutContinuation, createGuestBookingChangeClient: (current: () => GuestBookingChangeScope | null) => createGuestBookingChangeClient(current, send, () => "mounted-original-key")};
    if (name.endsWith("/context-switch")) return context;
    if (name.endsWith("booking-policy-summary")) return {BookingPolicySummary: () => null};
    if (name.endsWith("booking-quote-details-standard")) return {BookingQuoteDetailsStandard: () => null};
    if (name.endsWith("booking-flow")) return {QuoteDetails: () => null};
    if (name.endsWith(".css")) return {default: new Proxy({}, {get: (_target, key) => String(key)})};
    throw Error("Unexpected live dependency: " + name);
  };
  const code = ts.transpileModule(readFileSync(path, "utf8"), {compilerOptions: {jsx: ts.JsxEmit.ReactJSX, module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}}).outputText;
  new Function("require", "module", "exports", code)(localRequire, evaluated, evaluated.exports);
  const component = (evaluated.exports.GuestBookingManagementWorkspace ?? evaluated.exports.GuestBookingManager) as (props: {slug: string}) => React.ReactNode;
  type Node = React.ReactElement<Record<string, unknown>>;
  const collect = (value: React.ReactNode, result: Node[] = []): Node[] => {React.Children.forEach(value, child => {if (!React.isValidElement<Record<string, unknown>>(child)) return; result.push(child); if (typeof child.type === "function") collect((child.type as (props: Record<string, unknown>) => React.ReactNode)(child.props), result); else collect(child.props.children as React.ReactNode, result);}); return result;};
  return {render: () => {cursor = 0; return collect(component({slug: "original"}));}, renderRoot: () => {cursor = 0; return component({slug: "original"}) as Node;}, forceDirty: (value: boolean) => {forcedDirty = value;}, dirty: () => [...dirtyChecks].some(check => check()), close: () => effects.forEach(item => item.cleanup?.())};
}
it("keeps the actual management drawer and exact recovery visible after a lost confirmation instead of allowing Back to abandon it", async () => {
  const booking = {...history, hotel: {name: "Original hotel", address: "Address", contactPhone: "000", contactEmail: "hotel@example.test"}, guest: {name: "Original guest"}, currency: "THB", total: 10000, dueAtHotel: 0, purchases: {newWork: true, settleExisting: true}, pendingChange: null,
    rooms: [{id: "stay", roomName: "Room", arrival: "2026-11-01", departure: "2026-11-02", state: "reserved", typeId: "type", rateId: "rate", adults: 1, childAges: [], nights: [{total: 10000}]}], editableRates: [{id: "rate", typeId: "type", roomName: "Room", name: "Rate"}], refunds: [], documents: [], history: []};
  const proposal = {token: "reviewed-proposal", proposal: {items: [{stayId: "stay", oldTotal: 10000, dueAtHotel: 0}], fee: 0, dueNow: 0, refund: 10000}};
  const send = vi.fn<typeof fetch>().mockResolvedValueOnce(reply(booking)); vi.stubGlobal("fetch", send);
  vi.stubGlobal("window", {location: {hash: "#token=original.private-token", pathname: "/book/original/manage", search: ""}, addEventListener: vi.fn(), removeEventListener: vi.fn()});
  vi.stubGlobal("history", {replaceState: vi.fn()}); vi.stubGlobal("sessionStorage", {getItem: () => null, setItem: vi.fn()});
  const manager = mountedManager(send), flush = () => new Promise<void>(resolve => setImmediate(resolve));
  const text = (node: React.ReactElement<Record<string, unknown>>) => typeof node.props.children === "string" ? node.props.children : "";
  const click = (nodes: ReturnType<typeof manager.render>, label: string) => {const node = nodes.find(item => item.type === "button" && text(item) === label); if (!node) throw Error("Missing actual control: " + label); (node.props.onClick as () => void)();};
  try {
    manager.render(); await flush(); manager.render(); await flush(); let nodes = manager.render(); const room = nodes.find(item => item.type === "input" && String(item.props["aria-label"]).startsWith("Select Room"));
    expect(room).toBeDefined(); (room!.props.onChange as (event: unknown) => void)({target: {checked: true}}); nodes = manager.render(); click(nodes, "Cancel selected rooms"); nodes = manager.render();
    send.mockResolvedValueOnce(reply(proposal)); const form = nodes.find(item => item.type === "form" && !item.props.className); expect(form).toBeDefined(); (form!.props.onSubmit as (event: unknown) => void)({preventDefault() {}}); await flush(); nodes = manager.render();
    send.mockRejectedValueOnce(new TypeError("Lost after commit")); click(nodes, "Confirm changes"); await flush(); nodes = manager.render();
    const recover = nodes.find(item => item.type === "button" && text(item) === "Recover original confirmation"); expect(nodes.find(item => item.type === "button" && text(item) === "Back")?.props.disabled).toBe(true); expect(recover).toBeDefined(); expect(nodes.find(item => item.type === "aside")?.props.dismissDisabled).toBe(true); expect(manager.dirty()).toBe(true);
    click(nodes, "Close and keep confirmation"); nodes = manager.render(); expect(nodes.some(item => item.type === "aside")).toBe(false); expect(manager.dirty()).toBe(true);
    const retainedRecovery = nodes.find(item => item.type === "button" && text(item) === "Recover original confirmation"); expect(retainedRecovery).toBeDefined();
    const original = send.mock.calls[2]; send.mockResolvedValueOnce(reply(receipt)).mockResolvedValueOnce(reply(booking)); (retainedRecovery!.props.onClick as () => void)(); await flush(); nodes = manager.render();
    expect(send.mock.calls[3]).toEqual(original); expect(nodes.some(item => item.type === "aside")).toBe(false); expect(manager.dirty()).toBe(false); expect(send.mock.calls.filter(call => String(call[0]).endsWith("cancel-confirm"))).toHaveLength(2);
  } finally {manager.close();}
});
it("remounts the actual management owner for a new hash token while preserving query and other fragment state", () => {
  const listeners = new Map<string, () => void>(), location = {hash: "#token=first.private&view=rooms", pathname: "/book/original/manage", search: "?campaign=kept"}, stored = new Map<string, string>();
  const replaceState = vi.fn((_state: unknown, _unused: string, url: string) => {location.hash = url.includes("#") ? "#" + url.split("#")[1] : "";});
  vi.stubGlobal("window", {location, confirm: vi.fn(() => true), addEventListener: (name: string, run: () => void) => listeners.set(name, run), removeEventListener: (name: string) => listeners.delete(name)});
  vi.stubGlobal("history", {state: {preserved: true}, replaceState}); vi.stubGlobal("sessionStorage", {getItem: (name: string) => stored.get(name), setItem: (name: string, value: string) => stored.set(name, value)});
  const manager = mountedManager(vi.fn<typeof fetch>());
  try {
    manager.renderRoot(); const first = manager.renderRoot(); expect(first.props.accessToken).toBe("first.private"); expect(stored.get("booking-access:original")).toBe("first.private");
    expect(replaceState).toHaveBeenLastCalledWith({preserved: true}, "", "/book/original/manage?campaign=kept#view=rooms");
    location.hash = "#token=second.private&view=refunds"; listeners.get("hashchange")!(); const next = manager.renderRoot();
    expect(next.props.accessToken).toBe("second.private"); expect(next.key).not.toBe(first.key); expect(next.key).not.toContain("private"); expect(stored.get("booking-access:original")).toBe("second.private");
    expect(replaceState).toHaveBeenLastCalledWith({preserved: true}, "", "/book/original/manage?campaign=kept#view=refunds");
  } finally {manager.close();}
});
it("keeps the original token and owner when the user cancels a dirty hash-token handoff", () => {
  const listeners = new Map<string, () => void>(), location = {hash: "#token=first.private", pathname: "/book/original/manage", search: ""}, stored = new Map<string, string>(), confirm = vi.fn(() => false);
  vi.stubGlobal("window", {location, confirm, addEventListener: (name: string, run: () => void) => listeners.set(name, run), removeEventListener: (name: string) => listeners.delete(name)});
  vi.stubGlobal("history", {state: null, replaceState: vi.fn()}); vi.stubGlobal("sessionStorage", {getItem: (name: string) => stored.get(name), setItem: (name: string, value: string) => stored.set(name, value)});
  const manager = mountedManager(vi.fn<typeof fetch>());
  try {
    manager.renderRoot(); const first = manager.renderRoot(); manager.forceDirty(true); location.hash = "#token=other.private"; listeners.get("hashchange")!(); const retained = manager.renderRoot();
    expect(confirm).toHaveBeenCalledTimes(1); expect(retained.key).toBe(first.key); expect(retained.props.accessToken).toBe("first.private"); expect(stored.get("booking-access:original")).toBe("first.private");
    manager.forceDirty(false); location.hash = "#token=other.private"; listeners.get("hashchange")!(); const accepted = manager.renderRoot(); expect(accepted.props.accessToken).toBe("other.private"); expect(accepted.key).not.toBe(first.key);
  } finally {manager.close();}
});

it("offers native Checkout only while fresh booking history retains that exact pending payment", async () => {
  const {guestBookingCheckoutContinuation: continuation} = await import("../../../src/lib/pms/guest-booking-change-command");
  const payment = {checkoutUrl: "https://checkout.example.test/original", attemptToken: "original-payment"};
  expect(continuation(payment, {pendingChange: {attemptToken: "original-payment"}})).toBe(payment.checkoutUrl);
  expect(continuation(payment, {pendingChange: null})).toBeNull();
  expect(continuation(payment, {pendingChange: {attemptToken: "later-payment"}})).toBeNull();
  expect(continuation({checkoutUrl: payment.checkoutUrl}, {pendingChange: {attemptToken: "original-payment"}})).toBeNull();
  expect(continuation({...payment, checkoutUrl: null}, {pendingChange: {attemptToken: "original-payment"}})).toBeNull();
});

it.each([true, false])("the actual management controls remove a settled Checkout continuation, settled before confirm history=%s", async settledBefore => {
  const booking = {...history, hotel: {name: "Original hotel", address: "Address", contactPhone: "000", contactEmail: "hotel@example.test"}, guest: {name: "Original guest"}, currency: "THB", total: 10000, dueAtHotel: 0, purchases: {newWork: true, settleExisting: true}, pendingChange: null,
    rooms: [{id: "stay", roomName: "Room", arrival: "2026-11-01", departure: "2026-11-02", state: "reserved", typeId: "type", rateId: "rate", adults: 1, childAges: [], nights: [{total: 10000}]}], editableRates: [{id: "rate", typeId: "type", roomName: "Room", name: "Rate"}], refunds: [], documents: [], history: []};
  const proposal = {token: "reviewed-proposal", proposal: {items: [{stayId: "stay", oldTotal: 10000, dueAtHotel: 0}], fee: 0, dueNow: 1000, refund: 0}};
  const payment = {...receipt, checkoutUrl: "https://checkout.example.test/original", attemptToken: "original-payment"};
  const send = vi.fn<typeof fetch>().mockResolvedValueOnce(reply(booking)); vi.stubGlobal("fetch", send);
  vi.stubGlobal("window", {location: {hash: "#token=original.private-token", pathname: "/book/original/manage", search: ""}, addEventListener: vi.fn(), removeEventListener: vi.fn()});
  vi.stubGlobal("history", {replaceState: vi.fn()}); vi.stubGlobal("sessionStorage", {getItem: () => null, setItem: vi.fn()});
  const manager = mountedManager(send), flush = () => new Promise<void>(resolve => setImmediate(resolve));
  const click = (nodes: ReturnType<typeof manager.render>, label: string) => {const node = nodes.find(item => item.type === "button" && item.props.children === label); if (!node) throw Error("Missing actual control: " + label); (node.props.onClick as () => void)();};
  try {
    manager.render(); await flush(); manager.render(); await flush(); let nodes = manager.render();
    const room = nodes.find(item => item.type === "input" && String(item.props["aria-label"]).startsWith("Select Room")); expect(room).toBeDefined();
    (room!.props.onChange as (event: unknown) => void)({target: {checked: true}}); nodes = manager.render(); click(nodes, "Cancel selected rooms"); nodes = manager.render();
    send.mockResolvedValueOnce(reply(proposal)); const form = nodes.find(item => item.type === "form" && !item.props.className); expect(form).toBeDefined(); (form!.props.onSubmit as (event: unknown) => void)({preventDefault() {}}); await flush(); nodes = manager.render();
    send.mockResolvedValueOnce(reply(payment)).mockResolvedValueOnce(reply({...booking, pendingChange: settledBefore ? null : {attemptToken: payment.attemptToken, expiresAt: 1}}));
    click(nodes, "Confirm changes"); await flush(); nodes = manager.render();
    expect(nodes.some(item => item.props.href === payment.checkoutUrl)).toBe(!settledBefore);
    if (!settledBefore) {send.mockResolvedValueOnce(reply(booking)); click(nodes, "Refresh booking"); await flush(); nodes = manager.render();}
    expect(nodes.some(item => item.props.href === payment.checkoutUrl)).toBe(false);
    expect(send.mock.calls.filter(call => String(call[0]).endsWith("cancel-confirm"))).toHaveLength(1);
  } finally {manager.close();}
});
