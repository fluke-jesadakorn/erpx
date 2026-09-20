/** Date formatting shared with Home, without loading calendar validation schemas. */
export function today(timeZone = "Asia/Bangkok") {
  return new Intl.DateTimeFormat("en-CA", {
    timeZone,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(new Date());
}
