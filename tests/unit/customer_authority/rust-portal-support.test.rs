//! Native portal-support command validation/ingress.
use erpx_server_customer_authority::public::portal_support_commands::{AcknowledgeInput,AcknowledgementReceipt,CloseInput,CustomerActor,Receipt,RespondInput,SupportActor,replay,replay_acknowledgement};
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

#[test]
fn acknowledgement_hash_keeps_the_retained_customer_field_order(){
 let customer=CustomerActor{user_id:"customer".into(),email:"Customer@Example.Invalid".into()};
 let input=AcknowledgeInput{binding_id:"binding-1".into(),expected_binding_version:2,response_id:"response-1".into(),response_version:1,note:"Thanks".into()}.validate().unwrap();
 assert_eq!(input.hash(&customer),"{\"userId\":\"customer\",\"email\":\"customer@example.invalid\",\"input\":{\"bindingId\":\"binding-1\",\"expectedBindingVersion\":2,\"responseId\":\"response-1\",\"responseVersion\":1,\"note\":\"Thanks\"}}");
}
#[test]
fn acknowledgement_receipt_replay_requires_the_exact_acknowledged_shape(){
 let receipt=AcknowledgementReceipt{id:"ack".into(),request_id:"request".into(),binding_id:"binding".into(),response_id:"response".into(),response_version:1,created_at:1};
 let wire=receipt.wire();assert_eq!(wire["status"],"acknowledged");assert_eq!(wire["responseVersion"],1);
 assert_eq!(replay_acknowledgement(&wire.to_string()).unwrap(),receipt);
 assert!(replay_acknowledgement("{}").is_err());
 assert!(replay_acknowledgement(&serde_json::json!({"id":"a","requestId":"q","bindingId":"b","responseId":"r","responseVersion":1,"status":"published","createdAt":1}).to_string()).is_err());
}
