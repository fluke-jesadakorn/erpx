import type { CanvasDefinition } from "@/lib/shared/executive/canvas";

// The embedding page selects the scheme; the same saved design serves both appearances.
export const chartColors = [
  "light-dark(#7550c8,#a78bfa)", "light-dark(#087dad,#38bdf8)",
  "light-dark(#158264,#34d399)", "light-dark(#a66a08,#fbbf24)",
  "light-dark(#c44560,#fb7185)", "light-dark(#595ac6,#818cf8)",
  "light-dark(#0b827b,#2dd4bf)",
];

export function canvasAccent(accent: CanvasDefinition["accent"]) {
  return {violet:chartColors[0],blue:"light-dark(#306ed2,#60a5fa)",emerald:chartColors[2],amber:chartColors[3],rose:chartColors[4]}[accent];
}

/** Public styling contract for generated HTML/CSS, confined to each sandboxed document. */
export const widgetThemeCss = `
:root {
  color-scheme: light dark;
  --widget-bg: light-dark(#faf9f6,#171b27);
  --widget-surface: light-dark(#ffffff,#222735);
  --widget-surface-alt: light-dark(#f0eef6,#2b3040);
  --widget-text: light-dark(#272632,#eef0fa);
  --widget-muted: light-dark(#625f70,#acb1c5);
  --widget-border: light-dark(#dedce7,#3b4155);
  --widget-track: light-dark(#e8e5ef,#ffffff15);
  --widget-shadow: light-dark(#24203012,#00000030);
  --widget-positive: light-dark(#14785c,#6bd4b2);
  --widget-negative: light-dark(#b53850,#ff9eb0);
  ${chartColors.map((color,i)=>`--chart-${i+1}: ${color};`).join("\n  ")}
}
`;
