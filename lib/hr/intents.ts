import 'server-only';
import { invoke as aiInvoke } from '../ai/router';

export type HRIntent =
  | 'request_leave'
  | 'check_balance'
  | 'show_jd'
  | 'staff_scope'
  | 'switch_user'
  | 'general_chat';

export interface ClassifiedIntent {
  intent: HRIntent;
  leave_type?: 'sick' | 'annual' | 'personal' | null;
  start_date?: string | null;
  end_date?: string | null;
  days?: number | null;
  reason?: string | null;
  employee_code?: string | null;
  check_date?: string | null;
}

interface OllamaShape {
  intent?: string;
  leave_type?: string | null;
  start_date?: string | null;
  end_date?: string | null;
  days?: number | null;
  reason?: string | null;
  employee_code?: string | null;
  check_date?: string | null;
}

const VALID_INTENTS: HRIntent[] = [
  'request_leave',
  'check_balance',
  'show_jd',
  'staff_scope',
  'switch_user',
];

const today = (): string => {
  const d = new Date(Date.now() + 7 * 60 * 60 * 1000);
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  return `${y}-${m}-${day}`;
};

function safeParse(text: string): OllamaShape | null {
  const trimmed = text.trim();
  try {
    return JSON.parse(trimmed);
  } catch {}
  const m = trimmed.match(/\{[\s\S]*\}/);
  if (m) {
    try {
      return JSON.parse(m[0]);
    } catch {}
  }
  return null;
}

function normalize(raw: OllamaShape): ClassifiedIntent {
  const intent = (VALID_INTENTS as string[]).includes(raw.intent || '')
    ? (raw.intent as HRIntent)
    : 'general_chat';
  const leaveTypeRaw = raw.leave_type;
  const leaveType =
    leaveTypeRaw === 'sick' || leaveTypeRaw === 'annual' || leaveTypeRaw === 'personal'
      ? leaveTypeRaw
      : null;
  return {
    intent,
    leave_type: leaveType,
    start_date: raw.start_date || null,
    end_date: raw.end_date || null,
    days: typeof raw.days === 'number' ? raw.days : null,
    reason: raw.reason || null,
    employee_code: raw.employee_code || null,
    check_date: raw.check_date || null,
  };
}

export async function classifyLeaveIntent(text: string): Promise<ClassifiedIntent> {
  const fallback = (): ClassifiedIntent => ({ intent: 'general_chat' });
  const prompt = `You are an HR Assistant Bot. Parse the user's Thai message and extract structured fields in JSON format.\nCurrent date today is: ${today()} (Bangkok Time).\n\nAnalyze the input text and output a JSON object with these EXACT keys:\n{\n  "intent": "request_leave" | "check_balance" | "show_jd" | "staff_scope" | "switch_user" | "general_chat",\n  "leave_type": "sick" | "annual" | "personal" | null,\n  "start_date": "YYYY-MM-DD" | null,\n  "end_date": "YYYY-MM-DD" | null,\n  "days": number | null,\n  "reason": "Thai string" | null,\n  "employee_code": "EMPxxx" | null,\n  "check_date": "YYYY-MM-DD" | null\n}\n\nGuidelines:\n1. Intent:\n   - "request_leave": user wants to request leave (e.g. "ขอลา", "ป่วย", "ลากิจ", "พักร้อน").\n   - "check_balance": user wants to check remaining leave days (e.g. "วันลาคงเหลือ", "สิทธิ์วันหยุด", "เช็ควันลา").\n   - "show_jd": user wants to check job description (e.g. "งานของฉัน", "job description", "ขอบข่ายงาน").\n   - "staff_scope": user wants to check who is on leave in their department/team (e.g. "ใครลาบ้าง", "คิวทีม", "พรุ่งนี้มีใครหยุดไหม").\n   - "switch_user": user wants to switch account (e.g. "/switch EMP001", "สลับผู้ใช้").\n   - "general_chat": anything else.\n2. Relative and absolute dates (relative to today):\n   - "วันนี้" -> today's date\n   - "พรุ่งนี้" -> today + 1 day\n   - "เมื่อวาน" / "เมื่อวานนี้" -> today - 1 day\n   - "มะรืน" -> today + 2 days\n3. Leave types:\n   - ป่วย / sick -> "sick"\n   - พักร้อน / annual / ลาพักผ่อน -> "annual"\n   - กิจ / personal -> "personal"\n4. Always output ONLY valid JSON. No markdown, no explanation.\n\nUser text:\n"""${text}"""`;

  const result = await aiInvoke('hr:classify-intent', 'chat', {
    systemPrompt: 'You output only valid JSON. No markdown. No prose.',
    text: prompt,
    temperature: 0,
  });

  if (!result.ok || !result.text) return fallback();
  const parsed = safeParse(result.text);
  if (!parsed) return fallback();
  return normalize(parsed);
}
