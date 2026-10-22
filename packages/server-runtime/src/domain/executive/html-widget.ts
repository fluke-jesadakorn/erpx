import type {MetricData} from "@erpx/server-runtime/public/rules/canvas-domain";
import {absoluteMaximum,chartUnits,formatMetric} from "@erpx/feature-shared/lib/executive/numbers";
export const escape = (value: unknown) => String(value).replace(/[&<>"']/g, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[character]!));
export const amount = (value: string, data: MetricData) => formatMetric(value, data);
const colors = Array.from({ length: 7 }, (_, index) => `var(--chart-${index + 1})`);
const sum = (values: string[]) => String(values.reduce((total, value) => total + BigInt(value), BigInt(0)));

export function chart(kind: string, data: MetricData) {
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

