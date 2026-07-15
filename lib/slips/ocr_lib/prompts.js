// Vision LLM prompts used by the per-page waterfall.
// Pass 1 = raw text extraction (primary path).
// Pass 4 = structured "map" extraction (LLM map fallback).

export const RAW_TEXT_PROMPT = `user\nดึงข้อความจากภาพ\nassistant\n`;

export const STRUCTURED_MAP_SYSTEM_PROMPT = `You are a financial document analyst. Given this scanned document page, extract a structured map. Return ONLY JSON, no markdown, no commentary.

Schema:
{
  "title": "string or null",
  "date": "YYYY-MM-DD or null",
  "parties": ["string"],
  "tables": [ { "label": "string", "rows": [[ "string" ]] } ],
  "sections": [ { "heading": "string", "content": "string" } ],
  "signatures": ["string"],
  "stamps_detected": ["string"],
  "unreadable": false,
  "notes": "string (empty when nothing to note)"
}

If the page is illegible, return { "unreadable": true, "reason": "..." }.
Do not invent fields you cannot see.`;

export const STRUCTURED_MAP_USER_PROMPT = (mime) =>
  `Analyze this scanned document page (${mime}). Extract structured fields per the schema. Return JSON only.`;