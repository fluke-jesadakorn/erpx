import { requestPublicOrigin } from "@/server/public-origin";
import { ApiError } from "./http";
export async function betaBody(request: Request) {
  const origin = request.headers.get("origin");
  if (origin && origin !== requestPublicOrigin(request))
    throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use the application origin.");
  if (Number(request.headers.get("content-length") ?? 0) > 24000)
    throw new ApiError(413, "INPUT_TOO_LARGE", "Please shorten your request.");
  const reader = request.body?.getReader();
  let text = "",
    size = 0;
  const decoder = new TextDecoder();
  if (reader)
    while (true) {
      const chunk = await reader.read();
      if (chunk.done) break;
      size += chunk.value.byteLength;
      if (size > 24000) {
        await reader.cancel();
        throw new ApiError(
          413,
          "INPUT_TOO_LARGE",
          "Please shorten your request.",
        );
      }
      text += decoder.decode(chunk.value, { stream: true });
    }
  try {
    return JSON.parse(text + decoder.decode());
  } catch {
    throw new ApiError(422, "INVALID_JSON", "Provide a JSON request.");
  }
}
