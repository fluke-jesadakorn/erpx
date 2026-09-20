/** Grid geometry shared by business widgets and Executive's report canvas. */
export function resizeGrid(
  columns: number,
  height: number,
  dx: number,
  dy: number,
  gridWidth: number,
  edge: string,
  bounds = { minColumns: 2, minHeight: 240, maxHeight: 900 },
) {
  const width = Math.max(1, gridWidth);
  return {
    columns: Math.max(
      bounds.minColumns,
      Math.min(
        12,
        Math.round(
          columns +
            (edge === "bottom"
              ? 0
              : (dx * (edge === "left" ? -1 : 1)) / (width / 12)),
        ),
      ),
    ),
    height: Math.max(
      bounds.minHeight,
      Math.min(
        bounds.maxHeight,
        Math.round(height + (edge === "right" || edge === "left" ? 0 : dy)),
      ),
    ),
  };
}
