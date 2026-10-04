import { expect, it } from "vitest";
import { boundedBody } from "../src/server/legal/body";

function streamed(chunks: Uint8Array[], declared?: string) {
  const state = { reads: 0, cancelled: false };
  const stream = new ReadableStream<Uint8Array>({
    pull(controller) {
      const chunk = chunks[state.reads++];
      if (chunk) controller.enqueue(chunk); else controller.close();
    },
    cancel() { state.cancelled = true; },
  }, { highWaterMark: 0 });
  const request = new Request("http://localhost:3100/api/v1/holdings/test/legal/upload/test", {
    method: "POST", headers: declared === undefined ? {} : { "Content-Length": declared }, body: stream, duplex: "half",
  } as RequestInit);
  return { request, state };
}

it("rejects a declared oversized Legal upload before reading a possibly truncated body", async () => {
  const { request, state } = streamed([new Uint8Array(1024)], String(10 * 1024 * 1024 + 1));
  await expect(boundedBody(request, 10 * 1024 * 1024)).rejects.toMatchObject({ status: 413, code: "BODY_TOO_LARGE" });
  expect(state.reads).toBe(0); expect(state.cancelled).toBe(true); expect(request.body!.locked).toBe(false);
});
it.each([undefined, "1", "invalid", "9007199254740993"])("enforces the actual Legal body bound for unknown or misdeclared length %s", async declared => {
  const { request, state } = streamed([new Uint8Array(3), new Uint8Array(3), new Uint8Array(3)], declared);
  await expect(boundedBody(request, 5)).rejects.toMatchObject({ status: 413, code: "BODY_TOO_LARGE" });
  expect(state.reads).toBe(declared === "9007199254740993" ? 0 : 2);
  expect(state.cancelled).toBe(true); expect(request.body!.locked).toBe(false);
});
it.each([undefined, "5"])("retains every byte at the exact Legal limit with declared length %s", async declared => {
  const { request, state } = streamed([Uint8Array.of(1, 2), Uint8Array.of(3, 4, 5)], declared);
  expect(await boundedBody(request, 5)).toEqual(Uint8Array.of(1, 2, 3, 4, 5));
  expect(state.cancelled).toBe(false); expect(request.body!.locked).toBe(false);
});
