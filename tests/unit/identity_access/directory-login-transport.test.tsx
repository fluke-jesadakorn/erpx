import { createElement, type ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { DirectoryLogin } from "@erpx/workplace/routes/login/directory-login";

type Submit = (event: { preventDefault(): void; currentTarget: HTMLFormElement }) => Promise<void>;
const transport = vi.hoisted(() => ({ push: vi.fn(), refresh: vi.fn(), assign: vi.fn(), fetch: vi.fn(), submit: undefined as Submit | undefined }));
vi.mock("next/navigation", () => ({ useRouter: () => ({ push: transport.push, refresh: transport.refresh }) }));
vi.mock("@erpx/ui/components/foundation", async importOriginal => {
  const actual = await importOriginal<typeof import("@erpx/ui/components/foundation")>();
  const { createElement, isValidElement } = await import("react");
  function findSubmit(node: ReactNode): Submit | undefined {
    if (Array.isArray(node)) { for (const child of node) { const found = findSubmit(child); if (found) return found; } return; }
    if (!isValidElement<{ children?: ReactNode; onSubmit?: Submit }>(node)) return;
    if (node.type === "form") return node.props.onSubmit;
    return findSubmit(node.props.children);
  }
  return { ...actual, Card: function ObservedCard(props: Parameters<typeof actual.Card>[0]) {
    transport.submit = findSubmit(props.children);
    return createElement(actual.Card, props);
  } };
});
beforeEach(() => {
  vi.clearAllMocks(); transport.submit = undefined;
  vi.stubGlobal("window", { location: { assign: transport.assign } });
  vi.stubGlobal("fetch", transport.fetch);
  // Only the browser FormData input is supplied; the real component, React hooks,
  // submit handler, response parser and destination branches execute unchanged.
  vi.stubGlobal("FormData", class FixtureFormData { get(name: string) { return name === "username" ? "fictional-directory-user" : "fictional-password"; } });
  transport.fetch.mockResolvedValue({ ok: true, json: async () => ({ success: true }) });
});
afterEach(() => vi.unstubAllGlobals());
async function submit(next: string, mode = "ldaps") {
  const html = renderToStaticMarkup(createElement(DirectoryLogin, { id: "directory-fixture", name: "Fictional corporate directory", mode, next }));
  expect(html).toContain("Fictional corporate directory");
  expect(transport.fetch).not.toHaveBeenCalled(); expect(transport.assign).not.toHaveBeenCalled();
  if (!transport.submit) throw new Error("Actual rendered DirectoryLogin form has no submit handler.");
  const preventDefault = vi.fn();
  await transport.submit({ preventDefault, currentTarget: {} as HTMLFormElement });
  expect(preventDefault).toHaveBeenCalledTimes(1);
}
it("completes successful LDAPS SSO with one document navigation and no RSC push or refresh", async () => {
  const next = "/sso/authorize?transaction=fictional-transaction&state=fictional-state&challenge=fictional-challenge";
  await submit(next);
  expect(transport.assign).toHaveBeenCalledExactlyOnceWith(next);
  expect(transport.push).not.toHaveBeenCalled(); expect(transport.refresh).not.toHaveBeenCalled();
  expect(transport.fetch).toHaveBeenCalledExactlyOnceWith("/api/auth/directory/sign-in", expect.objectContaining({ method: "POST", body: JSON.stringify({ connectionId: "directory-fixture", username: "fictional-directory-user", password: "fictional-password" }) }));
});
it.each(["/account/launch?next=%2Fsecurity%3Fwidget%3Daccount-security-controls", "/legal-portal/continue?holding=fictional-holding&share=fictional-share"])("retains a document navigation for the existing closed bridge %s", async next => {
  // Use the normal owner sanitizer to supply an accepted Legal enrollment URL.
  const { legalEnrollmentReturnPath } = await import("@erpx/server-identity_access/public/rules/public-surface-entry");
  const target = next.startsWith("/legal-portal/") ? legalEnrollmentReturnPath(next) : next;
  expect(target).toBe(next); if (!target) throw new Error("Expected valid owner bridge fixture");
  await submit(target); expect(transport.assign).toHaveBeenCalledExactlyOnceWith(target);
  expect(transport.push).not.toHaveBeenCalled(); expect(transport.refresh).not.toHaveBeenCalled();
});
it("retains ordinary same-product router navigation and refresh", async () => {
  await submit("/my-access?view=current");
  expect(transport.push).toHaveBeenCalledExactlyOnceWith("/my-access?view=current");
  expect(transport.refresh).toHaveBeenCalledTimes(1); expect(transport.assign).not.toHaveBeenCalled();
});
it("retains the actual OIDC provider redirect and callback payload", async () => {
  transport.fetch.mockResolvedValue({ ok: true, json: async () => ({ url: "https://fictional-idp.invalid/authorize" }) });
  const next = "/sso/authorize?transaction=fictional-transaction"; await submit(next, "oidc");
  expect(transport.fetch).toHaveBeenCalledExactlyOnceWith("/api/auth/sign-in/social", expect.objectContaining({ body: JSON.stringify({ provider: "directory-directory-fixture", callbackURL: next }) }));
  expect(transport.assign).toHaveBeenCalledExactlyOnceWith("https://fictional-idp.invalid/authorize");
  expect(transport.push).not.toHaveBeenCalled(); expect(transport.refresh).not.toHaveBeenCalled();
});
it.each(["rejected", "malformed"])("does not navigate after a %s directory response", async kind => {
  transport.fetch.mockResolvedValue(kind === "rejected" ? { ok: false, json: async () => ({ message: "Fictional rejected sign-in" }) } : { ok: true, json: async () => { throw new Error("Malformed response"); } });
  await submit("/sso/authorize?transaction=fictional-transaction");
  expect(transport.assign).not.toHaveBeenCalled(); expect(transport.push).not.toHaveBeenCalled(); expect(transport.refresh).not.toHaveBeenCalled();
});
