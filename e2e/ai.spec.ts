import { test, expect } from './fixtures';
import type { APIRequestContext } from '@playwright/test';
import { cleanupTestTag, vendorTag, userIdByCode } from './helpers/db';
import { createExpense, advanceWaybill } from './helpers/waybill';

async function ollamaUp(): Promise<boolean> {
  try {
    const r = await fetch('http://localhost:11434/api/tags');
    return r.ok;
  } catch {
    return false;
  }
}

async function signIn(req: APIRequestContext, code: string): Promise<void> {
  const id = await userIdByCode(code);
  const r = await req.post('/api/actor', { data: { id } });
  if (!r.ok()) throw new Error(`Sign-in failed for ${code}: ${r.status()}`);
}

test.describe('AI endpoints', () => {
  test.beforeAll(async () => {
    const up = await ollamaUp();
    test.skip(!up, 'Ollama not reachable on localhost:11434');
  });

  test('6d.1 PUT /api/ocr runs vision OCR on a tiny PNG', async ({ request }) => {
    await signIn(request, 'IT001');
    const pngB64 = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP8//8/AwAI/AL+Sj0fGAAAAABJRU5ErkJggg==';
    const r = await request.put('/api/ocr', {
      data: { file_data_b64: pngB64, file_mime: 'image/png', file_name: 'test.png' },
    });
    expect(r.ok()).toBeTruthy();
    const body = (await r.json()) as { ok: boolean; text?: string; chars?: number; source?: string; vision_model?: string };
    expect(body.ok).toBe(true);
    expect(typeof body.text).toBe('string');
    expect(body.source).toBe('vision-image');
    expect(body.vision_model).toBeTruthy();
  });

  test('6d.2 GET /api/vision-native returns ocrAvailable bool', async ({ request }) => {
    await signIn(request, 'IT001');
    const r = await request.get('/api/vision-native');
    expect(r.ok()).toBeTruthy();
    const body = (await r.json()) as { ocrAvailable: boolean };
    expect(typeof body.ocrAvailable).toBe('boolean');
  });

  test('6d.3 POST /api/ai/chat returns assistant message', async ({ request }) => {
    await signIn(request, 'IT001');
    const r = await request.post('/api/ai/chat', {
      data: { sectionKey: 'chat:full', messages: [{ role: 'user', content: 'Say hi in one word' }] },
    });
    expect(r.ok()).toBeTruthy();
    const body = (await r.json()) as { ok: boolean; text?: string; modelName?: string; latencyMs?: number };
    expect(body.ok).toBe(true);
    expect(typeof body.text).toBe('string');
    expect((body.text ?? '').length).toBeGreaterThan(0);
    expect(body.modelName).toBeTruthy();
  });

  test('6d.4 GET /api/ai/models returns enabled models', async ({ request }) => {
    await signIn(request, 'IT001');
    const r = await request.get('/api/ai/models');
    expect(r.ok()).toBeTruthy();
    const body = (await r.json()) as { models: Array<{ id: number; name: string; enabled: boolean }> };
    expect(Array.isArray(body.models)).toBe(true);
    expect(body.models.length).toBeGreaterThan(0);
    expect(body.models.some((m) => m.enabled)).toBe(true);
  });

  test('6d.5 GET /api/ai/providers returns provider list', async ({ request }) => {
    await signIn(request, 'IT001');
    const r = await request.get('/api/ai/providers');
    expect(r.ok()).toBeTruthy();
    const body = (await r.json()) as { providers: Array<{ id: number; name: string; type: string; enabled: boolean }> };
    expect(Array.isArray(body.providers)).toBe(true);
    expect(body.providers.length).toBeGreaterThan(0);
    expect(body.providers.some((p) => p.enabled)).toBe(true);
  });

  test('6d.6 POST /api/ai/sql returns valid SELECT', async ({ request }) => {
    await signIn(request, 'IT001');
    const r = await request.post('/api/ai/sql', { data: { question: 'how many users are there?' } });
    expect(r.ok()).toBeTruthy();
    const body = (await r.json()) as { ok: boolean; sql?: string; explanation?: string; rowCount?: number };
    expect(body.ok).toBe(true);
    expect(typeof body.sql).toBe('string');
    expect(body.sql!.trim().toLowerCase()).toMatch(/^select\b/);
    expect(body.sql!.toLowerCase()).not.toMatch(/\b(insert|update|delete|drop|alter)\b/);
  });

  test('6d.7 POST /api/finance/rag answers expense question', async ({ request }) => {
    await signIn(request, 'IT001');
    const r = await request.post('/api/finance/rag', { data: { question: 'expense rules' } });
    expect(r.ok()).toBeTruthy();
    const body = (await r.json()) as { ok: boolean; answer?: string | null; hits?: Array<unknown> };
    expect(body.ok).toBe(true);
    expect(Array.isArray(body.hits)).toBe(true);
  });

  test('6d.8 POST /api/waybill/explain-event explains a submitted event', async ({ request }) => {
    await signIn(request, 'IT001');
    const tag = await vendorTag();
    const { waybillId } = await createExpense({
      submitter: 'EMP001',
      amount: 107,
      vendorName: 'Explain Vendor',
      vendorTag: tag,
    });
    const r = await request.post('/api/waybill/explain-event', {
      data: {
        waybillId,
        eventKind: 'submitted',
        fromStage: '',
        toStage: 'submission',
        actorName: 'John Staff',
      },
    });
    expect(r.ok()).toBeTruthy();
    const body = (await r.json()) as { ok: boolean; text?: string; modelName?: string };
    expect(body.ok).toBe(true);
    expect(typeof body.text).toBe('string');
    expect((body.text ?? '').length).toBeGreaterThan(0);
    await cleanupTestTag(tag);
  });

  test('6d.9 POST /api/waybill/[id]/review-hint returns hint', async ({ request }) => {
    await signIn(request, 'IT001');
    const tag = await vendorTag();
    const { waybillId } = await createExpense({
      submitter: 'EMP001',
      amount: 107,
      vendorName: 'Hint Vendor',
      vendorTag: tag,
    });
    await advanceWaybill(waybillId, 'dept_verification');
    const r = await request.post(`/api/waybill/${waybillId}/review-hint`, { data: { stage: 'hod' } });
    expect(r.ok()).toBeTruthy();
    const body = (await r.json()) as { ok: boolean; hint?: { hint: string; stage: string } };
    expect(body.ok).toBe(true);
    expect(body.hint?.stage).toBe('hod');
    expect(typeof body.hint?.hint).toBe('string');
    expect((body.hint?.hint ?? '').length).toBeGreaterThan(0);
    await cleanupTestTag(tag);
  });

  test('6d.10 /ai-settings renders provider/model/assignment panels', async ({ page, request }) => {
    await signIn(request, 'IT001');

    const r1 = await request.get('/ai-settings');
    expect(r1.ok()).toBeTruthy();
    const html1 = await r1.text();
    expect(html1).toMatch(/Providers/);
    expect(html1).toMatch(/Models/);
    expect(html1).toMatch(/Assignments/);

    const r2 = await request.get('/ai-settings?tab=models');
    expect(r2.ok()).toBeTruthy();
    const html2 = await r2.text();
    expect(html2).toMatch(/qwen2\.5|MiniMax|bge-m3|llama/i);

    const r3 = await request.get('/ai-settings?tab=assignments');
    expect(r3.ok()).toBeTruthy();
    const html3 = await r3.text();
    expect(html3).toMatch(/chat:full|cockpit:sql|finance:rag|events:explain/i);

    const id = await userIdByCode('IT001');
    await page.request.post('/api/actor', { data: { id } });
    await page.goto('/ai-settings');
    await expect(page.getByText('Providers', { exact: false }).first()).toBeVisible();
  });
});
