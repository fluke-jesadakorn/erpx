import "server-only";
import { createHash } from "node:crypto";
import sanitizeHtml from "sanitize-html";
import type { CanvasDefinition, MetricData, Widget } from "@/lib/executive/canvas";
import { absoluteMaximum, chartUnits, formatMetric } from "@/lib/executive/numbers";
import { canvasAccent, widgetThemeCss } from "@/lib/executive/theme";

const escape = (value: unknown) => String(value).replace(/[&<>"']/g, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[character]!));
const amount = (value: string, data: MetricData) => formatMetric(value, data);
const colors = Array.from({ length: 7 }, (_, index) => `var(--chart-${index + 1})`);
const sum = (values: string[]) => String(values.reduce((total, value) => total + BigInt(value), BigInt(0)));

function chart(kind: string, data: MetricData) {
  if (data.unit === "quantity") return `<table><tbody>${data.rows.map((row) => `<tr><td>${escape(row.label)}</td><td>${escape(amount(row.value, data))}${row.unit ? ` ${escape(row.unit)}` : ""}</td></tr>`).join("")}</tbody></table>`;
  if (data.total === null) return '<p class="erpx-empty">A total is unavailable for incompatible units.</p>';
  const aggregateTotal = data.total;
  const rows = data.rows.length > 24
    ? [...data.rows.slice(0, 23), { label: "Other groups", value: sum(data.rows.slice(23).map((row) => row.value)) }]
    : data.rows;
  const maximum = absoluteMaximum(rows.map((row) => row.value));
  const value = (raw: string) => chartUnits(raw, maximum);
  const x = (index: number) => 42 + index * 500 / Math.max(1, rows.length - 1);
  const y = (relative: number) => 190 - relative * 150;
  if (!rows.length) return '<p class="erpx-empty">No matching records in this scope and period.</p>';
  if (kind === "bar") return `<div class="erpx-bars">${rows.map((row, index) => `<div><div class="erpx-row"><span>${escape(row.label)}</span><strong>${escape(amount(row.value, data))}</strong></div><div class="erpx-track"><i style="width:${Math.max(0, value(row.value) * 100)}%;background:${colors[index % colors.length]}"></i></div></div>`).join("")}</div>`;
  if (kind === "donut") {
    if (BigInt(aggregateTotal) <= BigInt(0) || rows.some((row) => BigInt(row.value) < BigInt(0))) return chart("bar", data);
    let before = BigInt(0);
    return `<div class="erpx-donut"><svg viewBox="0 0 140 140" role="img" aria-label="Distribution"><circle cx="70" cy="70" r="52" fill="none" stroke="var(--widget-track)" stroke-width="16"/>${rows.map((row, index) => {
      const offset = before;
      before += BigInt(row.value);
      return `<circle cx="70" cy="70" r="52" fill="none" stroke="${colors[index % colors.length]}" stroke-width="16" stroke-dasharray="${chartUnits(row.value, BigInt(aggregateTotal)) * 326.73} 326.73" stroke-dashoffset="${-chartUnits(String(offset), BigInt(aggregateTotal)) * 326.73}" transform="rotate(-90 70 70)"><title>${escape(row.label)}: ${escape(amount(row.value, data))}</title></circle>`;
    }).join("")}</svg><div>${rows.map((row, index) => `<p><i style="background:${colors[index % colors.length]}"></i>${escape(row.label)} <strong>${escape(amount(row.value, data))}</strong></p>`).join("")}</div></div>`;
  }
  const points = rows.map((row, index) => `${x(index)},${y(value(row.value))}`).join(" ");
  return `<svg class="erpx-chart" viewBox="0 0 580 225" role="img" aria-label="Live ${escape(kind)} chart"><title>${escape(rows.map((row) => `${row.label}: ${amount(row.value, data)}`).join(", "))}</title>${[0, .25, .5, .75, 1].map((tick) => `<line x1="40" x2="555" y1="${y(tick)}" y2="${y(tick)}" stroke="currentColor" opacity=".1"/>`).join("")}${kind === "column" ? rows.map((row, index) => `<rect x="${40 + index * 510 / rows.length}" y="${y(value(row.value))}" width="${Math.max(3, 510 / rows.length - 10)}" height="${Math.max(0, 190 - y(value(row.value)))}" rx="5" fill="${colors[index % colors.length]}"><title>${escape(row.label)}: ${escape(amount(row.value, data))}</title></rect>`).join("") : `${kind === "area" ? `<polygon points="42,190 ${points} ${x(rows.length - 1)},190" fill="var(--accent)" opacity=".17"/>` : ""}<polyline points="${points}" fill="none" stroke="var(--accent)" stroke-width="3" stroke-linejoin="round"/>${rows.map((row, index) => `<circle cx="${x(index)}" cy="${y(value(row.value))}" r="4" fill="var(--accent)"><title>${escape(row.label)}: ${escape(amount(row.value, data))}</title></circle>`).join("")}`}<text x="40" y="216" fill="currentColor" font-size="11">${escape(rows[0].label)}</text><text x="550" y="216" text-anchor="end" fill="currentColor" font-size="11">${escape(rows.at(-1)!.label)}</text></svg>`;
}

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
  source = sanitizeHtml(source, {
    allowedTags: [...sanitizeHtml.defaults.allowedTags.filter((tag) => tag !== "a"), "svg", "g", "path", "circle", "rect", "line", "polyline", "polygon", "text", "title"],
    allowedAttributes: { "*": ["class", "id", "style", "role", "aria-label"], svg: ["viewBox", "width", "height", "role", "aria-label"], g: ["transform"], path: ["d", "fill", "stroke", "stroke-width", "opacity"], circle: ["cx", "cy", "r", "fill", "stroke", "stroke-width", "stroke-dasharray", "stroke-dashoffset", "transform", "opacity"], rect: ["x", "y", "width", "height", "rx", "fill", "opacity"], line: ["x1", "x2", "y1", "y2", "stroke", "stroke-width", "opacity"], polyline: ["points", "fill", "stroke", "stroke-width", "stroke-linejoin"], polygon: ["points", "fill", "opacity"], text: ["x", "y", "fill", "font-size", "text-anchor"] },
    allowedSchemes: [], parser: { lowerCaseAttributeNames: false }, disallowedTagsMode: "discard",
  });
  const css = (widget.css ?? "").replace(/</g, "\\3c ");
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'none'; style-src 'unsafe-inline'; img-src 'none'; font-src 'none'; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="color-scheme" content="light dark"><style>${widgetThemeCss}:root{--accent:${canvasAccent(accent) ?? canvasAccent("violet")};--chart-1:var(--accent)}*{box-sizing:border-box}body{margin:0;padding:20px;background:var(--widget-bg);color:var(--widget-text);font:14px/1.6 system-ui}h1,h2,h3,p{margin:0 0 12px}table{width:100%;border-collapse:collapse}td{padding:8px;border-bottom:1px solid var(--widget-border)}td:last-child{text-align:right}svg{max-width:100%}.erpx-chart{width:100%;height:auto}.erpx-bars{display:grid;gap:14px}.erpx-row{display:flex;justify-content:space-between;gap:12px;font-size:12px}.erpx-track{height:9px;background:var(--widget-track);border-radius:8px;overflow:hidden;margin-top:6px}.erpx-track i{display:block;height:100%;border-radius:8px}.erpx-donut{display:flex;align-items:center;gap:22px;flex-wrap:wrap}.erpx-donut svg{width:150px}.erpx-donut>div{flex:1;font-size:11px}.erpx-donut p{display:flex;align-items:center;gap:8px}.erpx-donut i{width:8px;height:8px;border-radius:50%;flex-shrink:0}.erpx-donut strong{margin-left:auto}.erpx-empty{opacity:.65}${css}</style></head><body>${source}</body></html>`;
}

// Keep an unchanged iframe document stable across live refreshes so user scroll/focus survives.
// Callers authorize and query the source before reaching this cache; it never supplies fallback data.
const documents = new Map<string, string>();
export function htmlWidgetDocument(widget: Widget, data: MetricData, accent: CanvasDefinition["accent"] = "violet") {
  if (widget.kind !== "html" || data.error) return "";
  const key = createHash("sha256").update(JSON.stringify([widget.html, widget.css, widget.title, data.revision, accent])).digest("hex");
  const cached = documents.get(key);
  if (cached) { documents.delete(key); documents.set(key, cached); return cached; }
  const document = renderHtmlWidget(widget, data, accent);
  documents.set(key, document);
  if (documents.size > 128) documents.delete(documents.keys().next().value!);
  return document;
}
