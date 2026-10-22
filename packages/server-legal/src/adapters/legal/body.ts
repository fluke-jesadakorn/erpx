import { ApiError } from "@erpx/server-runtime/public/http";
import { readBoundedLegalBody } from "@erpx/server-legal/application/body";
export function boundedBody(request: Request, limit: number) { return readBoundedLegalBody({ declaredLength: request.headers.get("content-length"), cancel: async () => { await request.body?.cancel(); }, reader: () => request.body?.getReader() ?? null }, limit, (status, code, message) => new ApiError(status, code, message)); }
