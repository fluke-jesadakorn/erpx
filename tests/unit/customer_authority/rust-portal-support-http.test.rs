use erpx_server_runtime::public::http_support::{serde_json,serve};
use erpx_server_customer_authority::public::portal_support_commands::{StaffBackend,StaffCommand,StaffRequest,SupportFuture,SupportReply,SupportTarget,portal_support_staff_router};
use std::sync::{Arc,Mutex};
struct Probe{seen:Arc<Mutex<Vec<String>>>}
impl StaffBackend for Probe{fn execute<'a>(&'a self,r:StaffRequest<'a>)->SupportFuture<'a,SupportReply>{Box::pin(async move{let label=match &r.command{StaffCommand::Respond{request_id,input}=>format!("respond|{request_id}|{}|{}",input.expected_binding_version,input.expected_response_version),StaffCommand::Close{request_id,input}=>format!("close|{request_id}|{}",input.expected_binding_version)};self.seen.lock().unwrap().push(format!("{}|{}|{}|{}",r.host,r.holding_slug,r.key,label));Ok(SupportReply{data:serde_json::json!({"id":"receipt","requestId":"request","bindingId":"binding","version":1,"status":"published","publishedAt":1}),context:serde_json::json!({"userId":"user"})})})}}
fn target(seen:Arc<Mutex<Vec<String>>>)->SupportTarget{SupportTarget{host:"acme.example.test".into(),holding_slug:"acme".into(),origin:"https://acme.example.test".into(),backend:Arc::new(Probe{seen})}}
async fn http(addr:std::net::SocketAddr,raw:String)->(u16,serde_json::Value,String){use tokio::io::{AsyncReadExt,AsyncWriteExt};let mut s=tokio::net::TcpStream::connect(addr).await.unwrap();s.write_all(raw.as_bytes()).await.unwrap();let mut b=Vec::new();s.read_to_end(&mut b).await.unwrap();let t=String::from_utf8_lossy(&b).to_string();let status=t.lines().next().and_then(|l|l.split_whitespace().nth(1)).and_then(|v|v.parse().ok()).unwrap();let body=t.split("\r\n\r\n").nth(1).unwrap_or("");(status,serde_json::from_str(body).unwrap_or(serde_json::Value::Null),t)}
fn post(path:&str,key:Option<&str>,body:&str)->String{let mut h="Host: acme.example.test\r\nConnection: close\r\n".to_owned();if let Some(k)=key{h.push_str(&format!("Idempotency-Key: {k}\r\n"))}h.push_str(&format!("Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",body.len(),body));format!("POST {path} HTTP/1.1\r\n{h}")}
#[tokio::test]
async fn routes_forward_staff_response_and_closure(){
 let seen=Arc::new(Mutex::new(Vec::new()));let router=portal_support_staff_router(vec![target(seen.clone())]).unwrap();let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let addr=listener.local_addr().unwrap();let server=tokio::spawn(async move{let _=serve(listener,router).await;});
 let (status,body,text)=http(addr,post("/api/v1/holdings/acme/customer-access/support/request/respond",Some("portal-support-key-0001"),&serde_json::json!({"expectedBindingVersion":2,"expectedResponseVersion":0,"message":"Independent response","reason":"Reviewed request"}).to_string())).await;
 assert_eq!(status,200,"{text}");assert_eq!(body["data"]["status"],"published");
 let (status,_,text)=http(addr,post("/api/v1/holdings/acme/customer-access/support/request/close",Some("portal-support-key-0002"),&serde_json::json!({"expectedBindingVersion":2,"expectedResponseVersion":1,"reason":"Customer cannot acknowledge"}).to_string())).await;
 assert_eq!(status,200,"{text}");
 assert_eq!(seen.lock().unwrap().len(),2);
 let (status,_,text)=http(addr,post("/api/v1/holdings/acme/customer-access/support/request/respond",None,&serde_json::json!({"expectedBindingVersion":2,"expectedResponseVersion":0,"message":"Independent response","reason":"Reviewed request"}).to_string())).await;
 assert_eq!(status,422,"{text}");
 let (status,_,text)=http(addr,post("/api/v1/holdings/acme/customer-access/support/request/respond",Some("portal-support-key-0003"),&serde_json::json!({"expectedBindingVersion":0,"expectedResponseVersion":0,"message":"Independent response","reason":"Reviewed request"}).to_string())).await;
 assert_eq!(status,422,"{text}");
}
