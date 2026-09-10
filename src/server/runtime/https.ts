export async function pinnedNodeRequest(
  url: string,
  addresses: string[],
  headers: Record<string, string>,
  body?: unknown,
  timeoutMs = 25000,
) {
  const { request } = await import("node:https");
  return new Promise<Response>((resolve, reject) => {
    const req = request(
      url,
      {
        method: body ? "POST" : "GET",
        headers,
        agent: false,
        signal: AbortSignal.timeout(timeoutMs),
        lookup: (_hostname, options, callback) => {
          const verified = addresses.map((address) => ({
            address,
            family: address.includes(":") ? 6 : 4,
          }));
          if (options.all) callback(null, verified);
          else callback(null, verified[0].address, verified[0].family);
        },
      },
      (response) => {
        if ((response.statusCode ?? 500) >= 300) {
          response.resume();
          resolve(new Response(null, { status: response.statusCode ?? 500 }));
          return;
        }
        const chunks: Uint8Array[] = [];
        let bytes = 0;
        response.on("data", (chunk: Uint8Array) => {
          bytes += chunk.byteLength;
          if (bytes > 128000) {
            req.destroy(Error("AI_RESPONSE_LIMIT"));
            return;
          }
          chunks.push(chunk);
        });
        response.on("error", reject);
        response.on("end", () =>
          resolve(
            new Response(Buffer.concat(chunks), {
              status: response.statusCode ?? 500,
            }),
          ),
        );
      },
    );
    req.on("error", reject);
    req.end(body ? JSON.stringify(body) : undefined);
  });
}
