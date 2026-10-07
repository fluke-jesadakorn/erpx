import type {CanvasDefinition,MetricData,Widget} from "@/server/modules/runtime/public/rules/canvas-domain";
import type sanitizeHtml from "sanitize-html";
import {canvasAccent,widgetThemeCss} from "@/lib/executive/theme";
import {escape,amount,chart} from "../../domain/executive/html-widget";
export function createHtmlWidgetDocument(ports:{sanitize:typeof sanitizeHtml;allowedTags:string[];hash(value:string):string;documents:Map<string,string>}){
/** No scripts, navigation, network, credentials or parent access. Binding values are escaped before sanitization. */
function renderHtmlWidget(widget: Widget, data: MetricData, accent: CanvasDefinition["accent"] = "violet") {
  if (widget.kind !== "html" || data.error) return "";
  const values: Record<string, string> = {
    title: escape(widget.title), total: escape(data.total === null ? "Grouped by product unit" : amount(data.total, data)), scope: escape(data.scope), definition: escape(data.definition),
    updatedAt: escape(new Date(data.referenceAt).toLocaleString("en-GB")),
    rows: `<table><tbody>${data.rows.map((row) => `<tr><td>${escape(row.label)}</td><td>${escape(amount(row.value, data))}${row.unit ? ` ${escape(row.unit)}` : ""}</td></tr>`).join("")}</tbody></table>`,
  };
  let source = (widget.html ?? "<h2>{{title}}</h2><strong>{{total}}</strong>{{chart:bar}}")
    .replace(/\{\{(title|total|scope|definition|updatedAt|rows|chart:(?:bar|line|area|column|donut))\}\}/g, (_, key: string) => key.startsWith("chart:") ? chart(key.slice(6), data) : values[key]);
  source = ports.sanitize(source, {
    allowedTags: [...ports.allowedTags.filter((tag) => tag !== "a"), "svg", "g", "path", "circle", "rect", "line", "polyline", "polygon", "text", "title"],
    allowedAttributes: { "*": ["class", "id", "style", "role", "aria-label"], svg: ["viewBox", "width", "height", "role", "aria-label"], g: ["transform"], path: ["d", "fill", "stroke", "stroke-width", "opacity"], circle: ["cx", "cy", "r", "fill", "stroke", "stroke-width", "stroke-dasharray", "stroke-dashoffset", "transform", "opacity"], rect: ["x", "y", "width", "height", "rx", "fill", "opacity"], line: ["x1", "x2", "y1", "y2", "stroke", "stroke-width", "opacity"], polyline: ["points", "fill", "stroke", "stroke-width", "stroke-linejoin"], polygon: ["points", "fill", "opacity"], text: ["x", "y", "fill", "font-size", "text-anchor"] },
    allowedSchemes: [], parser: { lowerCaseAttributeNames: false }, disallowedTagsMode: "discard",
  });
  const css = (widget.css ?? "").replace(/</g, "\\3c ");
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'none'; style-src 'unsafe-inline'; img-src 'none'; font-src 'none'; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="color-scheme" content="light dark"><style>${widgetThemeCss}:root{--accent:${canvasAccent(accent) ?? canvasAccent("violet")};--chart-1:var(--accent)}*{box-sizing:border-box}body{margin:0;padding:20px;background:var(--widget-bg);color:var(--widget-text);font:14px/1.6 system-ui}h1,h2,h3,p{margin:0 0 12px}table{width:100%;border-collapse:collapse}td{padding:8px;border-bottom:1px solid var(--widget-border)}td:last-child{text-align:right}svg{max-width:100%}.erpx-chart{width:100%;height:auto}.erpx-bars{display:grid;gap:14px}.erpx-row{display:flex;justify-content:space-between;gap:12px;font-size:12px}.erpx-track{height:9px;background:var(--widget-track);border-radius:8px;overflow:hidden;margin-top:6px}.erpx-track i{display:block;height:100%;border-radius:8px}.erpx-donut{display:flex;align-items:center;gap:22px;flex-wrap:wrap}.erpx-donut svg{width:150px}.erpx-donut>div{flex:1;font-size:11px}.erpx-donut p{display:flex;align-items:center;gap:8px}.erpx-donut i{width:8px;height:8px;border-radius:50%;flex-shrink:0}.erpx-donut strong{margin-left:auto}.erpx-empty{opacity:.65}${css}</style></head><body>${source}</body></html>`;
}

// Keep an unchanged iframe document stable across live refreshes so user scroll/focus survives.
// Callers authorize and query the source before reaching this cache; it never supplies fallback data.
const documents = ports.documents;
function htmlWidgetDocument(widget: Widget, data: MetricData, accent: CanvasDefinition["accent"] = "violet") {
  if (widget.kind !== "html" || data.error) return "";
  const key = ports.hash(JSON.stringify([widget.html, widget.css, widget.title, data.revision, accent]));
  const cached = documents.get(key);
  if (cached) { documents.delete(key); documents.set(key, cached); return cached; }
  const document = renderHtmlWidget(widget, data, accent);
  documents.set(key, document);
  if (documents.size > 128) documents.delete(documents.keys().next().value!);
  return document;
}

return {htmlWidgetDocument};}
