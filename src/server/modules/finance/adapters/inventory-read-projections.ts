/** Fixed owner facts used by native joined Inventory projections; application ports return typed rows. */
export const inventoryReadProjection={
countCurrency:"JOIN finance_charts chart ON chart.id=s.chart_id AND chart.company_id=s.company_id",
transferCurrency:"JOIN finance_charts chart ON chart.id=c.chart_id AND chart.company_id=c.company_id"
} as const;
