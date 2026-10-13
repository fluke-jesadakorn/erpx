import type {AiProtocol} from "@/lib/shared/ai/providers";
import type {AiGatewayFacts} from './gateway';
import {BusinessRuleError} from '../../shared';
export type AiEndpoint<Gateway = AiGatewayFacts> = {
  timeoutMs?: number;
  protocol?: AiProtocol;
  sessionId?: string;
  baseUrl: string;
  apiKey: string | null;
  model: string;
  gateway?: Gateway;
};

export type AiMessage = {
  role: "system" | "user";
  content:
    | string
    | (
        | { type: "text"; text: string }
        | { type: "image_url"; image_url: { url: string } }
      )[];
};

export function aiBaseUrl(value: string) {
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    throw new BusinessRuleError(
      422,
      "AI_URL_INVALID",
      "Enter the provider's HTTPS base URL.",
    );
  }
  const host = url.hostname.toLowerCase();
  if (
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    url.search ||
    url.hash ||
    !host.includes(".") ||
    host.endsWith(".") ||
    /^[\d.]+$/.test(host) ||
    host.includes(":") ||
    /(^|\.)(localhost|local|internal|test|invalid|example)$/.test(host)
  )
    throw new BusinessRuleError(
      422,
      "AI_URL_INVALID",
      "Use a public HTTPS hostname without credentials, a query, or a fragment.",
    );
  if (/\/(chat\/completions|responses|messages|models)\/?$/.test(url.pathname))
    throw new BusinessRuleError(
      422,
      "AI_BASE_URL_REQUIRED",
      "Enter the base URL, such as https://provider.com/v1, without /chat/completions or /models.",
    );
  return url.toString().replace(/\/$/, "");
}
export function publicAddress(address: string) {
  if (address.includes(":")) {
    try {
      address = new URL(`https://[${address}]/`).hostname.slice(1, -1);
    } catch {
      return false;
    }
    const [first, second] = address
      .split(":")
      .map((part) => parseInt(part || "0", 16));
    // Public unicast only; exclude IANA special-purpose and transition ranges.
    return (
      first >= 0x2000 &&
      first <= 0x3fff &&
      first !== 0x2002 &&
      !(first === 0x2001 && (second < 0x200 || second === 0xdb8)) &&
      !(first === 0x3fff && second < 0x1000)
    );
  }
  const parts = address.split(".").map(Number);
  if (
    parts.length !== 4 ||
    parts.some((n) => !Number.isInteger(n) || n < 0 || n > 255)
  )
    return false;
  const [a, b, c] = parts;
  return (
    a !== 0 &&
    a !== 10 &&
    a !== 127 &&
    a < 224 &&
    !(a === 100 && b >= 64 && b <= 127) &&
    !(a === 169 && b === 254) &&
    !(a === 172 && b >= 16 && b <= 31) &&
    !(a === 192 && (b === 168 || b === 0 || b === 2)) &&
    !(a === 198 && (b === 18 || b === 19 || (b === 51 && c === 100))) &&
    !(a === 203 && b === 0 && c === 113)
  );
}
export function billableUnits(value: unknown): Record<string,number> {
  const output: Record<string,number>={};
  const visit=(node: unknown,prefix:string,depth:number) => {
    if (Object.keys(output).length>=48 || depth>3 || !node || typeof node!=="object" || Array.isArray(node)) return;
    for (const [key,item] of Object.entries(node)) {
      if (!/^[a-z][a-z_0-9]{0,60}$/i.test(key)) continue;
      const path=prefix?`${prefix}.${key}`:key;
      if (typeof item==="number" && Number.isSafeInteger(item) && item>=0) output[path]=item;
      else if (item && typeof item==="object") visit(item,path,depth+1);
    }
  };
  visit(value,"",0);
  return output;
}
export function tokens(value: unknown) {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0 ? value : 0;
}
