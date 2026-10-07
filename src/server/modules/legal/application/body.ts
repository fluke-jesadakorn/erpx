export type LegalBodyReader = { read(): Promise<{ done: boolean; value?: Uint8Array }>; cancel(): Promise<void>; releaseLock(): void };
export async function readBoundedLegalBody(source: { declaredLength: string | null; cancel(): Promise<void>; reader(): LegalBodyReader | null }, limit: number, error: (status: number, code: string, message: string) => Error) {
  const tooLarge = () => error(413, "BODY_TOO_LARGE", `Request exceeds ${Math.round(limit / 1024)} KB.`);
  if (source.declaredLength !== null && /^\d+$/.test(source.declaredLength) && BigInt(source.declaredLength) > BigInt(limit)) { await source.cancel(); throw tooLarge(); }
  const reader = source.reader(); if (!reader) return new Uint8Array();
  const chunks: Uint8Array[] = []; let size = 0;
  try { while (true) { const { done, value } = await reader.read(); if (done) break; size += value!.byteLength; if (size > limit) { await reader.cancel(); throw tooLarge(); } chunks.push(value!); } } finally { reader.releaseLock(); }
  const bytes = new Uint8Array(size); let offset = 0; for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; } return bytes;
}
