import { afterEach, expect, it, vi } from "vitest";
import { createElement, type ComponentProps } from "react";
import type { Dialog } from "@erpx/ui/components/dialog";
import { renderToStaticMarkup } from "react-dom/server";
import { createMentionRecordRead, mentionRecordEndpoint } from "@erpx/feature-records/lib/mention-read";
import { SocialTextBody } from "@erpx/feature-collaboration/components/social-text-body";

const observedMention = vi.hoisted(() => ({ dialogs: [] as ComponentProps<typeof Dialog>[] }));
vi.mock("@erpx/ui/components/dialog", async importOriginal => {
  const actual = await importOriginal<typeof import("@erpx/ui/components/dialog")>();
  const { createElement } = await import("react");
  return { ...actual, Dialog: function ObservedDialog(props: ComponentProps<typeof actual.Dialog>) {
    observedMention.dialogs.push(props);
    return createElement(actual.Dialog, props);
  } };
});
afterEach(() => { observedMention.dialogs.length = 0; });

const retained = { title: "Original source", description: "Saved description", status: "approved", version: 4,
  fields: [{ key: "amount", label: "Amount", type: "number", value: 42, privateUnused: "not display" }],
  createdBy: "private-user", assignment: "private-assignment", sources: { private: true } };
const response = () => Response.json({ data: { record: retained, suggestions: [{ private: true }], history: [{ private: true }] } });

it("opens retained records in the selected Company/Branch/Department API without guessing a native destination", () => {
  expect(mentionRecordEndpoint("/holdings/acme/branches/b/departments/d", "finance/original/id")).toBe("/api/v1/holdings/acme/branches/b/departments/d/records/finance/original%2Fid?view=mention");
  for (const [base, id] of [["https://foreign.test/holdings/acme", "finance/id"], ["/holdings/acme/home", "finance/id"], ["/holdings/acme", "finance/"], ["/holdings/acme", "id"]]) expect(mentionRecordEndpoint(base, id)).toBeNull();
});

it("fetches only on an explicit read and returns the exact display allowlist", async () => {
  const fetcher = vi.fn(async () => response());
  const read = createMentionRecordRead(fetcher, () => 1);
  expect(fetcher).not.toHaveBeenCalled();
  expect(await read.read("/holdings/acme", "finance/id")).toEqual({ title: retained.title, description: retained.description, status: "approved", version: 4, fields: [{ key: "amount", label: "Amount", value: 42 }] });
  expect(fetcher).toHaveBeenCalledTimes(1);
  expect(fetcher.mock.calls[0]).toMatchObject(["/api/v1/holdings/acme/records/finance/id?view=mention", { cache: "no-store" }]);
});

it("discards a dismissed response even when the transport ignores abort", async () => {
  let finish!: (value: Response) => void;
  const fetcher = vi.fn(() => new Promise<Response>(resolve => { finish = resolve; }));
  const reader = createMentionRecordRead(fetcher, () => 1);
  const request = reader.read("/holdings/acme", "finance/id");
  reader.cancel(); finish(response());
  expect(await request).toBeNull();
});

it("withholds old scope bytes if context changes during JSON parsing", async () => {
  let generation = 1, finish!: (value: unknown) => void;
  const parsing = new Promise(resolve => { finish = resolve; });
  const reader = createMentionRecordRead(async () => ({ ok: true, json: () => parsing }) as Response, () => generation);
  const request = reader.read("/holdings/acme", "finance/id");
  await Promise.resolve(); generation++;
  finish({ data: { record: retained } });
  expect(await request).toBeNull();
});

it("keeps a replacement record when an earlier read finishes last", async () => {
  const responses: ((value: Response) => void)[] = [];
  const reader = createMentionRecordRead(() => new Promise(resolve => responses.push(resolve)), () => 1);
  const first = reader.read("/holdings/acme", "finance/first");
  const second = reader.read("/holdings/acme", "finance/second");
  responses[1](response()); expect((await second)?.title).toBe("Original source");
  responses[0](response()); expect(await first).toBeNull();
});

it("preserves current owner refusal and does not display malformed success data", async () => {
  await expect(createMentionRecordRead(async () => Response.json({ error: { message: "Change your role" } }, { status: 403 }), () => 1).read("/holdings/acme", "finance/id")).rejects.toThrow("Change your role");
  await expect(createMentionRecordRead(async () => Response.json({ data: { record: { ...retained, fields: [{ key: "x", label: "X", value: { secret: true } }] } } }), () => 1).read("/holdings/acme", "finance/id")).rejects.toThrow("record response is unavailable");
});

it("renders both plain and rich record mentions as explicit detail actions with no mount request or retired href", () => {
  const fetcher = vi.spyOn(globalThis, "fetch");
  try {
    const plain = renderToStaticMarkup(createElement(SocialTextBody, { appearance: "standard", base: "/holdings/acme", body: "@Source", mentions: [{ kind: "record", targetId: "finance/id", label: "Source", start: 0, end: 7 }] }));
    const rich = renderToStaticMarkup(createElement(SocialTextBody, { appearance: "standard", base: "/holdings/acme", body: "@Source", doc: { type: "doc", content: [{ type: "paragraph", content: [{ type: "text", text: "@Source", marks: [{ type: "mention", attrs: { kind: "record", targetId: "finance/id", label: "Source" } }] }] }] } }));
    for (const markup of [plain, rich]) { expect(markup).toContain("@Source</button>"); expect(markup).not.toContain("/records/"); expect(markup).not.toContain("Person ·"); }
    expect(fetcher).not.toHaveBeenCalled();
  } finally { fetcher.mockRestore(); }
});


it.each([
  ["legacy", "plain"], ["legacy", "rich"], ["standard", "plain"], ["standard", "rich"],
] as const)("keeps the actual %s %s record detail action and drawer in the same appearance", (appearance, format) => {
  const fetcher = vi.spyOn(globalThis, "fetch");
  try {
    // The real PMS caller omits appearance; Office explicitly requests standard.
    const props: ComponentProps<typeof SocialTextBody> = {
      ...(appearance === "standard" ? { appearance } : {}), base: "/holdings/acme", body: "@Source",
      ...(format === "plain" ? {
        mentions: [{ kind: "record", targetId: "finance/id", label: "Source", start: 0, end: 7 }],
      } : {
        doc: { type: "doc", content: [{ type: "paragraph", content: [{ type: "text", text: "@Source",
          marks: [{ type: "mention", attrs: { kind: "record", targetId: "finance/id", label: "Source" } }],
        }] }] },
      }),
    };
    const html = renderToStaticMarkup(createElement(SocialTextBody, props));
    expect(html).toContain("@Source</button>");
    expect(html).toContain(`data-appearance="${appearance}"`);
    expect(html).not.toContain("/records/");
    expect(html.includes("ui-standard-control")).toBe(appearance === "standard");
    expect(observedMention.dialogs).toHaveLength(1);
    const detail = observedMention.dialogs[0];
    expect(detail.appearance).toBe(appearance);
    expect(detail.presentation).toBe("form-drawer");
    expect(detail.workspaceRole).toBe("detail");
    expect(detail.title).toBe("Source");
    expect(detail.open).toBe(false);
    expect(fetcher).not.toHaveBeenCalled();
  } finally { fetcher.mockRestore(); }
});
