const FILLER_PATTERNS: RegExp[] = [
  /^(please|plz|pls)\s+/i,
  /^(hi|hello|hey|good\s+(morning|afternoon|evening))\b[,!\s]*/i,
  /^(can\s+you|could\s+you|would\s+you|will\s+you)\s+/i,
  /^(i\s+want\s+to|i\s+need\s+to|i'?d\s+like\s+to|help\s+me|i'?m\s+trying\s+to)\s+/i,
  /^(show\s+me|give\s+me|tell\s+me|find|fetch|get|list)\s+/i,
  /^(สวัสดี|ขอ|ช่วย|หน่อย|ครับ|ค่ะ|คะ)\s*/i,
  /^(können\s+Sie|bitte|zeig\s+mir|gib\s+mir|ich\s+möchte)\s+/i,
];

const TRAILING_PUNCT = /[?!.,;:]+$/;

function stripCodeFences(s: string): string {
  return s.replace(/```[\s\S]*?```/g, ' ').replace(/`[^`]*`/g, ' ');
}

function collapseWhitespace(s: string): string {
  return s.replace(/\s+/g, ' ').trim();
}

function titleCase(s: string): string {
  return s
    .split(' ')
    .map((w, i) => {
      if (i === 0 || i === s.split(' ').length - 1) {
        return w.charAt(0).toUpperCase() + w.slice(1).toLowerCase();
      }
      return w;
    })
    .join(' ');
}

function clamp(s: string, max: number): string {
  if (s.length <= max) return s;
  const cut = s.slice(0, max);
  const lastSpace = cut.lastIndexOf(' ');
  return (lastSpace > max * 0.6 ? cut.slice(0, lastSpace) : cut).replace(TRAILING_PUNCT, '') + '…';
}

function firstClause(s: string): string {
  for (const sep of ['\n', '. ', '? ', '! ']) {
    const i = s.indexOf(sep);
    if (i > 0 && i < 120) return s.slice(0, i);
  }
  return s;
}

export function suggestTitle(rawText: string, fallback = 'New chat'): string {
  const cleaned = collapseWhitespace(stripCodeFences(rawText ?? ''));
  if (!cleaned) return fallback;

  let s = cleaned;
  for (const p of FILLER_PATTERNS) s = s.replace(p, '');
  s = collapseWhitespace(s).replace(TRAILING_PUNCT, '');
  s = firstClause(s);
  if (!s) s = cleaned;

  const cased = /[a-z]/.test(s) && /[A-Z]/.test(s) === false ? titleCase(s) : s;
  return clamp(cased, 60);
}

export function isPlaceholderTitle(title: string): boolean {
  const t = (title ?? '').trim().toLowerCase();
  if (!t) return true;
  if (t === 'new chat') return true;
  if (t.startsWith('new chat ')) return true;
  if (/^untitled\s*\d*$/.test(t)) return true;
  return false;
}