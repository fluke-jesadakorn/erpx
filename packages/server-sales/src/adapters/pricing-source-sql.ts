/** One closed price-list head shared by the Sales pricing repository. The tail
 * is owner-local fixed SQL composed by the caller; no caller input enters the
 * rendered statement. */
export const priceListSelect=(tail:string)=>`SELECT * FROM sales_price_lists ${tail}`;
