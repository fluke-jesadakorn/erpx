// tests/think_strip.test.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';

function stripThink(text) {
  if (!text) return '';
  return text
    .replace(/<think>[\s\S]*?<\/think>/gi, '')
    .replace(/<\/?think>/gi, '')
    .trim();
}

const OPEN_TAG = '<think>';
const CLOSE_TAG = '</think>';

class Parser {
  constructor() {
    this.buf = '';
    this.cursor = 0;
    this.mode = 'before';
    this.openIdx = -1;
    this.closeIdx = -1;
  }
  push(chunk) {
    this.buf += chunk;
    let t = '';
    let a = '';
    let phase = 'thinking';
    let safety = 0;
    while (safety++ < 8) {
      if (this.mode === 'before') {
        const idx = this.buf.indexOf(OPEN_TAG, this.cursor);
        if (idx === -1) {
          const reserve = OPEN_TAG.length - 1;
          const safeEnd = Math.max(this.cursor, this.buf.length - reserve);
          a += this.buf.slice(this.cursor, safeEnd);
          this.cursor = safeEnd;
          phase = 'answering';
          break;
        }
        this.openIdx = idx;
        this.mode = 'thinking';
        a += this.buf.slice(this.cursor, idx);
        this.cursor = idx + OPEN_TAG.length;
        phase = 'answering';
        continue;
      }
      if (this.mode === 'thinking') {
        if (this.closeIdx === -1) {
          const idx = this.buf.indexOf(CLOSE_TAG, this.cursor);
          if (idx === -1) {
            const reserve = CLOSE_TAG.length - 1;
            const safeEnd = Math.max(this.cursor, this.buf.length - reserve);
            t += this.buf.slice(this.cursor, safeEnd);
            this.cursor = safeEnd;
            phase = 'thinking';
            break;
          }
          this.closeIdx = idx;
          this.mode = 'after';
          t += this.buf.slice(this.cursor, idx);
          this.cursor = idx + CLOSE_TAG.length;
          const tail = this.buf.slice(this.cursor);
          const lead = tail.match(/^\s+/)?.[0].length ?? 0;
          a += tail.slice(lead);
          this.cursor = this.cursor + lead;
          phase = 'answering';
          break;
        }
        t += this.buf.slice(this.cursor);
        this.cursor = this.buf.length;
        phase = 'thinking';
        break;
      }
      a += this.buf.slice(this.cursor);
      this.cursor = this.buf.length;
      phase = 'answering';
      break;
    }
    return { t, a, phase };
  }
  finish() {
    if (this.openIdx === -1) return { reasoning: '', answer: this.buf };
    if (this.closeIdx === -1) return { reasoning: this.buf.slice(this.openIdx + OPEN_TAG.length).trim(), answer: '' };
    return {
      reasoning: this.buf.slice(this.openIdx + OPEN_TAG.length, this.closeIdx).trim(),
      answer: this.buf.slice(this.closeIdx + CLOSE_TAG.length).replace(/^\s+/, ''),
    };
  }
}

test('stripThink removes single block', () => {
  assert.equal(stripThink('<think>foo</think>hello'), 'hello');
});

test('stripThink removes multiple blocks', () => {
  assert.equal(stripThink('<think>a</think>b<think>c</think>d'), 'bd');
});

test('stripThink removes orphaned tags', () => {
  assert.equal(stripThink('<think>hello'), 'hello');
  assert.equal(stripThink('hello</think>'), 'hello');
});

test('stripThink empty input', () => {
  assert.equal(stripThink(''), '');
  assert.equal(stripThink(null), '');
  assert.equal(stripThink(undefined), '');
});

test('parser buffers partial open tag at chunk boundary', () => {
  const p = new Parser();
  let acc = '';
  const s = 'abc<thi';
  for (let i = 0; i < s.length; i += 4) {
    acc += p.push(s.slice(i, i + 4)).a;
  }
  const d2 = p.push('nk>foo</think>answer');
  acc += d2.a;
  assert.match(acc, /abc/);
  assert.equal(d2.t, 'foo');
  assert.equal(d2.a, 'bcanswer');
});

test('parser handles full sequence', () => {
  const p = new Parser();
  p.push('<think>');
  p.push('reasoning content');
  p.push('</think>');
  const d = p.push(' actual answer');
  assert.equal(d.a, ' actual answer');
  const f = p.finish();
  assert.equal(f.reasoning, 'reasoning content');
  assert.equal(f.answer, 'actual answer');
});

test('parser no think tags returns text as answer on finish', () => {
  const p = new Parser();
  p.push('just an answer');
  const f = p.finish();
  assert.equal(f.answer, 'just an answer');
  assert.equal(f.reasoning, '');
});

test('parser streams answer text, reserving last N chars for partial tag match', () => {
  const p = new Parser();
  const d1 = p.push('just an ');
  // reserve 6 keeps last 6 chars (n " ") buffered; only 'ju' emitted
  assert.equal(d1.a, 'ju');
  const d2 = p.push('answer');
  // buf length now 14, cursor 2, reserve 6, safeEnd = max(2, 8)=8, delta = buf.slice(2,8)='st an '
  assert.equal(d2.a, 'st an ');
  const d3 = p.push('text');
  // buf length 18, cursor 8, reserve 6, safeEnd = max(8, 12)=12, delta = buf.slice(8,12)='answ'
  assert.equal(d3.a, 'answ');
  // finish exposes full final buffer as answer (caller typically concatenates answerDelta + finishDelta)
  const f = p.finish();
  assert.equal(f.answer, 'just an answertext');
});