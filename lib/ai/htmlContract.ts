export const HTML_BLOCK = /\[HTML\]([\s\S]*?)\[\/HTML\]/g;

export interface HtmlBlock { html: string; }

export function parseHtmlBlocks(text: string): { plain: string; htmls: HtmlBlock[] } {
  const htmls: HtmlBlock[] = [];
  if (!text) return { plain: '', htmls };
  const plain = text.replace(HTML_BLOCK, (_m, body: string) => {
    if (typeof body === 'string' && body.trim().length > 0) htmls.push({ html: body });
    return '';
  });
  return { plain: plain.trim(), htmls };
}