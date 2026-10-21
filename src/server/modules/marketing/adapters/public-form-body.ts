import { ApiError } from "../../runtime/public/http";

const PUBLIC_MARKETING_BODY_LIMIT = 32 * 1024;

type PublicMarketingBodyRequest = Pick<Request, "headers" | "body">;

function tooLarge() {
  return new ApiError(413, "BODY_TOO_LARGE", "Form submission is too large.");
}

/** Read anonymous form JSON without buffering an unbounded streamed request. */
export async function readBoundedPublicMarketingBody(request: PublicMarketingBodyRequest): Promise<string> {
  const declaredLength = request.headers.get("content-length");
  if (declaredLength !== null && /^\d+$/.test(declaredLength) && BigInt(declaredLength) > BigInt(PUBLIC_MARKETING_BODY_LIMIT)) {
    await request.body?.cancel().catch(() => undefined);
    throw tooLarge();
  }

  const reader = request.body?.getReader();
  if (!reader) return "";

  const chunks: Uint8Array[] = [];
  let size = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      if (!value) continue;
      size += value.byteLength;
      if (size > PUBLIC_MARKETING_BODY_LIMIT) {
        await reader.cancel().catch(() => undefined);
        throw tooLarge();
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
    offset += chunk.byteLength;
  }
  let text: string;
  try { text = new TextDecoder("utf-8", { fatal: true }).decode(bytes); }
  catch { throw new ApiError(422, "INVALID_ENCODING", "Form submission must contain valid UTF-8."); }
  if (Buffer.byteLength(text) > PUBLIC_MARKETING_BODY_LIMIT) throw tooLarge();
  return text;
}
