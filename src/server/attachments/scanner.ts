import "server-only";

import { createConnection } from "node:net";
import { isAbsolute, join } from "node:path";
import { stat } from "node:fs/promises";
import { attachmentsV2Enabled } from "./feature";

const DEFAULT_MAX_DEFINITION_AGE_MS = 24 * 60 * 60_000;

function boundedTimeout(environment:NodeJS.ProcessEnv,key:string,fallback:number){
  const configured=Number(environment[key]??fallback);
  return Number.isSafeInteger(configured)&&configured>=25&&configured<=fallback?configured:fallback;
}

function scannerSocket(environment: NodeJS.ProcessEnv = process.env) {
  const value = environment.ERPX_CLAMD_SOCKET ?? "";
  if (!value || !isAbsolute(value)) throw new Error("ERPX_CLAMD_SOCKET_REQUIRED");
  return value;
}

function command(value: string, environment: NodeJS.ProcessEnv = process.env) {
  return new Promise<string>((resolve, reject) => {
    const socket = createConnection({ path: scannerSocket(environment) });
    const chunks: Buffer[] = [];
    let settled = false;
    const finish = (error?: Error, response?: string) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      socket.destroy();
      if (error) reject(error);
      else resolve(response!);
    };
    const parse = () => {
      const raw = Buffer.concat(chunks);
      const delimiter = raw.indexOf(0);
      if (delimiter < 0) return;
      const response = raw.subarray(0, delimiter).toString("utf8").trim();
      finish(response ? undefined : new Error("CLAMD_EMPTY_RESPONSE"), response);
    };
    const timeout = setTimeout(() => finish(new Error("CLAMD_TIMEOUT")), boundedTimeout(environment,"ERPX_CLAMD_COMMAND_TIMEOUT_MS",5_000));
    socket.on("connect", () => socket.write(`z${value}\0`));
    socket.on("data", (chunk) => { chunks.push(Buffer.from(chunk)); parse(); });
    socket.on("error", (error) => finish(error));
    socket.on("close", () => {
      if (settled) return;
      const response = Buffer.concat(chunks).toString("utf8").replace(/\0+$/g, "").trim();
      finish(response ? undefined : new Error("CLAMD_EMPTY_RESPONSE"), response);
    });
  });
}

async function newestDefinition(environment: NodeJS.ProcessEnv = process.env) {
  const directory = environment.ERPX_CLAMAV_DATABASE_DIR ?? "";
  if (!directory || !isAbsolute(directory)) throw new Error("ERPX_CLAMAV_DATABASE_DIR_REQUIRED");
  const candidates = await Promise.all(["daily.cvd", "daily.cld", "main.cvd", "main.cld"].map(async (name) => {
    try { return await stat(join(/* turbopackIgnore: true */ directory, name)); } catch { return null; }
  }));
  const modifiedAt = Math.max(...candidates.flatMap((item) => item ? [item.mtimeMs] : []), 0);
  if (!modifiedAt) throw new Error("CLAMAV_DEFINITIONS_MISSING");
  // PostgreSQL scan audit columns are bigint epoch milliseconds. macOS exposes
  // sub-millisecond stat precision, so normalize before persisting readiness.
  return Math.floor(modifiedAt);
}

export type AttachmentScannerReadiness = {
  enabled: boolean;
  ready: boolean;
  engine: string | null;
  definitionUpdatedAt: number | null;
  error: string | null;
};

export async function attachmentScannerReadiness(environment: NodeJS.ProcessEnv = process.env): Promise<AttachmentScannerReadiness> {
  if (!attachmentsV2Enabled(environment)) return { enabled: false, ready: true, engine: null, definitionUpdatedAt: null, error: null };
  try {
    const [pong, version, definitionUpdatedAt] = await Promise.all([
      command("PING", environment), command("VERSION", environment), newestDefinition(environment),
    ]);
    if (pong !== "PONG") throw new Error("CLAMD_UNHEALTHY");
    const configured = Number(environment.ERPX_CLAMAV_MAX_DEFINITION_AGE_MS ?? DEFAULT_MAX_DEFINITION_AGE_MS);
    const maxAge = Number.isSafeInteger(configured) && configured >= 60_000 ? configured : DEFAULT_MAX_DEFINITION_AGE_MS;
    if (Date.now() - definitionUpdatedAt > maxAge) throw new Error("CLAMAV_DEFINITIONS_STALE");
    return { enabled: true, ready: true, engine: version, definitionUpdatedAt, error: null };
  } catch (error) {
    return { enabled: true, ready: false, engine: null, definitionUpdatedAt: null, error: error instanceof Error ? error.message : "CLAMAV_UNAVAILABLE" };
  }
}

export async function scanAttachmentBytes(bytes: Uint8Array, environment: NodeJS.ProcessEnv = process.env) {
  const readiness = await attachmentScannerReadiness(environment);
  if (!readiness.ready || !readiness.enabled) throw new Error(readiness.error ?? "ATTACHMENT_SCANNER_DISABLED");
  return new Promise<{ clean: boolean; signature: string | null; engine: string; definitionUpdatedAt: number }>((resolve, reject) => {
    const socket = createConnection({ path: scannerSocket(environment) });
    const chunks: Buffer[] = [];
    let settled=false;
    const finish=(error?:Error,result?:{ clean: boolean; signature: string | null; engine: string; definitionUpdatedAt: number })=>{
      if(settled)return;
      settled=true;
      clearTimeout(timeout);
      socket.destroy();
      if(error)reject(error);else resolve(result!);
    };
    const parse=()=>{
      const raw=Buffer.concat(chunks),delimiter=raw.indexOf(0);
      if(delimiter<0)return;
      const result=raw.subarray(0,delimiter).toString("utf8").trim();
      if(!result)return finish(new Error("CLAMD_EMPTY_SCAN_RESPONSE"));
      if(result.endsWith(": OK"))return finish(undefined,{clean:true,signature:null,engine:readiness.engine??"clamd",definitionUpdatedAt:readiness.definitionUpdatedAt!});
      const infected=result.match(/:\s+(.+)\s+FOUND$/);
      if(infected)return finish(undefined,{clean:false,signature:infected[1]??"FOUND",engine:readiness.engine??"clamd",definitionUpdatedAt:readiness.definitionUpdatedAt!});
      finish(new Error(`CLAMD_SCAN_RESPONSE:${result.slice(0,160)}`));
    };
    const timeout = setTimeout(() => finish(new Error("CLAMD_SCAN_TIMEOUT")), boundedTimeout(environment,"ERPX_CLAMD_SCAN_TIMEOUT_MS",30_000));
    socket.on("connect", () => {
      socket.write("zINSTREAM\0");
      const chunkSize = 64 * 1024;
      for (let offset = 0; offset < bytes.byteLength; offset += chunkSize) {
        const part = Buffer.from(bytes.buffer, bytes.byteOffset + offset, Math.min(chunkSize, bytes.byteLength - offset));
        const length = Buffer.allocUnsafe(4); length.writeUInt32BE(part.byteLength, 0);
        socket.write(length); socket.write(part);
      }
      socket.write(Buffer.alloc(4));
    });
    socket.on("data", (chunk) => {chunks.push(Buffer.from(chunk));parse();});
    socket.on("error", (error)=>finish(error));
    socket.on("close", () => {
      if(settled)return;
      const result = Buffer.concat(chunks).toString("utf8").replace(/\0+$/g, "").trim();
      if(!result)return finish(new Error("CLAMD_EMPTY_SCAN_RESPONSE"));
      if (result.endsWith(": OK"))return finish(undefined,{ clean: true, signature: null, engine: readiness.engine ?? "clamd", definitionUpdatedAt: readiness.definitionUpdatedAt! });
      const infected = result.match(/:\s+(.+)\s+FOUND$/);
      if (infected)return finish(undefined,{ clean: false, signature: infected[1] ?? "FOUND", engine: readiness.engine ?? "clamd", definitionUpdatedAt: readiness.definitionUpdatedAt! });
      finish(new Error(`CLAMD_SCAN_RESPONSE:${result.slice(0, 160)}`));
    });
  });
}
