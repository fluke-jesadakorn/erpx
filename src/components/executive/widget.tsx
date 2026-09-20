"use client";
import { Database, CircleAlert } from "lucide-react";
import type { MetricData, Widget } from "@/lib/executive/canvas";
import { MetricChart, formatMetric } from "./chart";
import styles from "./canvas.module.css";
export const metricValue=formatMetric;
export function CanvasWidget({widget,data,document,preview=false}:{widget:Widget;data?:MetricData;document?:string;preview?:boolean}) {
  return <div className={styles.widgetContent}><div className={styles.widgetHeading}><h3>{widget.title}</h3><span>{preview?"PREVIEW":widget.kind.toUpperCase()}</span></div>{widget.description&&<p className={styles.widgetDescription}>{widget.description}</p>}
    {preview?<div className={styles.previewMetric}><Database size={21}/><strong>{widget.metric.replaceAll("-"," ")}</strong><span>Live data loads when placed</span></div>:data?.error?<div className={styles.metricError}><CircleAlert size={20}/><p>{data.error}</p></div>:!data?<div className={styles.metricSkeleton} aria-label="Loading metric"/>:<>
    {widget.kind==="html"?document?<iframe title={`${widget.title} · custom HTML widget`} className={styles.htmlWidget} style={{colorScheme:"inherit"}} sandbox="" referrerPolicy="no-referrer" srcDoc={document}/>:<div className={styles.metricSkeleton} aria-label="Preparing custom widget"/>:<>
      <div className={styles.metricTotal}>{metricValue(data.total,data)}</div>
      {widget.kind==="kpi"&&!data.rows.length&&<p className={styles.widgetDescription}>No matching records for this scope and period.</p>}
      {widget.kind==="table"?<div className={styles.tableWrap}><table><thead><tr><th>Group</th><th>{data.unit==="money"?data.currency:"Count"}</th></tr></thead><tbody>{data.rows.map((r,i)=><tr key={`${r.label}-${i}`}><td>{r.label.replaceAll("_"," ")}</td><td>{metricValue(r.value,data)}</td></tr>)}</tbody></table></div>:widget.kind!=="kpi"&&<MetricChart widget={widget} data={data}/>}
    </>}
    <details className={styles.source}><summary><Database size={12}/> Source & definition</summary><p>{data.definition}</p><p>{data.scope} · Refreshed {new Date(data.updatedAt).toLocaleTimeString("en-GB")}</p></details>
    </>}
  </div>;
}
