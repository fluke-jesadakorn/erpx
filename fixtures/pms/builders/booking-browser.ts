import { writeFile } from "node:fs/promises";
import { join } from "node:path";
import { randomBytes } from "node:crypto";
import { hashPassword } from "better-auth/crypto";
import type { RuntimeEnv } from "@erpx/server-runtime/adapters/runtime/env";
import { seedExpenseCompany } from "../../expenses/company";
import { loadPms, pmsCommand } from "../../../packages/server-pms/src/public/pms";
import { pmsAccountKeys, type Config } from "@erpx/server-pms/public/rules/pms";
import { isTransactionalSqlDatabase } from "@erpx/server-runtime/adapters/runtime/types";

/** Normal PMS commands establish prerequisites; the browser creates the stay. */
export async function createBookingBrowserFixture(env: RuntimeEnv) {
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
  const roomId = String((await command("room.save", { number: "101", typeId, active: true })).id);
  const rateId = String((await command("rate.save", { name: "Family flexible", typeId, from: rateStart.toISOString().slice(0, 10), to: rateEnd.toISOString().slice(0, 10), price: 300000, includedGuests: 3, childBands: [{ minAge: 0, maxAge: 17, price: 0 }] })).id);
  await command("property.enable", {});
  await command("booking.save", { published: true, description: "Fictional isolated booking acceptance hotel", address: "Bangkok", contactEmail: "hotel@example.test", contactPhone: "020000000", cancellationDays: 1, cancellationTime: "18:00", rateIds: [rateId], roomContent: [{ typeId, description: "Family room for two adults and one child" }] });
  return { company: fixture, owner, reviewer, accounts,
    fixture: { profile: "booking", holdingSlug: "booking-test", companyId: fixture.company, propertyId, typeId, rateId, roomId } };
}
export async function seedBookingBrowser(env: RuntimeEnv, directory:string) {
  if(!/\/\.runtime\/browser\/[a-f0-9]{12}$/.test(directory))throw new Error('EXACT_OWNED_BOOKING_DIRECTORY_REQUIRED');
  const created=await createBookingBrowserFixture(env), mediaReviewer=await created.company.member('booking-media-reviewer','holding_owner');
  if(process.env.ERPX_PMS_PROVIDER_FIXTURE==='1'){
    const db=env.H001_DB;if(!isTransactionalSqlDatabase(db))throw new Error('FICTIONAL_PROVIDER_POSTGRES_REQUIRED');
    if(process.env.POS_STRIPE_TEST_SECRET_KEY!=='sk_test_erpx_fictional_provider'||process.env.ERPX_PUBLIC_ORIGIN!=='http://127.0.0.1:3104')throw new Error('EXACT_FICTIONAL_BOOKING_PROVIDER_REQUIRED');
    // Declared fictional Connect facts are setup only, never Stripe acceptance.
    await env.H001_DB.prepare("INSERT INTO pos_connections(company_id,account_id,livemode,charges_enabled,payouts_enabled,card_enabled,promptpay_enabled,state,payout_interval,thb_bank_ready) VALUES(?,'acct_erpx_fictional',false,true,true,true,true,'ready','daily',true)").bind(created.fixture.companyId).run();
    await env.CONTROL_DB.prepare("INSERT INTO pos_stripe_routes(account_id,livemode,holding_id,company_id,created_at) VALUES('acct_erpx_fictional',false,'h001',?,?)").bind(created.fixture.companyId,Date.now()).run();
    const command=async(operation:string,input:unknown)=>{
      const snapshot=await loadPms(env.H001_DB,created.owner,created.fixture.propertyId);
      return pmsCommand(db,created.owner,{propertyId:created.fixture.propertyId,expectedVersion:snapshot.property.version,operation,input},crypto.randomUUID());
    };
    await command('room.save',{number:'102',typeId:created.fixture.typeId,active:true});
    const snapshot=await loadPms(env.H001_DB,created.owner,created.fixture.propertyId),rate=snapshot.records.find(record=>record.id===created.fixture.rateId)!;
    await command('rate.save',{...rate.data,id:rate.id,collection:{method:'full',depositPercent:30,changesAllowed:true}});
  }
  const password=`Booking-${randomBytes(20).toString('hex')}`,hash=await hashPassword(password),now=Date.now();
  const staff=[{actor:created.owner,label:'Gallery author'},{actor:mediaReviewer,label:'Independent gallery reviewer'}];
  const users=[];
  for(const {actor,label} of staff){
    const email=`${actor.userId}@example.test`;
    await env.H001_DB.prepare('UPDATE hr_people SET legal_name=?,preferred_name=?,email=? WHERE company_id=? AND user_id=?').bind(label,label,email,created.fixture.companyId,actor.userId).run();
    await env.CONTROL_DB.batch([
      env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?)').bind(actor.userId,label,email,now,now),
      env.CONTROL_DB.prepare("INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,'credential','local:credential',?,?,?,?)").bind(`account-${actor.userId}`,actor.userId,actor.userId,hash,now,now),
      env.CONTROL_DB.prepare("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?)").bind(`principal-${actor.userId}`,actor.userId,label,now),
    ]);
    users.push({label,email,userId:actor.userId,role:actor.role,assignmentId:actor.assignmentId});
  }
  await writeFile(join(directory,'credentials.json'),JSON.stringify({password,users})+'\n',{mode:0o600});
  return {...created.fixture,staff:{authorId:created.owner.userId,reviewerId:mediaReviewer.userId}};
}
