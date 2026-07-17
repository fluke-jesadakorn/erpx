export interface ReasoningSplit {
  reasoning: string;
  answer: string;
  phase: 'thinking' | 'answering';
}

export interface ReasoningDelta {
  thinkingDelta: string;
  answerDelta: string;
  phase: 'thinking' | 'answering';
}

const OPEN_TAG = '<think>';
const CLOSE_TAG = '</think>';

export class ReasoningParser {
  private buf = '';
  private cursor = 0;
  private mode: 'before' | 'thinking' | 'after' = 'before';
  private openIdx = -1;
  private closeIdx = -1;

  push(chunk: string): ReasoningDelta {
    this.buf += chunk;
    let thinkingDelta = '';
    let answerDelta = '';
    let phase: 'thinking' | 'answering' = 'thinking';
    let safety = 0;
    while (safety++ < 8) {
      if (this.mode === 'before') {
        const idx = this.buf.indexOf(OPEN_TAG, this.cursor);
        if (idx === -1) {
          const reserve = OPEN_TAG.length - 1;
          const safeEnd = Math.max(this.cursor, this.buf.length - reserve);
          answerDelta += this.buf.slice(this.cursor, safeEnd);
          this.cursor = safeEnd;
          phase = 'answering';
          break;
        }
        this.openIdx = idx;
        this.mode = 'thinking';
        answerDelta += this.buf.slice(this.cursor, idx);
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
            thinkingDelta += this.buf.slice(this.cursor, safeEnd);
            this.cursor = safeEnd;
            phase = 'thinking';
            break;
          }
          this.closeIdx = idx;
          this.mode = 'after';
          thinkingDelta += this.buf.slice(this.cursor, idx);
          this.cursor = idx + CLOSE_TAG.length;
          const tail = this.buf.slice(this.cursor);
          const leadStrip = tail.match(/^\s+/)?.[0].length ?? 0;
          answerDelta += tail.slice(leadStrip);
          this.cursor = this.cursor + leadStrip;
          phase = 'answering';
          break;
        }
        thinkingDelta += this.buf.slice(this.cursor);
        this.cursor = this.buf.length;
        phase = 'thinking';
        break;
      }
      answerDelta += this.buf.slice(this.cursor);
      this.cursor = this.buf.length;
      phase = 'answering';
      break;
    }
    return { thinkingDelta, answerDelta, phase };
  }

  finish(): ReasoningSplit {
    if (this.openIdx === -1) {
      return { reasoning: '', answer: this.buf, phase: 'answering' };
    }
    if (this.closeIdx === -1) {
      return { reasoning: this.buf.slice(this.openIdx + OPEN_TAG.length).trim(), answer: '', phase: 'thinking' };
    }
    return {
      reasoning: this.buf.slice(this.openIdx + OPEN_TAG.length, this.closeIdx).trim(),
      answer: this.buf.slice(this.closeIdx + CLOSE_TAG.length).replace(/^\s+/, ''),
      phase: 'answering',
    };
  }

  reset(): void {
    this.buf = '';
    this.cursor = 0;
    this.mode = 'before';
    this.openIdx = -1;
    this.closeIdx = -1;
  }
}

export function splitReasoning(raw: string): ReasoningSplit {
  const open = raw.indexOf(OPEN_TAG);
  if (open === -1) {
    return { reasoning: '', answer: raw, phase: 'answering' };
  }
  const close = raw.indexOf(CLOSE_TAG, open + OPEN_TAG.length);
  if (close === -1) {
    return { reasoning: raw.slice(open + OPEN_TAG.length), answer: '', phase: 'thinking' };
  }
  const reasoning = raw.slice(open + OPEN_TAG.length, close).trim();
  const answer = raw.slice(close + CLOSE_TAG.length).replace(/^\s+/, '');
  return { reasoning, answer, phase: 'answering' };
}
