//! Native portal-support command validation/ingress.
use erpx_server_customer_authority::public::portal_support_commands::{AcknowledgeInput,CloseInput,Receipt,RespondInput,SupportActor,replay};
fn actor()->SupportActor{SupportActor{user_id:"manager".into(),company_id:"company".into(),branch_id:Some("branch".into()),department_id:None,assignment_id:"assignment".into(),role:"sales_manager".into(),session_id:None,assignment_version:None,permissions:vec!["commerce.read".into(),"commerce.sales".into(),"commerce.approve".into()]}}
#[test]
fn response_acknowledge_and_closure_validation_are_bounded_and_exact(){
 assert!(RespondInput{expected_binding_version:1,expected_response_version:0,message:"Independent response".into(),reason:"Reviewed request".into()}.validate().is_ok());
 assert!(RespondInput{expected_binding_version:0,expected_response_version:0,message:"Independent response".into(),reason:"Reviewed request".into()}.validate().is_err());
 assert!(RespondInput{expected_binding_version:1,expected_response_version:0,message:"x".into(),reason:"Reviewed request".into()}.validate().is_err());
 assert!(CloseInput{expected_binding_version:1,expected_response_version:0,reason:"x".into()}.validate().is_err());
 assert!(AcknowledgeInput{binding_id:"binding".into(),expected_binding_version:1,response_id:"response".into(),response_version:1,note:String::new()}.validate().is_ok());
 let identity=RespondInput{expected_binding_version:1,expected_response_version:0,message:"Independent response".into(),reason:"Reviewed request".into()}.identity(&actor(),"request","portal-support-0001");
 assert!(identity.contains("\"operation\":\"customer.portal.support.respond\""));
}
#[test]
fn receipt_replay_requires_the_exact_status_shape(){
 let receipt=Receipt{id:"response".into(),request_id:"request".into(),binding_id:"binding".into(),version:1,status:"published".into(),published_at:1};
 assert_eq!(replay(&receipt.wire().to_string()).unwrap(),receipt);
 assert!(replay("{}").is_err());
 assert!(replay(&serde_json::json!({"id":"r","requestId":"q","bindingId":"b","version":1,"status":"other","publishedAt":1}).to_string()).is_err());
}
