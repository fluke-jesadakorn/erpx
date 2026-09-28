import {readFile} from 'node:fs/promises';
import {beforeAll,expect,it} from 'vitest';
import {applyMigrations,env} from './runtime-env';
import {expenseDb as db,expenseFixture,key} from './helpers/expense-fixture';

beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});

it('backfills separate identities for same-name legacy leads without changing opportunity or event history',async()=>{
 const f=await expenseFixture(),customer=key(),now=Date.now();
 await db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'customer',?,'Legacy customer','THB',?,?)").bind(customer,f.company,customer,now,now).run();
 const leadIds=[key(),key()],opportunityId=key(),eventIds=[key(),key()];
 for(const id of leadIds)await db.prepare("INSERT INTO crm_leads(id,company_id,branch_id,title,contact_name,contact_detail,source,brief,owner_id,due_date,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(id,f.company,f.branch,'Legacy inquiry','Shared Name','Same email on old record','Direct','Existing history',f.owner.userId,'2026-09-25',f.owner.userId,now,now).run();
 await db.prepare('INSERT INTO crm_opportunities(id,lead_id,customer_id,expected_minor,currency,close_date,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)').bind(opportunityId,leadIds[0],customer,5000,'THB','2026-09-25',f.owner.userId,now,now).run();
 for(let i=0;i<2;i++)await db.prepare('INSERT INTO crm_case_events(id,lead_id,opportunity_id,version,action,actor_id,reason,snapshot_json,created_at) VALUES(?,?,NULL,1,?,?,?,?,?)').bind(eventIds[i],leadIds[i],'create',f.owner.userId,'Original lead evidence',JSON.stringify({source:'Direct'}),now).run();
 const migration=await readFile('migrations/postgres/holding-write/0310_crm_marketing_analytics.sql','utf8');
 const backfill=migration.split('-- A legacy lead gets its own identity. Matching names or email addresses never merges people.')[1]?.split('CREATE INDEX crm_accounts_company')[0];
 expect(backfill).toBeTruthy();await db.pool.query(backfill!);
 const rows=(await db.prepare('SELECT l.id,l.account_id,l.contact_id,a.finance_party_id,a.reviewed_by,c.name FROM crm_leads l JOIN crm_accounts a ON a.id=l.account_id JOIN crm_contacts c ON c.id=l.contact_id WHERE l.id IN (?,?) ORDER BY l.id').bind(...leadIds).all()).results;
 expect(rows).toHaveLength(2);expect(rows[0].account_id).not.toBe(rows[1].account_id);expect(rows[0].contact_id).not.toBe(rows[1].contact_id);
 expect(rows.find(r=>r.id===leadIds[0])).toMatchObject({finance_party_id:customer,reviewed_by:'migration'});
 expect(Number(await db.prepare('SELECT count(*) FROM crm_case_events WHERE id IN (?,?)').bind(...eventIds).first('count'))).toBe(2);
 expect(await db.prepare('SELECT customer_id FROM crm_opportunities WHERE id=?').bind(opportunityId).first('customer_id')).toBe(customer);
});
