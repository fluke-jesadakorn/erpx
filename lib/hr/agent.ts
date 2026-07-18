import 'server-only';
import { query } from '../db';
import { appendMessage, loadSession, nowIso, pruneSession, type ChatMessage } from '../ai/memory';
import { resolve } from '../ai/router';

const SYSTEM_PROMPT = `You are a Thai HR assistant bot for employees. You can answer leave-balance questions, find team schedules, submit leave requests, and cancel pending leave. Always reply in 1-2 short sentences (under 200 chars). Use Thai unless the user wrote English. If the user has not registered yet, tell them to use /switch EMP001 to bind their LINE account.`;

const MAX_ITERATIONS = 5;
const MAX_MESSAGES = 20;

interface ToolDef {
  type: 'function';
  function: {
    name: string;
    description: string;
    parameters: Record<string, unknown>;
  };
}

const TOOL_DEFS: ToolDef[] = [
  {
    type: 'function',
    function: {
      name: 'check_quota',
      description: 'Look up remaining sick/annual/personal leave days for a given employee.',
      parameters: {
        type: 'object',
        properties: { employee_id: { type: 'string', description: 'UUID' } },
        required: ['employee_id'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'find_team_schedule',
      description: 'List approved leave for a department on a given date (YYYY-MM-DD).',
      parameters: {
        type: 'object',
        properties: {
          department: { type: 'string' },
          date: { type: 'string', description: 'YYYY-MM-DD' },
        },
        required: ['department', 'date'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'submit_leave',
      description: 'Submit a new leave request.',
      parameters: {
        type: 'object',
        properties: {
          employee_id: { type: 'string' },
          leave_type: { type: 'string', enum: ['sick', 'annual', 'personal'] },
          start_date: { type: 'string', description: 'YYYY-MM-DD' },
          end_date: { type: 'string', description: 'YYYY-MM-DD' },
          days: { type: 'number' },
          reason: { type: 'string' },
        },
        required: ['employee_id', 'leave_type', 'start_date', 'end_date', 'days'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'cancel_pending',
      description: 'Cancel the most recent pending leave request for an employee.',
      parameters: {
        type: 'object',
        properties: { employee_id: { type: 'string' } },
        required: ['employee_id'],
      },
    },
  },
];

interface ToolCall {
  id: string;
  type: 'function';
  function: { name: string; arguments: string };
}

type AgentMessage = ChatMessage | { role: 'tool'; content: string; ts: string; meta?: Record<string, unknown> };

interface ResolvedShape {
  provider: {
    id: number;
    name: string;
    type: 'ollama' | 'openai_compat' | 'minimax';
    base_url: string;
    api_key: string | null;
  };
  model: { id: number; name: string; defaults_json: any };
  params: Record<string, any>;
}

async function lookupEmployeeByLineUser(lineUserId: string): Promise<{ id: string; employee_code: string; name: string; department: string } | null> {
  const r = await query<{ id: string; employee_code: string; name: string; department: string }>(
    `SELECT id, employee_code, name, department FROM hr.employees WHERE line_user_id = $1`,
    [lineUserId],
  );
  return r.rows[0] ?? null;
}

async function executeTool(name: string, args: Record<string, unknown>): Promise<unknown> {
  if (name === 'check_quota') {
    const { getEmployee } = await import('./employees');
    const emp = await getEmployee(String(args.employee_id));
    if (!emp) return { error: 'employee not found' };
    return {
      sick_remaining: emp.total_sick_leave - emp.used_sick_leave,
      annual_remaining: emp.total_annual_leave - emp.used_annual_leave,
      personal_remaining: emp.total_personal_leave - emp.used_personal_leave,
    };
  }
  if (name === 'find_team_schedule') {
    const department = String(args.department ?? '').trim();
    const date = String(args.date ?? '').trim();
    if (!department || !date) return { error: 'department and date required' };
    const r = await query<{ name: string; leave_type: string }>(
      `SELECT e.name, lr.leave_type
         FROM hr.leave_requests lr
         JOIN hr.employees e ON e.id = lr.employee_id
        WHERE lr.status = 'approved'
          AND e.department = $1
          AND $2::date BETWEEN lr.start_date AND lr.end_date
        ORDER BY e.name`,
      [department, date],
    );
    return { date, department, approved: r.rows };
  }
  if (name === 'submit_leave') {
    const { submitLeave } = await import('./leave');
    try {
      const result = await submitLeave({
        employeeId: Number(args.employee_id),
        leaveType: String(args.leave_type) as 'sick' | 'annual' | 'personal',
        startDate: String(args.start_date),
        endDate: String(args.end_date),
        days: Number(args.days),
        reason: (args.reason as string | null) ?? null,
      });
      return result;
    } catch (e: any) {
      return { error: e?.message ?? String(e) };
    }
  }
  if (name === 'cancel_pending') {
    const employeeId = String(args.employee_id);
    const r = await query<{ id: string }>(
      `SELECT id FROM hr.leave_requests
        WHERE employee_id = $1 AND status = 'pending'
        ORDER BY created_at DESC LIMIT 1`,
      [employeeId],
    );
    if (r.rows.length === 0) return { error: 'no pending leave to cancel' };
    const id = r.rows[0].id;
    await query(
      `UPDATE hr.leave_requests SET status = 'rejected', reject_reason = 'cancelled by user', updated_at = now() WHERE id = $1`,
      [id],
    );
    return { cancelled: id };
  }
  return { error: `unknown tool ${name}` };
}

async function callOllama(
  baseUrl: string,
  model: string,
  messages: AgentMessage[],
): Promise<{ text?: string; toolCalls?: ToolCall[] }> {
  const apiMessages = messages.map((m) => {
    const base: Record<string, unknown> = { role: m.role, content: m.content };
    if (m.role === 'assistant' && m.meta && Array.isArray((m.meta as any).tool_calls)) {
      base.tool_calls = (m.meta as any).tool_calls;
    }
    return base;
  });
  const res = await fetch(`${baseUrl.replace(/\/$/, '')}/api/chat`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      model,
      messages: apiMessages,
      stream: false,
      tools: TOOL_DEFS,
      options: { num_predict: 600 },
    }),
  });
  if (!res.ok) throw new Error(`ollama HTTP ${res.status}`);
  const data: any = await res.json();
  const msg = data?.message ?? {};
  const text: string = typeof msg.content === 'string' ? msg.content : '';
  const calls = Array.isArray(msg.tool_calls) ? msg.tool_calls : [];
  const toolCalls: ToolCall[] = calls.map((c: any, idx: number) => ({
    id: c.id ?? `call_${idx}_${Math.random().toString(36).slice(2)}`,
    type: 'function',
    function: {
      name: c.function?.name ?? c.name,
      arguments: typeof c.function?.arguments === 'string'
        ? c.function.arguments
        : JSON.stringify(c.function?.arguments ?? c.args ?? {}),
    },
  }));
  return { text, toolCalls: toolCalls.length > 0 ? toolCalls : undefined };
}

async function callOpenAICompat(
  baseUrl: string,
  apiKey: string | null,
  model: string,
  messages: AgentMessage[],
): Promise<{ text?: string; toolCalls?: ToolCall[] }> {
  const apiMessages: any[] = messages.map((m) => {
    if (m.role === 'tool') {
      const id = m.meta && typeof (m.meta as any).tool_call_id === 'string'
        ? (m.meta as any).tool_call_id
        : '';
      const out: Record<string, unknown> = { role: 'tool', content: m.content };
      if (id) out.tool_call_id = id;
      return out;
    }
    if (m.role === 'assistant' && m.meta && Array.isArray((m.meta as any).tool_calls)) {
      return {
        role: 'assistant',
        content: m.content || null,
        tool_calls: (m.meta as any).tool_calls,
      };
    }
    return { role: m.role, content: m.content };
  });
  const headers: Record<string, string> = { 'Content-Type': 'application/json' };
  if (apiKey) headers['Authorization'] = `Bearer ${apiKey}`;
  const res = await fetch(`${baseUrl.replace(/\/$/, '')}/chat/completions`, {
    method: 'POST',
    headers,
    body: JSON.stringify({
      model,
      messages: apiMessages,
      temperature: 0.2,
      max_tokens: 600,
      tools: TOOL_DEFS,
      tool_choice: 'auto',
    }),
  });
  if (!res.ok) {
    const errTxt = await res.text().catch(() => '');
    throw new Error(`openai-compat HTTP ${res.status}: ${errTxt.slice(0, 200)}`);
  }
  const data: any = await res.json();
  const choice = data?.choices?.[0]?.message;
  const text: string = choice?.content ?? '';
  const calls = Array.isArray(choice?.tool_calls) ? choice.tool_calls : [];
  const toolCalls: ToolCall[] = calls.map((c: any) => ({
    id: c.id ?? `call_${Math.random().toString(36).slice(2)}`,
    type: 'function',
    function: {
      name: c.function?.name,
      arguments: typeof c.function?.arguments === 'string' ? c.function.arguments : JSON.stringify(c.function?.arguments ?? {}),
    },
  }));
  return { text, toolCalls: toolCalls.length > 0 ? toolCalls : undefined };
}

async function callModelOnce(
  res: ResolvedShape,
  messages: AgentMessage[],
): Promise<{ text?: string; toolCalls?: ToolCall[] }> {
  if (res.provider.type === 'ollama') {
    return callOllama(res.provider.base_url, res.model.name, messages);
  }
  return callOpenAICompat(res.provider.base_url, res.provider.api_key, res.model.name, messages);
}

function toAgentMessage(m: ChatMessage): AgentMessage {
  return m as AgentMessage;
}

export async function runHrAgent(lineUserId: string, userText: string): Promise<string> {
  const employee = await lookupEmployeeByLineUser(lineUserId);
  const actorBlock = employee
    ? `The user's LINE account is bound to employee ${employee.employee_code} (${employee.name}) in department "${employee.department}". Their employee_id is "${employee.id}". When invoking tools that need an employee_id, use this value unless they explicitly reference a different person.`
    : `The user's LINE account is NOT bound to any employee. Tell them to use "/switch EMP001" to bind their account before you can help with personal data.`;

  const sessionKey = `hr:${lineUserId}`;
  const userIdNum = 0;

  const prior = await loadSession(userIdNum, sessionKey);
  const messages: AgentMessage[] = [];
  messages.push({ role: 'system', content: `${SYSTEM_PROMPT}\n\n${actorBlock}`, ts: nowIso() });
  if (prior) {
    for (const m of prior.messages) messages.push(toAgentMessage(m));
  }
  messages.push({ role: 'user', content: userText, ts: nowIso() });

  const resolved = (await resolve('hr:agent', 'chat')) as ResolvedShape | null;
  if (!resolved) {
    return 'ระบบ AI ยังไม่พร้อมใช้งาน กรุณาลองใหม่ภายหลัง';
  }

  let finalText = '';
  for (let iter = 0; iter < MAX_ITERATIONS; iter++) {
    const result = await callModelOnce(resolved, messages);
    if (result.text) finalText = result.text;
    if (!result.toolCalls || result.toolCalls.length === 0) break;

    messages.push({
      role: 'assistant',
      content: result.text ?? '',
      ts: nowIso(),
      meta: {
        tool_calls: result.toolCalls.map((c) => ({
          id: c.id,
          type: c.type,
          function: { name: c.function.name, arguments: c.function.arguments },
        })),
      },
    });

    for (const call of result.toolCalls) {
      let args: Record<string, unknown> = {};
      try {
        args = JSON.parse(call.function.arguments);
      } catch {
        args = {};
      }
      let toolResult: unknown;
      try {
        toolResult = await executeTool(call.function.name, args);
      } catch (e: any) {
        toolResult = { error: e?.message ?? String(e) };
      }
      messages.push({
        role: 'tool',
        content: JSON.stringify(toolResult),
        ts: nowIso(),
        meta: { tool_call_id: call.id, name: call.function.name },
      });
    }
  }

  if (!finalText) finalText = employee ? 'ไม่สามารถตอบได้ในขณะนี้' : 'กรุณาลงทะเบียนก่อน';

  await appendMessage(userIdNum, sessionKey, { role: 'user', content: userText, ts: nowIso() });
  await appendMessage(userIdNum, sessionKey, { role: 'assistant', content: finalText, ts: nowIso() });
  await pruneSession(userIdNum, sessionKey, MAX_MESSAGES);

  return finalText;
}