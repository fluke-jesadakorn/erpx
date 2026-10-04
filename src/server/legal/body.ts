import { ApiError } from "../http";
export async function boundedBody(request: Request, limit: number) {
  const declared = request.headers.get("content-length");
  if (declared !== null && /^\d+$/.test(declared) && BigInt(declared) > BigInt(limit)) {
    await request.body?.cancel();
    throw new ApiError(413, "BODY_TOO_LARGE", `Request exceeds ${Math.round(limit / 1024)} KB.`);
  }
  const reader = request.body?.getReader();
  if (!reader) return new Uint8Array();
  const chunks: Uint8Array[] = [];
  let size = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > limit) {
        await reader.cancel();
        throw new ApiError(
          413,
          "BODY_TOO_LARGE",
          `Request exceeds ${Math.round(limit / 1024)} KB.`,
        );
      }
      chunks.push(value);
    }
  } finally {
    reader.releaseLock();
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  return bytes;
}
