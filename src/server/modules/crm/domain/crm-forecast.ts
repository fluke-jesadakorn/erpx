/** Per-opportunity half-up weighting, summed exactly and never across currencies. */
export function crmForecast(rows: ReadonlyArray<{opportunity_id?: unknown;stage?: unknown;currency?: unknown;expected_minor?: unknown}>) {
  const totals: Record<string,{unweightedMinor:bigint;weightedMinor:bigint;wonMinor:bigint}> = {};
  for (const row of rows) {
    if (!row.opportunity_id || row.stage === 'lost') continue;
    const value = BigInt(String(row.expected_minor));
    if (value < BigInt(0)) throw new Error('CRM estimates must be nonnegative.');
    const total = totals[String(row.currency)] ??= {unweightedMinor:BigInt(0),weightedMinor:BigInt(0),wonMinor:BigInt(0)};
    if (row.stage === 'won') total.wonMinor += value;
    else {
      const weight = row.stage === 'discovery' ? BigInt(25) : row.stage === 'proposal' ? BigInt(50) : row.stage === 'negotiation' ? BigInt(75) : null;
      if (weight === null) throw new Error('Unknown CRM forecast stage.');
      total.unweightedMinor += value;
      total.weightedMinor += (value * weight + BigInt(50)) / BigInt(100);
    }
  }
  return Object.fromEntries(Object.entries(totals).map(([currency,total])=>[currency,{
    unweightedMinor:total.unweightedMinor.toString(),weightedMinor:total.weightedMinor.toString(),wonMinor:total.wonMinor.toString(),
  }]));
}
/** CRM currently uses the owner's two-decimal currency contract. */
export function crmForecastDecimal(minor: string) {
  const amount = BigInt(minor);
  return `${amount / BigInt(100)}.${String(amount % BigInt(100)).padStart(2,'0')}`;
}
