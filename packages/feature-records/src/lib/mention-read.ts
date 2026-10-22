import { workspacePathParts } from "@erpx/feature-work_projection/lib/destination-registry";

export type MentionRecordDisplay = {
  title: string;
  description: string;
  status: string;
  version: number;
  fields: { key: string; label: string; value: string | number | boolean }[];
};

/** Mentions identify retained workspace records, not native business records. */
export function mentionRecordEndpoint(base: string, targetId: string): string | null {
  const scope = workspacePathParts(base);
  const separator = targetId.indexOf("/");
  if (!scope || scope.base !== base || scope.path || separator < 1 || separator === targetId.length - 1) return null;
  return `/api/v1${base}/records/${encodeURIComponent(targetId.slice(0, separator))}/${encodeURIComponent(targetId.slice(separator + 1))}?view=mention`;
}

function displayRecord(payload: unknown): MentionRecordDisplay {
  const record = (payload as { data?: { record?: Partial<MentionRecordDisplay> } } | null)?.data?.record;
  if (!record || typeof record.title !== "string" || typeof record.description !== "string" || typeof record.status !== "string"
    || !Number.isSafeInteger(record.version) || !Array.isArray(record.fields)) throw Error("The record response is unavailable. Try again.");
  const fields = record.fields.map(field => {
    if (!field || typeof field.key !== "string" || typeof field.label !== "string"
      || !["string", "boolean", "number"].includes(typeof field.value)
      || typeof field.value === "number" && !Number.isFinite(field.value)) throw Error("The record response is unavailable. Try again.");
    return { key: field.key, label: field.label, value: field.value };
  });
  return { title: record.title, description: record.description, status: record.status, version: record.version!, fields };
}

/** A dismissed, replaced or context-stale read cannot populate another drawer. */
export function createMentionRecordRead(fetcher: (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>, context: () => number) {
  let sequence = 0;
  let pending: AbortController | null = null;
  function cancel() { sequence++; pending?.abort(); pending = null; }
  return {
    cancel,
    async read(base: string, targetId: string): Promise<MentionRecordDisplay | null> {
      cancel();
      const endpoint = mentionRecordEndpoint(base, targetId);
      if (!endpoint) throw Error("This record reference is unavailable.");
      const request = sequence, generation = context(), controller = new AbortController();
      pending = controller;
      const current = () => sequence === request && context() === generation && !controller.signal.aborted;
      try {
        const response = await fetcher(endpoint, { cache: "no-store", signal: controller.signal });
        const payload: unknown = await response.json();
        if (!current()) return null;
        if (!response.ok) throw Error((payload as {error?:{message?:string}} | null)?.error?.message ?? "This record is unavailable in your current workspace.");
        return displayRecord(payload);
      } catch (error) {
        if (!current()) return null;
        throw error;
      } finally { if (request === sequence) pending = null; }
    },
  };
}
