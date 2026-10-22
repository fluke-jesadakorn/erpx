import "server-only";
import {lookup} from "node:dns/promises";
import {request as httpsRequest} from "node:https";
import {publicAddress} from "@erpx/server-agent/public/ai-transport";
import {ApiError} from "@erpx/server-runtime/public/http";
import {officialLawUrl} from "@erpx/server-legal/domain/law-corpus";
/** Pin public DNS results; never follow redirects or read more than the document limit. */
export async function fetchOfficialLawSource(url: string): Promise<Response> {
  const canonical = officialLawUrl(url,(status,code,message)=>new ApiError(status,code,message)), addresses = await lookup(new URL(canonical).hostname, { all: true });
  if (!addresses.length || addresses.some(({ address }) => !publicAddress(address))) throw new ApiError(503,"LEGAL_SOURCE_DNS","Official source DNS did not resolve exclusively to public addresses.");
  return new Promise((resolve, reject) => {
    const request = httpsRequest(canonical, { method: "GET", agent: false, signal: AbortSignal.timeout(30000), headers: { "User-Agent": "ERPX-Legal-Source-Check/1.0", Accept: "application/pdf,text/html,text/plain" }, lookup: (_host, options, callback) => options.all ? callback(null, addresses) : callback(null, addresses[0].address, addresses[0].family) }, (response) => {
      if ((response.statusCode ?? 500) < 200 || (response.statusCode ?? 500) >= 300) { response.resume(); resolve(new Response(null, { status: response.statusCode ?? 500 })); return; }
      const chunks: Uint8Array[] = []; let size = 0;
      response.on("data", (chunk: Uint8Array) => { size += chunk.byteLength; if (size > 10 * 1024 * 1024) request.destroy(new Error("LEGAL_SOURCE_TOO_LARGE")); else chunks.push(chunk); });
      response.on("error", reject);
      response.on("end", () => resolve(new Response(Buffer.concat(chunks), { status: response.statusCode, headers: { "Content-Type": String(response.headers["content-type"] ?? "application/octet-stream") } })));
    });
    request.on("error", reject); request.end();
  });
}
