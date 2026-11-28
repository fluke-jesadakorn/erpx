import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  socialPostTopicExists,
  socialCommentTopicExists,
} from '../../../modules/collaboration/server/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../modules/collaboration/server/src');
const adapters = resolve(srcRoot, 'adapters');
const shared = readFileSync(resolve(adapters, 'read-tail-sql.ts'), 'utf8');
const socialText = readFileSync(resolve(adapters, 'social-text.ts'), 'utf8');
const socialHome = readFileSync(resolve(adapters, 'social-home.ts'), 'utf8');

describe('Collaboration read-tail r18 parameterized family', () => {
  test('socialPostTopicExists stays byte-identical to the pre-dedupe literals', () => {
    expect(socialPostTopicExists('t.tag')).toBe(
      'EXISTS(SELECT 1 FROM social_post_topics pt WHERE pt.post_id=p.id AND pt.tag=t.tag)',
    );
    expect(socialPostTopicExists('?')).toBe(
      'EXISTS(SELECT 1 FROM social_post_topics pt WHERE pt.post_id=p.id AND pt.tag=?)',
    );
  });

  test('socialCommentTopicExists stays byte-identical to the pre-dedupe literals', () => {
    expect(socialCommentTopicExists('t.tag')).toBe(
      "EXISTS(SELECT 1 FROM social_comment_topics ct JOIN social_comments c ON c.id=ct.comment_id WHERE c.post_id=p.id AND c.state='published' AND ct.tag=t.tag)",
    );
    expect(socialCommentTopicExists('?')).toBe(
      "EXISTS(SELECT 1 FROM social_comment_topics ct JOIN social_comments c ON c.id=ct.comment_id WHERE c.post_id=p.id AND c.state='published' AND ct.tag=?)",
    );
  });

  test('family is defined once in the shared file', () => {
    expect(shared.match(/export const socialPostTopicExists=/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const socialCommentTopicExists=/g) ?? []).toHaveLength(1);
  });

  test('inline topic EXISTS literals are gone and callers use the helpers', () => {
    expect(socialText.split('socialPostTopicExists(').length - 1).toBe(1);
    expect(socialText.split('socialCommentTopicExists(').length - 1).toBe(1);
    expect(socialHome.split('socialPostTopicExists(').length - 1).toBe(1);
    expect(socialHome.split('socialCommentTopicExists(').length - 1).toBe(1);
    expect(socialText).not.toContain('EXISTS(SELECT 1 FROM social_post_topics pt WHERE');
    expect(socialText).not.toContain('EXISTS(SELECT 1 FROM social_comment_topics ct JOIN');
    expect(socialHome).not.toContain('EXISTS(SELECT 1 FROM social_post_topics pt WHERE');
    expect(socialHome).not.toContain('EXISTS(SELECT 1 FROM social_comment_topics ct JOIN');
  });
});

describe('Collaboration read-tail r18 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r18 BEFORE=415 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(415);
  });
});
