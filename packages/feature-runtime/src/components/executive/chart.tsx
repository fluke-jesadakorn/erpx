"use client";
import { useId, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { monthBounds, type MetricData, type Widget } from "@erpx/feature-shared/lib/executive/canvas";
import { absoluteMaximum, chartUnits, formatMetric as formatExactMetric, scaledChartUnits } from "@erpx/feature-shared/lib/executive/numbers";
import { chartColors } from "@erpx/feature-shared/lib/executive/theme";
import styles from "@erpx/feature-runtime/components/executive/canvas.module.css";

export const formatMetric = formatExactMetric;
const palette = ["var(--canvas-accent)", ...chartColors.slice(1)];
const sum = (values: string[]) => String(values.reduce((total, value) => total + BigInt(value), BigInt(0)));

export function MetricChart({ widget, data, onSelectMonth }: { widget: Widget; data: MetricData; onSelectMonth?:(month:string)=>void }) {
  const id = useId().replaceAll(":", ""), [active, setActive] = useState<number | null>(null);
  const sourceRows = data.rows.length > 24
    ? [...data.rows.slice(0, 23), { label: "Other groups", value: sum(data.rows.slice(23).map((row) => row.value)) }]
    : data.rows;
  const maximumUnits = absoluteMaximum(sourceRows.map((row) => row.value));
  const rows = sourceRows.map((row) => ({ ...row, chart: chartUnits(row.value, maximumUnits) }));
  const max = Math.max(0, ...rows.map((row) => row.chart));
  const min = Math.min(0, ...rows.map((row) => row.chart));
  const span = max - min || 1;
  const x = (index: number) => 58 + index * 510 / Math.max(1, rows.length - 1);
  const y = (value: number) => 210 - (value - min) / span * 170;
  const zero = y(0);
  const axisValue = (value: number) => formatExactMetric(scaledChartUnits(maximumUnits, value), data);
  const monthSelectProps = (label:string) => {
    if(widget.breakdown!=="month"||!onSelectMonth||!monthBounds(label))return {};
    return {role:"button" as const,"aria-label":`Filter report to ${label}`,onClick:()=>onSelectMonth(label),onKeyDown:(event:ReactKeyboardEvent<SVGElement>)=>{if(event.key==="Enter"||event.key===" "){event.preventDefault();onSelectMonth(label);}}};
  };
  if (!rows.length) return <div className={styles.chartEmpty}>No matching records for this scope and period.</div>;
  if (data.unit === "quantity" || data.total === null) return <div className={styles.chartEmpty}>Use a table to read quantities in each product&apos;s unit.</div>;
  const aggregateTotal = data.total;

  return <div className={styles.chartContainer}>
    {widget.kind === "bar" || widget.kind === "donut" && data.unit === "percent" ? <div className={styles.rankedBars}>{rows.map((row, index) => <div key={index}><div><span><i style={{ background: palette[index % palette.length] }}/>{row.label.replaceAll("_", " ")}</span><strong>{formatExactMetric(row.value, data)}</strong></div><div className={styles.barTrack}><div style={{ width: `${Math.abs(row.chart) / Math.max(max, Math.abs(min)) * 100}%`, background: palette[index % palette.length] }} title={`${row.label}: ${formatExactMetric(row.value, data)}`}/></div></div>)}</div>
      : widget.kind === "donut" && BigInt(aggregateTotal) > BigInt(0) && min >= 0 ? <div className={styles.ringChart}><svg viewBox="0 0 210 210" role="img" aria-label={`${widget.title}: ${formatExactMetric(aggregateTotal, data)}`}><circle cx="105" cy="105" r="78" stroke="currentColor" opacity=".07" strokeWidth="22" fill="none"/>{rows.map((row, index) => {
        const before = sum(rows.slice(0, index).map((item) => item.value));
        const share = chartUnits(row.value, BigInt(aggregateTotal));
        const offset = chartUnits(before, BigInt(aggregateTotal));
        return <circle key={index} cx="105" cy="105" r="78" fill="none" stroke={palette[index % palette.length]} strokeWidth={active === index ? 28 : 22} strokeDasharray={`${share * 490.09} 490.09`} strokeDashoffset={-offset * 490.09} transform="rotate(-90 105 105)" onMouseEnter={() => setActive(index)} onMouseLeave={() => setActive(null)} tabIndex={0} onFocus={() => setActive(index)} onBlur={() => setActive(null)} {...monthSelectProps(row.label)}><title>{row.label}: {formatExactMetric(row.value, data)}</title></circle>;
      })}<text x="105" y="103" textAnchor="middle" fill="currentColor" fontSize="25" fontWeight="600">{active === null ? formatExactMetric(aggregateTotal, data) : `${(chartUnits(rows[active].value, BigInt(aggregateTotal)) * 100).toFixed(1)}%`}</text><text x="105" y="125" textAnchor="middle" fill="currentColor" opacity=".5" fontSize="10">{active === null ? "TOTAL" : "SHARE OF TOTAL"}</text></svg><div className={styles.chartLegend}>{rows.map((row, index) => <div key={index} data-active={active === index}><i style={{ background: palette[index % palette.length] }}/><span>{row.label.replaceAll("_", " ")}</span><strong>{formatExactMetric(row.value, data)}</strong></div>)}</div></div>
      : <svg className={styles.plot} viewBox="0 0 610 250" role="img" aria-labelledby={`${id}-title`}><title id={`${id}-title`}>{widget.title}: {rows.map((row) => `${row.label}: ${formatExactMetric(row.value, data)}`).join(", ")}</title><defs><linearGradient id={`${id}-fill`} x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stopColor="var(--canvas-accent)" stopOpacity=".35"/><stop offset="100%" stopColor="var(--canvas-accent)" stopOpacity=".02"/></linearGradient></defs>{[0, .25, .5, .75, 1].map((tick, index) => {
        const value = min + tick * span;
        return <g key={index}><line x1="55" x2="580" y1={y(value)} y2={y(value)} stroke="currentColor" opacity=".08" strokeDasharray="3 5"/><text x="45" y={y(value) + 4} textAnchor="end" fontSize="10" fill="currentColor" opacity=".5">{axisValue(value)}</text></g>;
      })}{widget.kind === "column" ? rows.map((row, index) => <rect key={index} x={58 + index * 520 / rows.length} y={Math.min(y(row.chart), zero)} width={Math.max(4, 520 / rows.length - 12)} height={Math.max(1, Math.abs(zero - y(row.chart)))} rx="5" fill={palette[index % palette.length]} onMouseEnter={() => setActive(index)} onMouseLeave={() => setActive(null)} tabIndex={widget.breakdown==="month"&&onSelectMonth&&monthBounds(row.label)?0:-1} {...monthSelectProps(row.label)}><title>{row.label}: {formatExactMetric(row.value, data)}</title></rect>) : <>{widget.kind === "area" && <polygon points={`58,${zero} ${rows.map((row, index) => `${x(index)},${y(row.chart)}`).join(" ")} ${x(rows.length - 1)},${zero}`} fill={`url(#${id}-fill)`}/>}<polyline points={rows.map((row, index) => `${x(index)},${y(row.chart)}`).join(" ")} fill="none" stroke="var(--canvas-accent)" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round"/>{rows.map((row, index) => <circle key={index} cx={x(index)} cy={y(row.chart)} r={active === index ? 6 : 4} fill="var(--canvas-card)" stroke="var(--canvas-accent)" strokeWidth="2" tabIndex={0} onFocus={() => setActive(index)} onBlur={() => setActive(null)} onMouseEnter={() => setActive(index)} onMouseLeave={() => setActive(null)} {...monthSelectProps(row.label)}><title>{row.label}: {formatExactMetric(row.value, data)}</title></circle>)}</>}{rows.map((row, index) => index === 0 || index === rows.length - 1 || index % Math.max(1, Math.ceil(rows.length / 5)) === 0 ? <text key={index} x={widget.kind === "column" ? 58 + (index + .5) * 520 / rows.length : x(index)} y="237" textAnchor={index === 0 ? "start" : index === rows.length - 1 ? "end" : "middle"} fontSize="10" fill="currentColor" opacity=".55">{row.label.length > 15 ? `${row.label.slice(0, 13)}…` : row.label}</text> : null)}</svg>}
    {active !== null && rows[active] && <div className={styles.chartTooltip} role="status">{rows[active].label} <strong>{formatExactMetric(rows[active].value, data)}</strong></div>}
    {data.rows.length > 24 && <p className={styles.widgetDescription}>Showing 23 groups plus the combined remainder. Use a table for full detail.</p>}
  </div>;
}
