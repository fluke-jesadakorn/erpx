export type ChartType = 'line' | 'bar' | 'pie' | 'area';

export interface ChartSeries {
  name: string;
  data: number[];
}

export interface ChartSpec {
  type: ChartType;
  title?: string;
  series: ChartSeries[];
  axes?: { x?: string | string[]; y?: string };
}

const BLOCK = /\[CHART\]\s*([\s\S]*?)\s*\[\/CHART\]/g;

export function parseChartBlocks(text: string): { plain: string; charts: ChartSpec[] } {
  const charts: ChartSpec[] = [];
  if (!text) return { plain: '', charts };
  const plain = text.replace(BLOCK, (_m, body: string) => {
    try {
      const parsed = JSON.parse(body);
      if (parsed && Array.isArray(parsed.series)) {
        const spec: ChartSpec = {
          type: (parsed.type as ChartType) || 'bar',
          title: typeof parsed.title === 'string' ? parsed.title : undefined,
          series: parsed.series.map((s: any) => ({
            name: String(s?.name ?? ''),
            data: Array.isArray(s?.data) ? s.data.map((n: any) => Number(n) || 0) : [],
          })),
          axes: parsed.axes ?? undefined,
        };
        charts.push(spec);
      }
    } catch {}
    return '';
  });
  return { plain: plain.trim(), charts };
}