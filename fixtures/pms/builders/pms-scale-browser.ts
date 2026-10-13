import { randomBytes } from 'node:crypto';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { hashPassword } from 'better-auth/crypto';
import type { RuntimeEnv } from "../../../src/server/modules/runtime/adapters/runtime/env";
import { pmsCommand } from "../../../src/server/modules/pms/public/pms";
import { createBookingBrowserFixture } from "./booking-browser";
import { isTransactionalSqlDatabase } from "../../../src/server/modules/runtime/adapters/runtime/types";
import { pmsAccountKeys,type Config } from "../../../src/server/modules/pms/public/rules/pms";

/** Called only inside the exact disposable browser database launcher.
 * Reviewed configuration uses normal commands; volume history is synthetic
 * legacy metadata, not a claim that historical revenue was posted. */
export async function seedPmsScaleBrowser(env:RuntimeEnv,directory:string){
  if(process.env.ERPX_BROWSER_SIMULATION_PORT!=='3104')throw new Error('PMS_SCALE_REQUIRES_ISOLATED_RELEASE_PORT');
  const db=env.H001_DB;
  if(!isTransactionalSqlDatabase(db))throw new Error('PMS_SCALE_REQUIRES_OWNED_POSTGRES');
  const {company:f,owner,reviewer,fixture}=await createBookingBrowserFixture(env);
  const first=await db.prepare('SELECT business_date FROM pms_properties WHERE id=?').bind(fixture.propertyId).first<{business_date:string}>();
  const businessDate=first!.business_date;
  const command=async(propertyId:string,operation:string,input:unknown,actor=owner)=>{
    const row=await db.prepare('SELECT version FROM pms_properties WHERE id=?').bind(propertyId).first<{version:number}>();
    return pmsCommand(db,actor,{propertyId,expectedVersion:Number(row!.version),operation,input},crypto.randomUUID());
  };
  const properties=[{id:fixture.propertyId,typeId:fixture.typeId,rateId:fixture.rateId,roomId:fixture.roomId}];
  for(let n=1;n<3;n++){
    const branchId=`${f.company}-capacity-branch-${n}`;
    await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,?,?,?,?)")
      .bind(branchId,branchId,`Fictional hotel branch ${n+1}`,f.company,f.company,branchId).run();
    const id=String((await pmsCommand(db,owner,{operation:'property.create',input:{name:`Fictional Capacity Hotel ${n+1}`,branchId,businessDate}},crypto.randomUUID())).id);
    const accounts={} as Config['accounts'];
    for(const name of pmsAccountKeys){
      const accountId=`scale-account-${n}-${name}`;accounts[name]=accountId;
      const category=['revenue','service'].includes(name)?'revenue':['advance','deposit','tax'].includes(name)?'liability':['consumption','commission','cashDifference'].includes(name)?'expense':'asset';
      await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',1,1)")
        .bind(accountId,f.chart,f.company,`SCALE-${n}-${name}`,`Fictional hotel ${n+1} ${name}`,category).run();
    }
    await command(id,'accounting.save',{chartId:f.chart,accounts,taxBps:700,serviceBps:1000,taxInclusive:false,advanceTax:true});
    await command(id,'accounting.approve',{},reviewer);
    const typeId=String((await command(id,'room_type.save',{name:'Family room',capacity:3,amenities:'Fictional capacity fixture'})).id);
    const rateId=String((await command(id,'rate.save',{name:'Family flexible',typeId,from:businessDate,to:new Date(Date.parse(businessDate)+120*86400000).toISOString().slice(0,10),price:300000,includedGuests:3,childBands:[{minAge:0,maxAge:17,price:0}]})).id);
    properties.push({id,typeId,rateId,roomId:''});
  }
  for(const [propertyIndex,p] of properties.entries()){
    const roomIds=Array.from({length:200},(_,n)=>n===0&&p.roomId?p.roomId:`scale-room-${propertyIndex}-${n}`);
    for(let start=0;start<200;start+=50){
      const statements=[];
      // Bind each row directly. Keep volume seeding independent of callback
      // closure capture across the long asynchronous history loop.
      for(let index=start;index<Math.min(start+50,200);index++){
        const id=roomIds[index];
        if(id===p.roomId)continue;
        statements.push(db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'room',?,1,1)")
          .bind(id,p.id,JSON.stringify({number:String(101+index),typeId:p.typeId,active:true,cleanliness:'inspected',cleaningCycle:0,dnd:false})));
      }
      await db.batch(statements);
    }
    const roomProof=await db.prepare("SELECT count(*)::int rooms,count(*) FILTER(WHERE data_json::jsonb->>'typeId'=? AND data_json::jsonb->>'active'='true')::int scoped FROM pms_records WHERE property_id=? AND kind='room'")
      .bind(p.typeId,p.id).first<{rooms:number;scoped:number}>();
    if(roomProof?.rooms!==200||roomProof.scoped!==200)throw new Error('EXACT_PMS_CAPACITY_ROOM_SCOPE_REQUIRED');
    if(propertyIndex)await command(p.id,'property.enable',{});
    const guestId=`scale-history-guest-${propertyIndex}`;
    await db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'guest',?,1,1)")
      .bind(guestId,p.id,JSON.stringify({name:'Fictional archived guest',email:'history@example.test',phone:'020000000',nationality:'',documentRef:'',notes:''})).run();
    // 50% historical occupancy: 100 one-night stays/day × 730 days/property.
    // Small SQL chunks retain the normal statement timeout and bounded memory.
    for(let start=0;start<73000;start+=500){
      await db.prepare(`WITH history AS (
        SELECT n,?::date-(730-(n/100)) arrival,?::jsonb->>(n%100) room_id FROM generate_series(?::int,?::int) n
      ), facts AS (
        SELECT 'scale-history-'||?::text||'-'||n prefix,arrival,room_id FROM history
      ) INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at)
      SELECT prefix||'-reservation',?::text,'reservation',jsonb_build_object('number',prefix,'guestId',?::text,'source','legacy','externalRef',prefix,'state','confirmed','history','[]'::jsonb)::text,1,1 FROM facts
      UNION ALL SELECT prefix||'-account',?::text,'guest_account',jsonb_build_object('name','Fictional archived account','guestId',?::text,'reservationId',prefix||'-reservation','partyId','','state','closed','due',0,'advance',0,'advanceTax',0,'deposit',0)::text,1,1 FROM facts
      UNION ALL SELECT prefix||'-stay',?::text,'stay',jsonb_build_object('reservationId',prefix||'-reservation','guestId',?::text,'typeId',?::text,'rateId',?::text,'roomId',room_id,'arrival',arrival,'departure',arrival+1,'adults',2,'childAges','[]'::jsonb,'state','checked_out','guestAccountId',prefix||'-account','prices',jsonb_build_array(jsonb_build_object('date',arrival,'net',300000,'service',30000,'tax',23100,'total',353100)),'segments',jsonb_build_array(jsonb_build_object('roomId',room_id,'from',arrival,'to',arrival+1)))::text,1,1 FROM facts`)
        .bind(businessDate,JSON.stringify(roomIds),start,Math.min(start+499,72999),propertyIndex,p.id,guestId,p.id,guestId,p.id,guestId,p.typeId,p.rateId).run();
      if((start+500)%10000===0||start+500===73000)console.log(JSON.stringify({event:'pms_scale_history_seeded',property:propertyIndex+1,completedStays:start+500}));
    }
  }
  const password=`Scale-${randomBytes(20).toString('hex')}`,hash=await hashPassword(password),now=Date.now(),users=[];
  for(let n=0;n<20;n++){
    const role=n<16?'pms_frontdesk':'pms_manager';
    const actor=await f.member(`capacity-frontdesk-${n}`,role),email=`capacity-frontdesk-${n}@example.test`;
    await db.prepare("UPDATE assignments SET modules_json='[\"pms\"]' WHERE id=?").bind(actor.assignmentId).run();
    await db.prepare('UPDATE hr_people SET email=? WHERE user_id=? AND company_id=?').bind(email,actor.userId,f.company).run();
    await env.CONTROL_DB.batch([
      env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?)').bind(actor.userId,`Fictional capacity desk ${n}`,email,now,now),
      env.CONTROL_DB.prepare("INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,'credential','local:credential',?,?,?,?)").bind(`account-${actor.userId}`,actor.userId,actor.userId,hash,now,now),
      env.CONTROL_DB.prepare("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?)").bind(`principal-${actor.userId}`,actor.userId,`Fictional capacity desk ${n}`,now),
    ]);
    users.push({userId:actor.userId,email,role,assignmentId:actor.assignmentId});
  }
  const counts=[];
  for(const p of properties)counts.push(await db.prepare("SELECT count(*) FILTER (WHERE kind='room')::int rooms,count(*) FILTER (WHERE kind='stay')::int stays,count(*) FILTER (WHERE kind='reservation')::int reservations,count(*) FILTER (WHERE kind='guest_account')::int accounts FROM pms_records WHERE property_id=?").bind(p.id).first());
  if(counts.some(row=>!row||row.rooms!==200||row.stays!==73000||row.reservations!==73000||row.accounts!==73000))throw new Error('EXACT_PMS_CAPACITY_DATASET_REQUIRED');
  await writeFile(join(directory,'pms-scale.json'),JSON.stringify({companyId:f.company,holdingSlug:'booking-test',businessDate,properties,counts,historyDays:730,concurrentAccounts:20,users,password},null,2)+'\n',{mode:0o600});
  return {...fixture,scale:{properties:3,roomsPerProperty:200,historyDays:730,completedStays:219000,concurrentAccounts:20}};
}
