#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_records::public::rfq_request_source::*;
use erpx_server_shared::public::CompanyId;
use sqlx::Connection;

#[tokio::test]
async fn exact_request_facts_and_absence_are_bound_to_one_locked_transaction() {
    fixture::with_database(2,|pool|async move {
        let mut c=pool.raw().acquire().await.unwrap();
        sqlx::raw_sql("CREATE TABLE public.employee_requests(id text PRIMARY KEY,company_id text,kind text,state text,evidence_version bigint,items_json text,approval_json text); INSERT INTO public.employee_requests VALUES('request','company','purchase','in_progress',7,repeat('x',2100000),'{\"decision\":\"approved\"}'),('foreign','other','purchase','in_progress',7,'[]','{}');").execute(&mut *c).await.unwrap();
        sqlx::raw_sql("ALTER TABLE public.employee_requests ADD COLUMN title text NOT NULL DEFAULT 'original', ADD COLUMN parent_request_id text, ADD COLUMN created_at bigint NOT NULL DEFAULT 0; CREATE TABLE employee_request_events(id text,company_id text,request_id text,action text,actor_id text,note text,evidence_json text,request_version bigint,created_at bigint); INSERT INTO employee_request_events VALUES('b','company','request','edit','actor','B','{}',2,1),('a','company','request','create','actor','A','{}',1,1),('foreign','other','request','edit','actor','foreign','{}',1,0); INSERT INTO employee_requests(id,company_id,kind,state,evidence_version,items_json,approval_json,parent_request_id,created_at) SELECT 'child-'||n,'company','purchase','draft',1,'[]','{}','request',n FROM generate_series(1,51) n").execute(&mut *c).await.unwrap();
        let company=CompanyId::new("company").unwrap();
        assert!(lock_rfq_request_sources(&mut c).await.is_err());
        let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
        assert!(observe_rfq_request(&mut tx,&company,"request").await.is_err());
        lock_rfq_request_sources(&mut tx).await.unwrap();
        let witness=observe_rfq_request(&mut tx,&company,"request").await.unwrap();
        let quotation=observe_rfq_quotation_row(&mut tx,&company,"request").await.unwrap();
        assert!(quotation.image_json().unwrap().len()>2_100_000);
        assert_eq!(quotation.company(),&company);assert_eq!(quotation.id(),"request");
        assert!(observe_rfq_quotation_row(&mut tx,&company,"foreign").await.unwrap().image_json().is_none());
        let facts=witness.facts().unwrap();assert_eq!(facts.kind,"purchase");assert_eq!(facts.state,"in_progress");assert_eq!(facts.evidence_version,7);assert_eq!(facts.items_json.len(),2_100_000);
        let absent=observe_rfq_request(&mut tx,&company,"foreign").await.unwrap();assert!(absent.facts().is_none());
        let events=read_rfq_request_events(&mut tx,&company,"request").await.unwrap();
        assert_eq!(events.iter().map(|e|e.id.as_str()).collect::<Vec<_>>(),["a","b"]);
        let children=read_rfq_request_children(&mut tx,&company,"request").await.unwrap();
        assert_eq!(children.len(),50);assert_eq!(children.first().unwrap().id,"child-1");assert_eq!(children.last().unwrap().id,"child-50");
        witness.require_current(&mut tx).await.unwrap();absent.require_current(&mut tx).await.unwrap();
        sqlx::query("UPDATE public.employee_requests SET title='changed' WHERE id='request'").execute(&mut *tx).await.unwrap();
        witness.require_current(&mut tx).await.unwrap();
        assert!(quotation.require_current(&mut tx).await.is_err());
        sqlx::query("UPDATE public.employee_requests SET title='original' WHERE id='request'").execute(&mut *tx).await.unwrap();
        quotation.require_current(&mut tx).await.unwrap();
        sqlx::query("UPDATE public.employee_requests SET approval_json='{\"decision\":\"rejected\"}' WHERE id='request'").execute(&mut *tx).await.unwrap();
        assert!(witness.require_current(&mut tx).await.is_err());
        assert!(quotation.require_current(&mut tx).await.is_err());
        sqlx::query("UPDATE public.employee_requests SET company_id='company' WHERE id='foreign'").execute(&mut *tx).await.unwrap();
        assert!(absent.require_current(&mut tx).await.is_err());
        tx.rollback().await.unwrap();
        let mut next=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_rfq_request_sources(&mut next).await.unwrap();
        assert!(witness.require_current(&mut next).await.is_err());assert!(quotation.require_current(&mut next).await.is_err());next.rollback().await.unwrap();
    }).await;
}
