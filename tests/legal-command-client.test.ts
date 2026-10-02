import { afterEach, beforeEach, expect, it, vi } from "vitest";
const { workFetch } = vi.hoisted(() => ({ workFetch: vi.fn() }));
vi.mock("../src/lib/work-fetch", () => ({ workFetch }));
import { createLegalCommandClient } from "../src/lib/legal/command-client";

const path = "/api/v1/holdings/acme/legal";
const input = { action: "create", title: "Contract question", metadata: { subject: "Thai text ไทย" } };
let storage: Map<string, string>;
beforeEach(() => {
  storage = new Map();
  vi.stubGlobal("sessionStorage", {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
    removeItem: (key: string) => storage.delete(key),
  });
  workFetch.mockReset();
});
afterEach(() => vi.unstubAllGlobals());

it("recovers a committed command whose response was lost using the exact saved body and key after refresh", async () => {
  workFetch.mockRejectedValueOnce(new TypeError("connection lost after commit"));
  const first = createLegalCommandClient("company-a:command");
  await expect(first(path, input)).rejects.toThrow("connection lost");
  const original = workFetch.mock.calls[0][1];
  const refreshed = createLegalCommandClient("company-a:command");
  expect(refreshed.hasPending()).toBe(true);
  workFetch.mockResolvedValueOnce(Response.json({ data: { id: "committed-item", replayed: true } }));
  await expect(refreshed.recover()).resolves.toEqual({ input, data: { id: "committed-item", replayed: true } });
  expect(workFetch.mock.calls[1]).toEqual([path, original]);
  expect(refreshed.hasPending()).toBe(false);
  expect(storage.size).toBe(0);
});

it("blocks changed details or another action while the result is uncertain", async () => {
  workFetch.mockRejectedValueOnce(new TypeError("disconnected"));
  const send = createLegalCommandClient("company-a:command");
  await expect(send(path, input)).rejects.toThrow("disconnected");
  await expect(send(path, { ...input, title: "Changed" })).rejects.toThrow("uncertain result");
  await expect(send(`${path}/release-acceptance`, input)).rejects.toThrow("uncertain result");
  expect(workFetch).toHaveBeenCalledTimes(1);
});

it.each([408, 429, 500, 503])("retains the original command for recovery after HTTP %s", async status => {
  workFetch.mockResolvedValueOnce(Response.json({ error: { message: "Try later" } }, { status }));
  const send = createLegalCommandClient("company-a:command");
  await expect(send(path, input)).rejects.toThrow("Try later");
  expect(send.hasPending()).toBe(true);
  expect(storage.size).toBe(1);
});

it.each([401, 403, 409, 422])("clears a definitively rejected HTTP %s even if a proxy returns HTML", async status => {
  workFetch.mockResolvedValueOnce(new Response("<html>Rejected</html>", { status }));
  const send = createLegalCommandClient("company-a:command");
  await expect(send(path, input)).rejects.toThrow("request was rejected");
  expect(send.hasPending()).toBe(false);
  workFetch.mockResolvedValueOnce(Response.json({ data: { id: "new-item" } }));
  await expect(send(path, { ...input, title: "Reviewed details" })).resolves.toEqual({ id: "new-item" });
});

it("retains an unreadable successful response because the command may have committed", async () => {
  workFetch.mockResolvedValueOnce(new Response("truncated", { status: 200 }));
  const send = createLegalCommandClient("company-a:command");
  await expect(send(path, input)).rejects.toThrow("recover this action");
  expect(send.hasPending()).toBe(true);
});

it("prevents a second in-flight request from reaching the server", async () => {
  let finish!: (response: Response) => void;
  workFetch.mockReturnValueOnce(new Promise<Response>(resolve => { finish = resolve; }));
  const send = createLegalCommandClient("company-a:command");
  const pending = send(path, input);
  await expect(send(path, input)).rejects.toThrow("Wait for the current");
  expect(workFetch).toHaveBeenCalledTimes(1);
  finish(Response.json({ data: { id: "one-item" } }));
  await expect(pending).resolves.toEqual({ id: "one-item" });
});

it("keeps recovery isolated to the saved work context", async () => {
  workFetch.mockRejectedValueOnce(new TypeError("disconnected"));
  await expect(createLegalCommandClient("company-a:command")(path, input)).rejects.toThrow();
  const other = createLegalCommandClient("company-b:command");
  expect(other.hasPending()).toBe(false);
  await expect(other.recover()).resolves.toBeNull();
  expect(workFetch).toHaveBeenCalledTimes(1);
});
