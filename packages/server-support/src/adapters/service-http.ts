import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { ApiError } from "@erpx/server-runtime/public/http";
export async function serviceCommandBody(request: Request) {
    const origin = request.headers.get("origin");
    if (origin && origin !== requestPublicOrigin(request))
        throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use the application origin.");
    const key = request.headers.get("Idempotency-Key");
    if (!key || !/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))
        throw new ApiError(422, "COMMAND_KEY_REQUIRED", "A valid Idempotency-Key is required.");
    const reader = request.body?.getReader();
    if (!reader)
        throw new ApiError(422, "INVALID_JSON", "Provide a request body.");
    const chunks: Uint8Array[] = [];
    let length = 0;
    for (;;) {
        const { done, value } = await reader.read();
        if (done)
            break;
        length += value.byteLength;
        if (length > 192000) {
            await reader.cancel();
            throw new ApiError(413, "INPUT_TOO_LARGE", "Shorten the content before saving.");
        }
        chunks.push(value);
    }
    const bytes = new Uint8Array(length);
    let offset = 0;
    for (const chunk of chunks) {
        bytes.set(chunk, offset);
        offset += chunk.length;
    }
    try {
        return {
            key,
            input: JSON.parse(new TextDecoder().decode(bytes)) as unknown,
        };
    }
    catch {
        throw new ApiError(422, "INVALID_JSON", "Provide valid JSON.");
    }
}
