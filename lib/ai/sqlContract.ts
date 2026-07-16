export const SQL_BLOCK = /\[SQL\]([\s\S]*?)\[\/SQL\]/g;

export interface SqlAsk { question: string; }

export function parseSqlBlocks(text: string): { plain: string; asks: SqlAsk[] } {
  const asks: SqlAsk[] = [];
  if (!text) return { plain: '', asks };
  const plain = text.replace(SQL_BLOCK, (_m, body: string) => {
    try {
      const obj = JSON.parse(body);
      if (obj && typeof obj.question === 'string' && obj.question.trim()) {
        asks.push({ question: obj.question.trim() });
      }
    } catch { /* skip malformed */ }
    return '';
  });
  return { plain: plain.trim(), asks };
}