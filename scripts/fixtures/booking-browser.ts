import type { RuntimeEnv } from "../../src/server/runtime/env";
import { seedExpenseCompany } from "../../fixtures/expenses/company";
import { loadPms, pmsCommand } from "../../src/server/pms";
import { pmsAccountKeys, type Config } from "../../src/lib/pms";
import { isTransactionalSqlDatabase } from "../../src/server/runtime/types";

/** Normal PMS commands establish prerequisites; the browser creates the stay. */
export async function seedBookingBrowser(env: RuntimeEnv) {
  const db = env.H001_DB;
  if (!isTransactionalSqlDatabase(db)) throw new Error("BOOKING_FIXTURE_REQUIRES_OWNED_POSTGRES");
  const fixture = await seedExpenseCompany(db, env.H001_FILES), owner = fixture.owner, reviewer = await fixture.member("booking-reviewer", "finance_approver"), key = () => crypto.randomUUID();
  const today = new Date(), rateStart = new Date(today), rateEnd = new Date(today);
  rateStart.setUTCDate(rateStart.getUTCDate() - 30); rateEnd.setUTCDate(rateEnd.getUTCDate() + 120);
  const propertyId = String((await pmsCommand(db, owner, { operation: "property.create", input: { name: "Isolated Family Hotel", branchId: fixture.branch, businessDate: today.toISOString().slice(0, 10) } }, key())).id);
  const command = async (operation: string, input: unknown, actor = owner) => {
    const snapshot = await loadPms(db, owner, propertyId);
    return pmsCommand(db, actor, { propertyId, expectedVersion: snapshot.property.version, operation, input }, key());
  };
  const accounts = {} as Config["accounts"];
  for (const [index, name] of pmsAccountKeys.entries()) {
    const id = `booking-account-${index}`; accounts[name] = id;
    const category = ["revenue", "service"].includes(name) ? "revenue" : ["advance", "deposit", "tax"].includes(name) ? "liability" : ["consumption", "commission", "cashDifference"].includes(name) ? "expense" : "asset";
    await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',1,1)").bind(id, fixture.chart, fixture.company, `BOOK-${index}`, name, category).run();
  }
  await command("accounting.save", { chartId: fixture.chart, accounts, taxBps: 700, serviceBps: 1000, taxInclusive: false, advanceTax: true });
  await command("accounting.approve", {}, reviewer);
  for (const name of ["refund", "unallocated"]) await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,'liability','THB',1,1)").bind(`booking-${name}`, fixture.chart, fixture.company, `BOOK-${name}`, name).run();
  await command("online_accounting.prepare", { refundPayable: "booking-refund", unallocated: "booking-unallocated" });
  await command("online_accounting.approve", {}, reviewer);
  const typeId = String((await command("room_type.save", { name: "Family room", capacity: 3, amenities: "Fictional family room" })).id);
  await command("room.save", { number: "101", typeId, active: true });
  const rateId = String((await command("rate.save", { name: "Family flexible", typeId, from: rateStart.toISOString().slice(0, 10), to: rateEnd.toISOString().slice(0, 10), price: 300000, includedGuests: 3, childBands: [{ minAge: 0, maxAge: 17, price: 0 }] })).id);
  await command("property.enable", {});
  await command("booking.save", { published: true, description: "Fictional isolated booking acceptance hotel", address: "Bangkok", contactEmail: "hotel@example.test", contactPhone: "020000000", cancellationDays: 1, cancellationTime: "18:00", rateIds: [rateId], roomContent: [{ typeId, description: "Family room for two adults and one child" }] });
  return { profile: "booking", holdingSlug: "booking-test", companyId: fixture.company, propertyId, typeId, rateId };
}
