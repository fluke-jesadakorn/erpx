export type HRState =
  | 'idle'
  | 'awaiting_leave_type'
  | 'awaiting_start_date'
  | 'awaiting_end_date'
  | 'awaiting_reason'
  | 'awaiting_medical_cert'
  | 'display_team_schedule';

export type LeaveType = 'sick' | 'annual' | 'personal';

export interface TempData {
  leave_type?: LeaveType;
  leave_type_thai?: string;
  start_date?: string;
  end_date?: string;
  days?: number;
  reason?: string;
  schedule?: { name: string; leave_type: LeaveType }[];
  check_date?: string;
  department?: string;
}

export interface QuotaSummary {
  sick: number;
  annual: number;
  personal: number;
}

export interface TransitionInput {
  currentState: HRState | null;
  tempData: TempData;
  messageText: string;
  nlp: {
    intent?: string;
    leave_type?: LeaveType | null;
    start_date?: string | null;
    end_date?: string | null;
    days?: number | null;
    reason?: string | null;
    employee_code?: string | null;
    check_date?: string | null;
  };
  quota: QuotaSummary;
  employeeId?: string | null;
}

export type ResponseType = 'direct_reply' | 'execute_sql';

export interface TransitionResult {
  responseType: ResponseType;
  replyMessages: Array<{ type: string; text?: string; [k: string]: unknown }>;
  nextState: HRState;
  nextTempData: TempData;
  sql?: string;
  params?: unknown[];
}

const THAI_MONTHS_FULL = [
  'มกราคม', 'กุมภาพันธ์', 'มีนาคม', 'เมษายน', 'พฤษภาคม', 'มิถุนายน',
  'กรกฎาคม', 'สิงหาคม', 'กันยายน', 'ตุลาคม', 'พฤศจิกายน', 'ธันวาคม',
];
const THAI_MONTHS_SHORT = [
  'ม.ค.', 'ก.พ.', 'มี.ค.', 'เม.ย.', 'พ.ค.', 'มิ.ย.',
  'ก.ค.', 'ส.ค.', 'ก.ย.', 'ต.ค.', 'พ.ย.', 'ธ.ค.',
];

function thaiMonthIndex(text: string): { monthIdx: number; replaced: string } | null {
  for (let i = 0; i < 12; i++) {
    if (text.includes(THAI_MONTHS_FULL[i])) {
      return { monthIdx: i + 1, replaced: text.replace(THAI_MONTHS_FULL[i], ' ' + (i + 1) + ' ') };
    }
  }
  for (let i = 0; i < 12; i++) {
    const term = THAI_MONTHS_SHORT[i].replace('.', '\\.?');
    const regex = new RegExp(term, 'g');
    if (regex.test(text)) {
      return { monthIdx: i + 1, replaced: text.replace(regex, ' ' + (i + 1) + ' ') };
    }
  }
  return null;
}

export function todayBangkok(): Date {
  return new Date(Date.now() + 7 * 60 * 60 * 1000);
}

export function formatDate(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  return `${y}-${m}-${day}`;
}

export function parseDate(text: string, baseDate?: string | null): string | null {
  if (!text) return null;
  const clean = text.trim().toLowerCase().replace(/\s+/g, ' ');
  const today = todayBangkok();
  const currentYear = today.getFullYear();

  if (clean === 'วันนี้') return formatDate(today);
  if (clean === 'พรุ่งนี้') return formatDate(new Date(today.getTime() + 24 * 60 * 60 * 1000));
  if (clean === 'เมื่อวาน' || clean === 'เมื่อวานนี้') {
    return formatDate(new Date(today.getTime() - 24 * 60 * 60 * 1000));
  }
  if (clean === 'วานซืน' || clean === 'เมื่อวานซืน') {
    return formatDate(new Date(today.getTime() - 2 * 24 * 60 * 60 * 1000));
  }
  if (clean === 'มะรืน' || clean === 'มะรืนนี้') {
    return formatDate(new Date(today.getTime() + 2 * 24 * 60 * 60 * 1000));
  }

  if (baseDate) {
    const dur = clean.match(/^(\d+)\s*(วัน|day|days)$/);
    if (dur) {
      const n = parseInt(dur[1], 10);
      if (n > 0) {
        const start = new Date(baseDate);
        const end = new Date(start.getTime() + (n - 1) * 24 * 60 * 60 * 1000);
        return formatDate(end);
      }
    }
  }

  const monthInfo = thaiMonthIndex(clean);
  const effective = monthInfo ? monthInfo.replaced : clean;
  const monthIdx = monthInfo ? monthInfo.monthIdx : -1;

  const ymd = effective.match(/^(\d{4})[-/](\d{1,2})[-/](\d{1,2})$/);
  if (ymd) {
    let y = parseInt(ymd[1], 10);
    const m = parseInt(ymd[2], 10) - 1;
    const d = parseInt(ymd[3], 10);
    if (y >= 2400) y -= 543;
    const obj = new Date(y, m, d);
    if (!isNaN(obj.getTime())) return formatDate(obj);
  }

  const dmy = effective.match(/^(\d{1,2})[-/ ](\d{1,2})[-/ ](\d{4})$/);
  if (dmy) {
    const d = parseInt(dmy[1], 10);
    const m = parseInt(dmy[2], 10) - 1;
    let y = parseInt(dmy[3], 10);
    if (y >= 2400) y -= 543;
    const obj = new Date(y, m, d);
    if (!isNaN(obj.getTime())) return formatDate(obj);
  }

  const dm = effective.match(/^(\d{1,2})[-/ ](\d{1,2})$/);
  if (dm) {
    const d = parseInt(dm[1], 10);
    const m = parseInt(dm[2], 10) - 1;
    const obj = new Date(currentYear, m, d);
    if (!isNaN(obj.getTime())) return formatDate(obj);
  }

  const digits = effective.match(/\d+/g);
  if (digits) {
    if (digits.length === 3) {
      const d = parseInt(digits[0], 10);
      const m = parseInt(digits[1], 10) - 1;
      let y = parseInt(digits[2], 10);
      if (y < 100) y += 2000;
      if (y >= 2400) y -= 543;
      const obj = new Date(y, m, d);
      if (!isNaN(obj.getTime())) return formatDate(obj);
    } else if (digits.length === 2) {
      const d = parseInt(digits[0], 10);
      let m = parseInt(digits[1], 10) - 1;
      const y = currentYear;
      if (monthIdx !== -1) m = monthIdx - 1;
      const obj = new Date(y, m, d);
      if (!isNaN(obj.getTime())) return formatDate(obj);
    } else if (digits.length === 1 && baseDate) {
      const d = parseInt(digits[0], 10);
      const base = new Date(baseDate);
      const obj = new Date(base.getFullYear(), base.getMonth(), d);
      if (obj < base) obj.setMonth(obj.getMonth() + 1);
      if (!isNaN(obj.getTime())) return formatDate(obj);
    }
  }

  const parsed = Date.parse(clean);
  if (!isNaN(parsed)) return formatDate(new Date(parsed));
  return null;
}

export function calculateDays(start: string, end: string): number {
  const s = new Date(start);
  const e = new Date(end);
  const diff = Math.ceil((e.getTime() - s.getTime()) / (1000 * 60 * 60 * 24)) + 1;
  return diff > 0 ? diff : 1;
}

export function getLeaveTypeThai(type: string | null | undefined): string | null {
  if (type === 'sick') return '🤒 ลาป่วย';
  if (type === 'annual') return '✈️ ลาพักร้อน';
  if (type === 'personal') return '💼 ลากิจ';
  return null;
}

export function getRemainingDays(type: string | undefined, q: QuotaSummary): number {
  if (type === 'sick') return q.sick;
  if (type === 'annual') return q.annual;
  if (type === 'personal') return q.personal;
  return 0;
}

function textReply(s: string): Array<{ type: string; text: string }> {
  return [{ type: 'text', text: `${s}\n\n- bot` }];
}

function detectLeaveTypeInput(cleanText: string): LeaveType | null {
  if (cleanText.includes('ป่วย') || cleanText === 'sick') return 'sick';
  if (cleanText.includes('พักร้อน') || cleanText === 'annual') return 'annual';
  if (cleanText.includes('กิจ') || cleanText === 'personal') return 'personal';
  return null;
}

function cancelReply(): TransitionResult {
  return {
    responseType: 'execute_sql',
    replyMessages: textReply('❌ ยกเลิกการทำรายการเรียบร้อยแล้ว กลับสู่สถานะปกติ'),
    nextState: 'idle',
    nextTempData: {},
    sql: `UPDATE hr.user_sessions SET current_state = 'idle', temp_data = '{}'::jsonb WHERE line_user_id = $1;`,
    params: [],
  };
}

function idleReply(name: string, position: string): TransitionResult {
  const pos = position ? ` ตำแหน่ง ${position}` : '';
  return {
    responseType: 'direct_reply',
    replyMessages: textReply(
      `สวัสดีครับคุณ ${name}${pos}\nโปรดระบุงานที่ต้องการ:\n- พิมพ์ "ลา" เพื่อยื่นใบลาหยุดงาน\n- พิมพ์ "วันลา" เพื่อเช็คสิทธิ์วันลาคงเหลือ\n- พิมพ์ "job description" เพื่อขอดูขอบข่ายงาน\n- พิมพ์ "คิวทีม" เพื่อเช็คตารางลาของทีม`,
    ),
    nextState: 'idle',
    nextTempData: {},
  };
}

function leaveTypePrompt(): TransitionResult {
  return {
    responseType: 'direct_reply',
    replyMessages: textReply(
      '⚠️ ประเภทการลาไม่ถูกต้อง โปรดเลือกประเภทการลา:\n- พิมพ์ "ลาป่วย"\n- พิมพ์ "ลาพักร้อน"\n- พิมพ์ "ลากิจ"\n(หรือพิมพ์ "ยกเลิก" เพื่อออกจากการทำรายการ)',
    ),
    nextState: 'awaiting_leave_type',
    nextTempData: {},
  };
}

function startDateInvalidReply(): TransitionResult {
  return {
    responseType: 'direct_reply',
    replyMessages: textReply(
      '⚠️ วันที่เริ่มไม่ถูกต้อง โปรดระบุฟอร์แมต YYYY-MM-DD (เช่น 2026-06-25) หรือพิมพ์ "วันนี้" / "พรุ่งนี้"',
    ),
    nextState: 'awaiting_start_date',
    nextTempData: {},
  };
}

function endDateInvalidReply(tempData: TempData): TransitionResult {
  return {
    responseType: 'direct_reply',
    replyMessages: textReply(
      '⚠️ วันที่สิ้นสุดไม่ถูกต้อง (ต้องไม่น้อยกว่าวันที่เริ่ม) โปรดระบุแบบ YYYY-MM-DD หรือพิมพ์จำนวนวัน เช่น 1 วัน',
    ),
    nextState: 'awaiting_end_date',
    nextTempData: tempData,
  };
}

function quarantineLeaveType(leaveType: LeaveType, q: QuotaSummary): TransitionResult | null {
  const rem = getRemainingDays(leaveType, q);
  if (rem > 0) return null;
  return {
    responseType: 'execute_sql',
    replyMessages: textReply(
      `⚠️ ไม่สามารถยื่นคำขอลาได้เนื่องจากสิทธิ์วันลาหมดแล้ว\n\n- ประเภทการลา: ${getLeaveTypeThai(leaveType)}\n- คงเหลือ: 0 วัน`,
    ),
    nextState: 'idle',
    nextTempData: {},
    sql: `UPDATE hr.user_sessions SET current_state = 'idle', temp_data = '{}'::jsonb WHERE line_user_id = $1;`,
    params: [],
  };
}

function approveQuotaGuard(leaveType: LeaveType, days: number, q: QuotaSummary): TransitionResult | null {
  const rem = getRemainingDays(leaveType, q);
  if (days <= rem) return null;
  return {
    responseType: 'execute_sql',
    replyMessages: textReply(
      `⚠️ ไม่สามารถยื่นคำขอลาได้เนื่องจากวันหยุดคงเหลือไม่พอ\n\n- ประเภท: ${getLeaveTypeThai(leaveType)}\n- ขอ: ${days} วัน\n- คงเหลือ: ${rem} วัน\n\nระบบยกเลิกรายการโดยอัตโนมัติ`,
    ),
    nextState: 'idle',
    nextTempData: {},
    sql: `UPDATE hr.user_sessions SET current_state = 'idle', temp_data = '{}'::jsonb WHERE line_user_id = $1;`,
    params: [],
  };
}

function insertLeaveSql(): string {
  return `
    WITH new_leave AS (
      INSERT INTO hr.leave_requests (employee_id, leave_type, start_date, end_date, days, reason, status)
      VALUES ($2::uuid, $3::text, $4::date, $5::date, $6::numeric, $7::text, 'pending')
      RETURNING id
    )
    UPDATE hr.user_sessions
      SET current_state = 'idle', temp_data = '{}'::jsonb
    WHERE line_user_id = $1;
  `;
}

function submittedReply(td: TempData, reason: string): Array<{ type: string; text: string }> {
  return textReply(
    `✅ ส่งคำขอลาหยุดเรียบร้อยแล้ว!\n\n📋 รายละเอียดคำขอ:\n- ประเภท: ${td.leave_type_thai}\n- ระยะเวลา: ${td.start_date} ถึง ${td.end_date} (${td.days} วัน)\n- เหตุผล: ${reason}\n\nคำขอของคุณได้รับการบันทึกเข้าระบบแล้ว และกำลังรอการพิจารณาอนุมัติจากฝ่ายบุคคล (HR)`,
  );
}

function sickCertPrompt(td: TempData): TransitionResult {
  return {
    responseType: 'execute_sql',
    replyMessages: textReply(
      `📋 เนื่องจากคุณขอลาป่วยมากกว่า 2 วัน (${td.days} วัน)\nกรุณาแจ้งรายละเอียดใบรับรองแพทย์:\n\n- พิมพ์ชื่อโรงพยาบาล/คลินิก และวันที่ออกใบรับรอง\n- หรือพิมพ์ "ยังไม่มี" หากยังไม่ได้รับใบรับรองแพทย์`,
    ),
    nextState: 'awaiting_medical_cert',
    nextTempData: td,
    sql: `UPDATE hr.user_sessions SET current_state = 'awaiting_medical_cert', temp_data = $2::jsonb WHERE line_user_id = $1;`,
    params: [],
  };
}

function switchAccountReply(_code: string): TransitionResult {
  return {
    responseType: 'execute_sql',
    replyMessages: textReply(`🔄 กำลังสลับบัญชี...`),
    nextState: 'idle',
    nextTempData: {},
    sql: `
      WITH unbind AS (
        UPDATE hr.employees SET line_user_id = NULL WHERE line_user_id = $1
      ), bind AS (
        UPDATE hr.employees SET line_user_id = $1 WHERE employee_code = $2 RETURNING name, position
      )
      INSERT INTO hr.user_sessions (line_user_id, current_state, temp_data)
      VALUES ($1, 'idle', '{}'::jsonb)
      ON CONFLICT (line_user_id) DO UPDATE SET current_state = 'idle', temp_data = '{}'::jsonb
      RETURNING (SELECT name FROM bind) AS employee_name, (SELECT position FROM bind) AS employee_position;
    `,
    params: [],
  };
}

function unregisteredReply(): TransitionResult {
  return {
    responseType: 'direct_reply',
    replyMessages: textReply(
      '⚠️ คุณยังไม่ได้ลงทะเบียนในระบบบอท HR\nโปรดพิมพ์คำสั่งสลับบัญชีเพื่อทดสอบ เช่น:\n/switch EMP001 (เพื่อสวมบทบาท สมชาย)',
    ),
    nextState: 'idle',
    nextTempData: {},
  };
}

export function transition(input: TransitionInput): TransitionResult {
  const cleanText = (input.messageText || '').trim().toLowerCase();
  const nlp = input.nlp;
  const quota = input.quota;

  if (!input.employeeId && !cleanText.startsWith('/switch ')) {
    return unregisteredReply();
  }

  if (cleanText.startsWith('/switch ') || (nlp.intent === 'switch_user' && nlp.employee_code)) {
    const code = nlp.intent === 'switch_user' && nlp.employee_code
      ? nlp.employee_code
      : input.messageText.replace('/switch ', '').trim().toUpperCase();
    return switchAccountReply(code);
  }

  if (input.currentState && input.currentState !== 'idle') {
    const state = input.currentState as HRState;
    const tempData: TempData = input.tempData || {};

    if (cleanText === 'ยกเลิก') return cancelReply();

    if (state === 'awaiting_leave_type') {
      const leaveType = detectLeaveTypeInput(cleanText);
      if (!leaveType) return leaveTypePrompt();
      const block = quarantineLeaveType(leaveType, quota);
      if (block) return block;
      const nextTd: TempData = {
        ...tempData,
        leave_type: leaveType,
        leave_type_thai: getLeaveTypeThai(leaveType) || leaveType,
      };
      if (!nextTd.start_date) {
        return {
          responseType: 'execute_sql',
          replyMessages: textReply(
            `📋 ประเภทการลา: ${nextTd.leave_type_thai}\n\nโปรดระบุ "วันที่เริ่มลาหยุด" (เช่น 2026-06-25 หรือพิมพ์ วันนี้ / พรุ่งนี้)`,
          ),
          nextState: 'awaiting_start_date',
          nextTempData: nextTd,
          sql: `UPDATE hr.user_sessions SET current_state = 'awaiting_start_date', temp_data = $2::jsonb WHERE line_user_id = $1;`,
          params: [],
        };
      }
      if (!nextTd.end_date) {
        return {
          responseType: 'execute_sql',
          replyMessages: textReply(
            `📋 ประเภทการลา: ${nextTd.leave_type_thai}\n📅 วันที่เริ่มลา: ${nextTd.start_date}\n\nโปรดระบุ "วันที่สิ้นสุด" (เช่น 2026-06-26)`,
          ),
          nextState: 'awaiting_end_date',
          nextTempData: nextTd,
          sql: `UPDATE hr.user_sessions SET current_state = 'awaiting_end_date', temp_data = $2::jsonb WHERE line_user_id = $1;`,
          params: [],
        };
      }
      if (!nextTd.reason) {
        return {
          responseType: 'execute_sql',
          replyMessages: textReply(
            `📋 ประเภทการลา: ${nextTd.leave_type_thai}\n📅 ระยะเวลาลา: ${nextTd.start_date} ถึง ${nextTd.end_date} (${nextTd.days} วัน)\n\nโปรดระบุ "เหตุผลการลา" (เช่น พักผ่อน / มีธุระ)`,
          ),
          nextState: 'awaiting_reason',
          nextTempData: nextTd,
          sql: `UPDATE hr.user_sessions SET current_state = 'awaiting_reason', temp_data = $2::jsonb WHERE line_user_id = $1;`,
          params: [],
        };
      }
      const days = nextTd.days ?? calculateDays(nextTd.start_date!, nextTd.end_date!);
      const guard = approveQuotaGuard(nextTd.leave_type!, days, quota);
      if (guard) return guard;
      if (nextTd.leave_type === 'sick' && days > 2) return sickCertPrompt({ ...nextTd, days });
      return submitNow(nextTd);
    }

    if (state === 'awaiting_start_date') {
      const start = parseDate(input.messageText);
      if (!start) return startDateInvalidReply();
      const nextTd: TempData = { ...tempData, start_date: start };
      if (!nextTd.end_date) {
        return {
          responseType: 'execute_sql',
          replyMessages: textReply(
            `📅 วันที่เริ่มลา: ${start}\n\nโปรดระบุ "วันที่สิ้นสุด" (เช่น 2026-06-26 หรือพิมพ์จำนวนวัน เช่น 2 วัน)`,
          ),
          nextState: 'awaiting_end_date',
          nextTempData: nextTd,
          sql: `UPDATE hr.user_sessions SET current_state = 'awaiting_end_date', temp_data = $2::jsonb WHERE line_user_id = $1;`,
          params: [],
        };
      }
      const days = calculateDays(start, nextTd.end_date);
      const guard = approveQuotaGuard(nextTd.leave_type!, days, quota);
      if (guard) return guard;
      nextTd.days = days;
      if (!nextTd.reason) {
        return {
          responseType: 'execute_sql',
          replyMessages: textReply(
            `📅 ระยะเวลาลา: ${start} ถึง ${nextTd.end_date} (${days} วัน)\n\nโปรดระบุ "เหตุผลการลา"`,
          ),
          nextState: 'awaiting_reason',
          nextTempData: nextTd,
          sql: `UPDATE hr.user_sessions SET current_state = 'awaiting_reason', temp_data = $2::jsonb WHERE line_user_id = $1;`,
          params: [],
        };
      }
      if (nextTd.leave_type === 'sick' && days > 2) return sickCertPrompt(nextTd);
      return submitNow(nextTd);
    }

    if (state === 'awaiting_end_date') {
      const end = parseDate(input.messageText, tempData.start_date);
      if (!end || (tempData.start_date && end < tempData.start_date)) {
        return endDateInvalidReply(tempData);
      }
      const days = calculateDays(tempData.start_date!, end);
      const guard = approveQuotaGuard(tempData.leave_type!, days, quota);
      if (guard) return guard;
      const nextTd: TempData = { ...tempData, end_date: end, days };
      if (!nextTd.reason) {
        return {
          responseType: 'execute_sql',
          replyMessages: textReply(
            `📅 ระยะเวลาลา: ${nextTd.start_date} ถึง ${end} (${days} วัน)\n\nโปรดระบุ "เหตุผลการลา"`,
          ),
          nextState: 'awaiting_reason',
          nextTempData: nextTd,
          sql: `UPDATE hr.user_sessions SET current_state = 'awaiting_reason', temp_data = $2::jsonb WHERE line_user_id = $1;`,
          params: [],
        };
      }
      if (nextTd.leave_type === 'sick' && days > 2) return sickCertPrompt(nextTd);
      return submitNow(nextTd);
    }

    if (state === 'awaiting_reason') {
      const reason = input.messageText;
      const nextTd: TempData = { ...tempData, reason };
      const days = nextTd.days ?? calculateDays(nextTd.start_date!, nextTd.end_date!);
      const guard = approveQuotaGuard(nextTd.leave_type!, days, quota);
      if (guard) return guard;
      if (nextTd.leave_type === 'sick' && days > 2) return sickCertPrompt({ ...nextTd, days });
      return submitNow(nextTd);
    }

    if (state === 'awaiting_medical_cert') {
      const certNote = input.messageText;
      const nextTd: TempData = tempData;
      const finalReason = `${nextTd.reason || ''} [ใบรับรองแพทย์: ${certNote}]`.trim();
      return {
        responseType: 'execute_sql',
        replyMessages: textReply(
          `✅ ส่งคำขอลาหยุดเรียบร้อยแล้ว!\n\n📋 รายละเอียดคำขอ:\n- ประเภท: ${nextTd.leave_type_thai}\n- ระยะเวลา: ${nextTd.start_date} ถึง ${nextTd.end_date} (${nextTd.days} วัน)\n- เหตุผล: ${finalReason}\n- 🏥 ใบรับรองแพทย์: ${certNote}\n\nคำขอของคุณได้รับการบันทึกเข้าระบบแล้ว`,
        ),
        nextState: 'idle',
        nextTempData: {},
        sql: insertLeaveSql(),
        params: [],
      };
    }
  }

  if (nlp.intent === 'request_leave') {
    const today = formatDate(todayBangkok());
    const nextTd: TempData = {
      leave_type: nlp.leave_type || undefined,
      leave_type_thai: nlp.leave_type ? getLeaveTypeThai(nlp.leave_type) || undefined : undefined,
      start_date: nlp.start_date || undefined,
      end_date: nlp.end_date || undefined,
      days: nlp.days || undefined,
      reason: nlp.reason || undefined,
    };

    if (nextTd.end_date && !nextTd.start_date) nextTd.start_date = today;
    if (nextTd.start_date && !nextTd.end_date) {
      if (nextTd.days) {
        const s = new Date(nextTd.start_date);
        const e = new Date(s.getTime() + (nextTd.days - 1) * 24 * 60 * 60 * 1000);
        nextTd.end_date = formatDate(e);
      } else {
        nextTd.end_date = nextTd.start_date;
        nextTd.days = 1;
      }
    }
    if (nextTd.start_date && nextTd.end_date && !nextTd.days) {
      nextTd.days = calculateDays(nextTd.start_date, nextTd.end_date);
    }

    const haveAll = nextTd.leave_type && nextTd.start_date && nextTd.end_date && nextTd.reason;
    if (haveAll) {
      const guard = approveQuotaGuard(nextTd.leave_type!, nextTd.days!, quota);
      if (guard) return guard;
      if (nextTd.leave_type === 'sick' && (nextTd.days ?? 0) > 2) return sickCertPrompt(nextTd);
      return submitNow(nextTd);
    }

    if (nextTd.leave_type) {
      const block = quarantineLeaveType(nextTd.leave_type, quota);
      if (block) return block;
    }

    if (!nextTd.leave_type) {
      return {
        responseType: 'execute_sql',
        replyMessages: textReply(
          'กรุณาเลือกประเภทการลา:\n- พิมพ์ "ลาป่วย"\n- พิมพ์ "ลาพักร้อน"\n- พิมพ์ "ลากิจ"',
        ),
        nextState: 'awaiting_leave_type',
        nextTempData: nextTd,
        sql: `UPDATE hr.user_sessions SET current_state = 'awaiting_leave_type', temp_data = $2::jsonb WHERE line_user_id = $1;`,
        params: [],
      };
    }
    if (!nextTd.start_date) {
      return {
        responseType: 'execute_sql',
        replyMessages: textReply(
          `📋 ประเภทการลา: ${nextTd.leave_type_thai}\n\nโปรดระบุ "วันที่เริ่มลาหยุด" (เช่น 2026-06-25 หรือพิมพ์ วันนี้ / พรุ่งนี้)`,
        ),
        nextState: 'awaiting_start_date',
        nextTempData: nextTd,
        sql: `UPDATE hr.user_sessions SET current_state = 'awaiting_start_date', temp_data = $2::jsonb WHERE line_user_id = $1;`,
        params: [],
      };
    }
    if (!nextTd.end_date) {
      return {
        responseType: 'execute_sql',
        replyMessages: textReply(
          `📋 ประเภทการลา: ${nextTd.leave_type_thai}\n📅 วันเริ่มลา: ${nextTd.start_date}\n\nโปรดระบุ "วันที่สิ้นสุด" (เช่น 2026-06-26 หรือพิมพ์จำนวนวัน เช่น 3 วัน)`,
        ),
        nextState: 'awaiting_end_date',
        nextTempData: nextTd,
        sql: `UPDATE hr.user_sessions SET current_state = 'awaiting_end_date', temp_data = $2::jsonb WHERE line_user_id = $1;`,
        params: [],
      };
    }
    return {
      responseType: 'execute_sql',
      replyMessages: textReply(
        `📋 ประเภทการลา: ${nextTd.leave_type_thai}\n📅 ระยะเวลาลา: ${nextTd.start_date} ถึง ${nextTd.end_date} (${nextTd.days} วัน)\n\nโปรดระบุ "เหตุผลการลา" (เช่น พักผ่อนส่วนตัว / เป็นไข้หวัด)`,
      ),
      nextState: 'awaiting_reason',
      nextTempData: nextTd,
      sql: `UPDATE hr.user_sessions SET current_state = 'awaiting_reason', temp_data = $2::jsonb WHERE line_user_id = $1;`,
      params: [],
    };
  }

  if (nlp.intent === 'check_balance') {
    return {
      responseType: 'direct_reply',
      replyMessages: textReply(
        `📊 สิทธิ์วันลาคงเหลือ\n\n🤒 ลาป่วย: คงเหลือ ${quota.sick} วัน\n✈️ ลาพักร้อน: คงเหลือ ${quota.annual} วัน\n💼 ลากิจ: คงเหลือ ${quota.personal} วัน`,
      ),
      nextState: 'idle',
      nextTempData: {},
    };
  }

  if (nlp.intent === 'staff_scope') {
    const today = formatDate(todayBangkok());
    const checkDate = nlp.check_date || today;
    return {
      responseType: 'execute_sql',
      replyMessages: textReply(`🔍 กำลังค้นหาข้อมูลคิวทีม...`),
      nextState: 'display_team_schedule',
      nextTempData: { check_date: checkDate },
      sql: `
        SELECT e.name, lr.leave_type
          FROM hr.leave_requests lr
          JOIN hr.employees e ON lr.employee_id = e.id
         WHERE lr.status = 'approved'
           AND e.department = $1
           AND $2::date BETWEEN lr.start_date AND lr.end_date
         ORDER BY e.name;
      `,
      params: [],
    };
  }

  return idleReply(input.employeeId ? 'สมาชิก' : 'ผู้ใช้', '');
}

function submitNow(td: TempData): TransitionResult {
  return {
    responseType: 'execute_sql',
    replyMessages: submittedReply(td, td.reason || ''),
    nextState: 'idle',
    nextTempData: {},
    sql: insertLeaveSql(),
    params: [],
  };
}

export function serialize(state: HRState, tempData: TempData): string {
  return JSON.stringify(tempData ?? {});
}

export function loadSession(state: string | null, tempData: unknown): { state: HRState; tempData: TempData } {
  let parsed: TempData = {};
  if (tempData && typeof tempData === 'object') parsed = tempData as TempData;
  else if (typeof tempData === 'string') {
    try {
      parsed = JSON.parse(tempData);
    } catch {
      parsed = {};
    }
  }
  return {
    state: (state || 'idle') as HRState,
    tempData: parsed,
  };
}
