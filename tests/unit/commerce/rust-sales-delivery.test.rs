//! Native Commerce sales goods/mixed delivery domain facts: bounded command
//! validation, replay identity, tracking/allocation invariants, Inventory plan
//! cross-checks, fulfillment ordering, persisted readback and wire output.
#![allow(dead_code)]
use erpx_server_commerce::public::receipt::{
    CommerceReceipt, CommerceReceiptLine, GoodsValuation, ReceiptFulfillmentLineReadback,
    ReceiptFulfillmentReadback, ReceiptOrder, ReceiptOrderLine, ReceiptReadback,
    ReceiptReadbackLine, ReceiptStockMoveReadback, ReceiptStockReadback,
};
use erpx_server_commerce::public::sales_delivery::{
    PreparedSalesDelivery, SalesDeliveryAllocationCommand, SalesDeliveryCommand,
    SalesDeliveryDisposition, SalesDeliveryFulfillmentPlan, SalesDeliveryLineCommand,
    SalesDeliveryLineValue, SalesDeliveryMode, SalesDeliveryOrder, SalesDeliveryPlanAllocation,
    SalesDeliveryPlanLine, SalesDeliveryStockApplied, SalesDeliveryStockAppliedAllocation,
    SalesDeliveryStockAppliedLine, apply_prepared_delivery, build_sales_delivery_fulfillment,
    sales_delivery_stock_request, sales_delivery_valuation_plan, sales_delivery_values,
    sales_delivery_wire, validate_sales_delivery_readback,
};
use erpx_server_shared::public::{
    AssignmentId, CommandKey, CompanyId, HoldingId, RecordId, TimestampMillis, UserId,
};

const NOW: i64 = 1_800_000_000_000;

fn actor() -> erpx_server_commerce::public::order::CommerceOrderActor {
    erpx_server_commerce::public::order::CommerceOrderActor {
        user_id: UserId::new("seller").unwrap(),
        person_user_ids: vec![],
        holding_id: HoldingId::new("holding").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: Some(RecordId::new("branch").unwrap()),
        department_id: None,
        assignment_id: AssignmentId::new("assignment-seller").unwrap(),
        role: Some("warehouse_manager".into()),
        authority_revision: Some(1),
        assignment_version: Some(1),
        permissions: vec!["commerce.read".into(), "inventory.prepare".into()],
        modules: vec!["inventory".into()],
        expires_at: 2_000_000_000_000,
    }
}

fn line(
    id: &str,
    product: &str,
    item_type: &str,
    quantity: i64,
    fulfilled: i64,
    price: i64,
    tax_bps: i64,
    subtotal: i64,
    tax: i64,
    tracking: &str,
) -> ReceiptOrderLine {
    ReceiptOrderLine {
        id: id.into(),
        product_id: product.into(),
        item_type: item_type.into(),
        description: product.into(),
        quantity_milli: quantity,
        fulfilled_milli: fulfilled,
        price_minor: price,
        tax_bps,
        subtotal_minor: subtotal,
        tax_minor: tax,
        prior_subtotal_minor: 0,
        prior_tax_minor: 0,
        tracking_mode: tracking.into(),
    }
}

/// Untracked stock line + lot-tracked stock line + service line.
fn order() -> SalesDeliveryOrder {
    ReceiptOrder {
        id: "sales-1".into(),
        state: "confirmed".into(),
        version: 3,
        warehouse_id: "warehouse-1".into(),
        branch_id: "branch".into(),
        department_id: None,
        currency: "THB".into(),
        base_currency: "THB".into(),
        order_date: "2026-11-01".into(),
        lines: vec![
            line(
                "line-stock",
                "product-stock",
                "stock",
                5_000,
                0,
                1_000,
                0,
                5_000,
                0,
                "none",
            ),
            line(
                "line-lot",
                "product-lot",
                "stock",
                2_000,
                0,
                500,
                0,
                1_000,
                0,
                "lot",
            ),
            line(
                "line-svc",
                "product-svc",
                "service",
                1_000,
                0,
                2_000,
                700,
                2_000,
                140,
                "none",
            ),
        ],
    }
}

fn allocation(lot: &str, quantity: i64) -> SalesDeliveryAllocationCommand {
    SalesDeliveryAllocationCommand {
        lot_id: Some(lot.into()),
        lot_code: None,
        serial_number: None,
        quantity_milli: quantity,
        disposition: SalesDeliveryDisposition::Available,
        quarantine_reason: None,
        quarantine_due_date: None,
    }
}

fn command(key: &str, mode: SalesDeliveryMode) -> SalesDeliveryCommand {
    SalesDeliveryCommand {
        key: CommandKey::new(key).unwrap(),
        order_id: RecordId::new("sales-1").unwrap(),
        mode,
        expected_version: 3,
        date: "2026-11-05".into(),
        reason: Some("Delivered to the customer dock".into()),
        lines: vec![
            SalesDeliveryLineCommand {
                line_id: "line-stock".into(),
                quantity_milli: 2_000,
                allocations: vec![],
            },
            SalesDeliveryLineCommand {
                line_id: "line-lot".into(),
                quantity_milli: 1_000,
                allocations: vec![allocation("lot-1", 1_000)],
            },
        ],
    }
}

fn code(error: erpx_server_shared::public::BusinessError) -> String {
    error.code.to_owned()
}

fn prepared() -> PreparedSalesDelivery<()> {
    PreparedSalesDelivery {
        plan: (),
        order_version: 3,
        warehouse_id: "warehouse-1".into(),
        branch_id: "branch".into(),
        total_value_minor: 2_500,
        lines: vec![
            SalesDeliveryPlanLine {
                line_id: "line-stock".into(),
                product_id: "product-stock".into(),
                tracking_mode: "none".into(),
                ordered_milli: 5_000,
                quantity_milli: 2_000,
                stock_value_minor: 2_000,
                expected_fulfilled_milli: 0,
                stock_version: 5,
                allocations: vec![],
            },
            SalesDeliveryPlanLine {
                line_id: "line-lot".into(),
                product_id: "product-lot".into(),
                tracking_mode: "lot".into(),
                ordered_milli: 2_000,
                quantity_milli: 1_000,
                stock_value_minor: 500,
                expected_fulfilled_milli: 0,
                stock_version: 4,
                allocations: vec![SalesDeliveryPlanAllocation {
                    lot_id: "lot-1".into(),
                    lot_code: Some("L1".into()),
                    serial_number: None,
                    quantity_milli: 1_000,
                    value_minor: 500,
                }],
            },
        ],
    }
}

#[test]
fn goods_command_binds_identity_and_rejects_bounded_or_duplicate_input() {
    let command = command("delivery-unit-one", SalesDeliveryMode::Goods);
    command.validate().unwrap();
    let identity = command.identity(&actor()).unwrap();
    assert!(identity.contains("\"completionMode\":\"goods\""));
    assert!(identity.contains("\"orderId\":\"sales-1\""));
    assert!(identity.contains("\"lotId\":\"lot-1\""));

    let mut duplicate = command.clone();
    duplicate.lines[1].line_id = "line-stock".into();
    assert_eq!(
        code(duplicate.validate().unwrap_err()),
        "COMMERCE_DELIVERY_REQUEST_INVALID"
    );

    let mut zero = command.clone();
    zero.lines[0].quantity_milli = 0;
    assert_eq!(
        code(zero.validate().unwrap_err()),
        "COMMERCE_DELIVERY_REQUEST_INVALID"
    );

    let mut reason = command.clone();
    reason.reason = Some("   ".into());
    assert_eq!(
        code(reason.validate().unwrap_err()),
        "COMMERCE_DELIVERY_REQUEST_INVALID"
    );
}

#[test]
fn goods_mode_rejects_service_and_quarantine_evidence() {
    let mut with_service = command("delivery-unit-service", SalesDeliveryMode::Goods);
    with_service.lines.push(SalesDeliveryLineCommand {
        line_id: "line-svc".into(),
        quantity_milli: 1_000,
        allocations: vec![],
    });
    with_service.validate().unwrap();
    assert_eq!(
        code(sales_delivery_values(&order(), &with_service).unwrap_err()),
        "SERVICE_COMPLETION_REQUIRED"
    );

    let mut quarantined = command("delivery-unit-quarantine", SalesDeliveryMode::Goods);
    quarantined.lines[1].allocations[0].disposition = SalesDeliveryDisposition::Quarantine;
    assert_eq!(
        code(quarantined.validate().unwrap_err()),
        "QUARANTINE_DELIVERY_FORBIDDEN"
    );

    let mut reason_only = command("delivery-unit-quarantine-reason", SalesDeliveryMode::Goods);
    reason_only.lines[1].allocations[0].quarantine_reason = Some("Damaged".into());
    assert_eq!(
        code(reason_only.validate().unwrap_err()),
        "QUARANTINE_DELIVERY_FORBIDDEN"
    );
}

#[test]
fn tracked_lines_require_the_exact_current_lot_allocations() {
    let mut missing = command("delivery-unit-missing-lot", SalesDeliveryMode::Goods);
    missing.lines[1].allocations.clear();
    missing.validate().unwrap();
    assert_eq!(
        code(sales_delivery_values(&order(), &missing).unwrap_err()),
        "LOT_ALLOCATION_REQUIRED"
    );

    let mut partial = command("delivery-unit-partial-lot", SalesDeliveryMode::Goods);
    partial.lines[1].allocations[0].quantity_milli = 500;
    assert_eq!(
        code(sales_delivery_values(&order(), &partial).unwrap_err()),
        "LOT_ALLOCATION_REQUIRED"
    );

    let mut duplicate = command("delivery-unit-duplicate-lot", SalesDeliveryMode::Goods);
    duplicate.lines[1]
        .allocations
        .push(allocation("lot-1", 1_000));
    duplicate.lines[1].allocations[0].quantity_milli = 500;
    assert_eq!(
        code(duplicate.validate().unwrap_err()),
        "LOT_ALLOCATION_DUPLICATE"
    );

    let mut serial_order = order();
    serial_order.lines[1].tracking_mode = "serial".into();
    let mut serial = command("delivery-unit-serial", SalesDeliveryMode::Goods);
    serial.lines[1].quantity_milli = 1_000;
    serial.lines[1].allocations[0].quantity_milli = 500;
    assert_eq!(
        code(sales_delivery_values(&serial_order, &serial).unwrap_err()),
        "SERIAL_QUANTITY_INVALID"
    );
}

#[test]
fn over_fulfillment_missing_line_and_stale_version_fail_closed() {
    let mut over = command("delivery-unit-over", SalesDeliveryMode::Goods);
    over.lines[0].quantity_milli = 5_001;
    over.validate().unwrap();
    assert_eq!(
        code(sales_delivery_values(&order(), &over).unwrap_err()),
        "OVER_FULFILLMENT"
    );

    let mut unknown = command("delivery-unit-unknown", SalesDeliveryMode::Goods);
    unknown.lines[0].line_id = "line-missing".into();
    assert_eq!(
        code(sales_delivery_values(&order(), &unknown).unwrap_err()),
        "COMMERCE_ORDER_CHANGED"
    );

    let mut stale = command("delivery-unit-stale", SalesDeliveryMode::Goods);
    stale.expected_version = 2;
    assert_eq!(
        code(sales_delivery_values(&order(), &stale).unwrap_err()),
        "COMMERCE_ORDER_CHANGED"
    );

    let mut draft = order();
    draft.state = "draft".into();
    assert_eq!(
        code(
            sales_delivery_values(
                &draft,
                &command("delivery-unit-draft", SalesDeliveryMode::Goods)
            )
            .unwrap_err()
        ),
        "COMMERCE_ORDER_CHANGED"
    );
}

#[test]
fn prepared_plan_fills_authoritative_costs_and_rejects_substitution() {
    let command = command("delivery-unit-plan", SalesDeliveryMode::Goods);
    let mut values = sales_delivery_values(&order(), &command).unwrap();
    let request = sales_delivery_stock_request(&command, &values).unwrap();
    assert_eq!(request.lines.len(), 2);
    assert_eq!(request.lines[1].allocations[0].lot_id, "lot-1");

    let plan = prepared();
    apply_prepared_delivery(&order(), &command, &mut values, &plan).unwrap();
    assert_eq!(values[0].stock_value_minor, 2_000);
    assert_eq!(values[1].stock_value_minor, 500);

    let mut substituted = prepared();
    substituted.lines[1].allocations[0].lot_id = "lot-2".into();
    let mut fresh = sales_delivery_values(&order(), &command).unwrap();
    assert_eq!(
        code(apply_prepared_delivery(&order(), &command, &mut fresh, &substituted).unwrap_err()),
        "COMMERCE_ORDER_CHANGED"
    );

    let mut value_changed = prepared();
    value_changed.total_value_minor = 2_499;
    assert_eq!(
        code(apply_prepared_delivery(&order(), &command, &mut fresh, &value_changed).unwrap_err()),
        "COMMERCE_ORDER_CHANGED"
    );
}

#[test]
fn mixed_fulfillment_keeps_stock_lines_first_and_services_at_zero_cost() {
    let mut command = command("delivery-unit-mixed", SalesDeliveryMode::Mixed);
    command.lines.push(SalesDeliveryLineCommand {
        line_id: "line-svc".into(),
        quantity_milli: 1_000,
        allocations: vec![],
    });
    let mut values = sales_delivery_values(&order(), &command).unwrap();
    apply_prepared_delivery(&order(), &command, &mut values, &prepared()).unwrap();
    let fulfillment = build_sales_delivery_fulfillment(
        &actor(),
        &command,
        &order(),
        &values,
        "ab".repeat(32).as_str(),
        TimestampMillis::new(NOW),
    )
    .unwrap();
    assert_eq!(fulfillment.id, "delivery-delivery-unit-mixed");
    assert_eq!(fulfillment.completion_kind, "mixed");
    assert_eq!(
        fulfillment.completion_note,
        "Delivered to the customer dock"
    );
    assert_eq!(
        fulfillment
            .lines
            .iter()
            .map(|line| (line.index, line.order_line_id.as_str(), line.value_minor))
            .collect::<Vec<_>>(),
        vec![
            (0, "line-stock", 2_000),
            (1, "line-lot", 500),
            (2, "line-svc", 0)
        ]
    );
    let receipt = CommerceReceipt {
        order_id: "sales-1".into(),
        fulfillment_id: fulfillment.id.clone(),
        state: "fulfilled".into(),
        version: 4,
        total_value_minor: 2_500,
        lines: values
            .iter()
            .map(|value| CommerceReceiptLine {
                line_id: value.line_id.clone(),
                quantity_milli: value.quantity_milli,
                value_minor: value.stock_value_minor,
            })
            .collect(),
    };
    let wire = sales_delivery_wire(&receipt);
    assert_eq!(wire["id"], "sales-1");
    assert_eq!(wire["fulfillmentId"], "delivery-delivery-unit-mixed");
    assert_eq!(wire["state"], "fulfilled");
    assert_eq!(wire["version"], 4);
    assert_eq!(wire["totalValueMinor"], 2_500);
}

#[test]
fn delivery_valuation_records_document_subtotal_and_functional_cost() {
    let mut command = command("delivery-unit-valuation", SalesDeliveryMode::Mixed);
    command.lines.push(SalesDeliveryLineCommand {
        line_id: "line-svc".into(),
        quantity_milli: 1_000,
        allocations: vec![],
    });
    let mut values = sales_delivery_values(&order(), &command).unwrap();
    apply_prepared_delivery(&order(), &command, &mut values, &prepared()).unwrap();
    let fulfillment = build_sales_delivery_fulfillment(
        &actor(),
        &command,
        &order(),
        &values,
        "cd".repeat(32).as_str(),
        TimestampMillis::new(NOW),
    )
    .unwrap();
    let valuation = GoodsValuation {
        stock_currency: "THB".into(),
        fx_rate_micros: 1_000_000,
        source_rate_id: None,
        target_rate_id: None,
        used_latest_fallback: false,
    };
    let plan = sales_delivery_valuation_plan(
        &fulfillment,
        "company",
        "chart-1",
        "journal-delivery-unit-valuation",
        "THB",
        "THB",
        "THB",
        &valuation,
        "seller",
        NOW,
    )
    .unwrap();
    assert_eq!(plan.document_subtotal_minor, 4_500);
    assert_eq!(plan.stock_value_minor, 2_500);
    assert_eq!(plan.valuation_date, "2026-11-05");
}

fn readback(fulfillment: &SalesDeliveryFulfillmentPlan) -> ReceiptReadback {
    ReceiptReadback {
        state: "fulfilled".into(),
        version: 4,
        lines: vec![
            ReceiptReadbackLine {
                id: "line-stock".into(),
                fulfilled_milli: 2_000,
            },
            ReceiptReadbackLine {
                id: "line-lot".into(),
                fulfilled_milli: 1_000,
            },
            ReceiptReadbackLine {
                id: "line-svc".into(),
                fulfilled_milli: 1_000,
            },
        ],
        fulfillment: Some(ReceiptFulfillmentReadback {
            id: fulfillment.id.clone(),
            order_id: "sales-1".into(),
            fulfillment_date: "2026-11-05".into(),
            lines: fulfillment
                .lines
                .iter()
                .map(|line| ReceiptFulfillmentLineReadback {
                    id: format!("{}-{}", fulfillment.id, line.index),
                    order_line_id: line.order_line_id.clone(),
                    quantity_milli: line.quantity_milli,
                    value_minor: line.value_minor,
                    subtotal_minor: line.subtotal_minor,
                    tax_minor: line.tax_minor,
                    stock_currency: Some("THB".into()),
                })
                .collect(),
        }),
        stock_moves: vec![
            ReceiptStockMoveReadback {
                id: format!("{}-0", fulfillment.id),
                warehouse_id: "warehouse-1".into(),
                product_id: "product-stock".into(),
                quantity_milli: -2_000,
                value_minor: -2_000,
            },
            ReceiptStockMoveReadback {
                id: format!("{}-1", fulfillment.id),
                warehouse_id: "warehouse-1".into(),
                product_id: "product-lot".into(),
                quantity_milli: -1_000,
                value_minor: -500,
            },
        ],
        stock: vec![
            ReceiptStockReadback {
                product_id: "product-stock".into(),
                quantity_milli: 3_000,
                value_minor: 1_000,
                version: 6,
            },
            ReceiptStockReadback {
                product_id: "product-lot".into(),
                quantity_milli: 1_000,
                value_minor: 500,
                version: 5,
            },
        ],
    }
}

fn applied(fulfillment: &SalesDeliveryFulfillmentPlan) -> SalesDeliveryStockApplied {
    SalesDeliveryStockApplied {
        fulfillment_id: fulfillment.id.clone(),
        replayed: false,
        lines: vec![
            SalesDeliveryStockAppliedLine {
                line_id: "line-stock".into(),
                stock_version: 6,
                quantity_milli: 2_000,
                value_minor: 2_000,
                allocations: vec![],
            },
            SalesDeliveryStockAppliedLine {
                line_id: "line-lot".into(),
                stock_version: 5,
                quantity_milli: 1_000,
                value_minor: 500,
                allocations: vec![SalesDeliveryStockAppliedAllocation {
                    lot_id: "lot-1".into(),
                    quantity_milli: 1_000,
                    value_minor: 500,
                    balance_version: 4,
                }],
            },
        ],
    }
}

#[test]
fn mixed_readback_accepts_service_lines_and_rejects_tampering() {
    let mut command = command("delivery-unit-readback", SalesDeliveryMode::Mixed);
    command.lines.push(SalesDeliveryLineCommand {
        line_id: "line-svc".into(),
        quantity_milli: 1_000,
        allocations: vec![],
    });
    let mut values = sales_delivery_values(&order(), &command).unwrap();
    apply_prepared_delivery(&order(), &command, &mut values, &prepared()).unwrap();
    let fulfillment = build_sales_delivery_fulfillment(
        &actor(),
        &command,
        &order(),
        &values,
        "ef".repeat(32).as_str(),
        TimestampMillis::new(NOW),
    )
    .unwrap();
    let receipt = CommerceReceipt {
        order_id: "sales-1".into(),
        fulfillment_id: fulfillment.id.clone(),
        state: "fulfilled".into(),
        version: 4,
        total_value_minor: 2_500,
        lines: values
            .iter()
            .map(|value: &SalesDeliveryLineValue| CommerceReceiptLine {
                line_id: value.line_id.clone(),
                quantity_milli: value.quantity_milli,
                value_minor: value.stock_value_minor,
            })
            .collect(),
    };
    let evidence = applied(&fulfillment);
    validate_sales_delivery_readback(
        &receipt,
        &order(),
        &fulfillment,
        &evidence,
        &readback(&fulfillment),
    )
    .unwrap();

    let mut tampered = readback(&fulfillment);
    tampered.stock_moves[0].value_minor = -1;
    assert_eq!(
        code(
            validate_sales_delivery_readback(
                &receipt,
                &order(),
                &fulfillment,
                &evidence,
                &tampered
            )
            .unwrap_err()
        ),
        "COMMERCE_DELIVERY_READBACK_INVALID"
    );

    let mut replay = applied(&fulfillment);
    replay.replayed = true;
    assert_eq!(
        code(
            validate_sales_delivery_readback(
                &receipt,
                &order(),
                &fulfillment,
                &replay,
                &readback(&fulfillment)
            )
            .unwrap_err()
        ),
        "COMMERCE_DELIVERY_READBACK_INVALID"
    );
}

mod http_ingress {
    use erpx_server_commerce::public::order_http::{
        CommerceOrderHttpBackend, CommerceOrderHttpCommand, CommerceOrderHttpFuture,
        CommerceOrderHttpRequest, CommerceOrderHttpResponse, CommerceOrderHttpState,
        CommerceOrderHttpTarget, CommerceOriginPolicy, CommerceSalesDeliveryHttpBackend,
        CommerceSalesDeliveryHttpRequest, CommerceSalesDeliveryHttpTarget,
    };
    use erpx_server_runtime::public::http_support::{
        HeaderMap, HeaderValue, StatusCode, body::to_bytes, serde_json,
    };
    use std::sync::{Arc, Mutex};

    struct OrderProbe {
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl CommerceOrderHttpBackend for OrderProbe {
        fn execute<'a>(
            &'a self,
            request: CommerceOrderHttpRequest<'a>,
        ) -> CommerceOrderHttpFuture<'a, CommerceOrderHttpResponse> {
            Box::pin(async move {
                self.seen.lock().unwrap().push(match request.command {
                    CommerceOrderHttpCommand::Fulfill { input, .. } => {
                        format!("service|{}", input.lines.len())
                    }
                    _ => "other".into(),
                });
                Ok(CommerceOrderHttpResponse {
                    data: serde_json::json!({
                        "id": "sales-1",
                        "fulfillmentId": "delivery-service",
                        "version": 4,
                        "state": "fulfilled",
                    }),
                    context: serde_json::json!({"userId": "seller"}),
                })
            })
        }
    }

    struct DeliveryProbe {
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl CommerceSalesDeliveryHttpBackend for DeliveryProbe {
        fn deliver<'a>(
            &'a self,
            request: CommerceSalesDeliveryHttpRequest<'a>,
        ) -> CommerceOrderHttpFuture<'a, CommerceOrderHttpResponse> {
            Box::pin(async move {
                let command = &request.command;
                self.seen.lock().unwrap().push(format!(
                    "delivery|{}|{}|{}|{}|{}|{}|{}",
                    request.host,
                    request.holding_slug,
                    request.order_id,
                    request.idempotency_key,
                    command.mode.as_str(),
                    command.expected_version,
                    command
                        .lines
                        .iter()
                        .map(|line| format!(
                            "{}:{}:{}",
                            line.line_id,
                            line.quantity_milli,
                            line.allocations
                                .iter()
                                .map(|allocation| allocation.lot_id.clone().unwrap_or_default())
                                .collect::<Vec<_>>()
                                .join(",")
                        ))
                        .collect::<Vec<_>>()
                        .join("+"),
                ));
                Ok(CommerceOrderHttpResponse {
                    data: serde_json::json!({
                        "id": "sales-1",
                        "fulfillmentId": "delivery-sales-1",
                        "version": 4,
                        "state": "partially_fulfilled",
                        "totalValueMinor": 600,
                        "lines": [{"lineId": "line-stock", "quantityMilli": 2000, "valueMinor": 400}],
                    }),
                    context: serde_json::json!({"userId": "seller"}),
                })
            })
        }
    }

    fn headers(body: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("acme.example.test"));
        headers.insert(
            "origin",
            HeaderValue::from_static("https://acme.example.test"),
        );
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        headers.insert("idempotency-key", HeaderValue::from_static("delivery-key"));
        assert!(!body.is_empty());
        headers
    }

    fn state(
        orders: Arc<Mutex<Vec<String>>>,
        delivery: Option<Arc<Mutex<Vec<String>>>>,
    ) -> CommerceOrderHttpState {
        let state = CommerceOrderHttpState::new_with_origin_policy(
            vec![CommerceOrderHttpTarget {
                host: "acme.example.test".into(),
                holding_slug: "acme".into(),
                origin: "https://acme.example.test".into(),
                backend: Arc::new(OrderProbe { seen: orders }),
            }],
            CommerceOriginPolicy::Production,
        )
        .unwrap();
        match delivery {
            None => state,
            Some(seen) => state
                .with_sales_delivery(vec![CommerceSalesDeliveryHttpTarget {
                    host: "acme.example.test".into(),
                    holding_slug: "acme".into(),
                    origin: "https://acme.example.test".into(),
                    backend: Arc::new(DeliveryProbe { seen }),
                }])
                .unwrap(),
        }
    }

    async fn body(
        response: erpx_server_runtime::public::http_support::Response,
    ) -> (StatusCode, serde_json::Value) {
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn goods_body_is_decoded_and_dispatched_to_the_delivery_backend() {
        let orders = Arc::new(Mutex::new(Vec::new()));
        let delivery = Arc::new(Mutex::new(Vec::new()));
        let state = state(orders.clone(), Some(delivery.clone()));
        let (status, value) = body(
            state
                .fulfill(
                    &headers("goods"),
                    "acme",
                    "sales-1",
                    None,
                    br#"{"completionMode":"goods","expectedVersion":3,"date":"2026-11-05","reason":"Dock 2","lines":[{"lineId":"line-stock","quantity":"2","allocations":[]},{"lineId":"line-lot","quantity":"1","allocations":[{"lotId":"lot-1","lotCode":"L1","quantity":"1","disposition":"available"}]}]}"#,
                )
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["data"]["fulfillmentId"], "delivery-sales-1");
        assert_eq!(value["data"]["totalValueMinor"], 600);
        assert_eq!(
            delivery.lock().unwrap().as_slice(),
            [
                "delivery|acme.example.test|acme|sales-1|delivery-key|goods|3|line-stock:2000:+line-lot:1000:lot-1"
            ]
        );
        assert!(orders.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn service_body_stays_on_the_service_completion_path() {
        let orders = Arc::new(Mutex::new(Vec::new()));
        let delivery = Arc::new(Mutex::new(Vec::new()));
        let state = state(orders.clone(), Some(delivery.clone()));
        let (status, value) = body(
            state
                .fulfill(
                    &headers("service"),
                    "acme",
                    "sales-1",
                    None,
                    br#"{"completionMode":"service","expectedVersion":3,"date":"2026-11-05","evidenceNote":"Signed off","lines":[{"lineId":"line-svc","quantity":"1"}]}"#,
                )
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["data"]["fulfillmentId"], "delivery-service");
        assert_eq!(orders.lock().unwrap().as_slice(), ["service|1"]);
        assert!(delivery.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn goods_body_without_a_delivery_backend_fails_closed() {
        let orders = Arc::new(Mutex::new(Vec::new()));
        let state = state(orders.clone(), None);
        let (status, value) = body(
            state
                .fulfill(
                    &headers("goods"),
                    "acme",
                    "sales-1",
                    None,
                    br#"{"completionMode":"goods","expectedVersion":3,"date":"2026-11-05","lines":[{"lineId":"line-stock","quantity":"2"}]}"#,
                )
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(value["error"]["code"], "SALES_DELIVERY_NOT_ENABLED");
        assert!(orders.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn goods_body_rejects_quarantine_evidence_before_dispatch() {
        let orders = Arc::new(Mutex::new(Vec::new()));
        let delivery = Arc::new(Mutex::new(Vec::new()));
        let state = state(orders.clone(), Some(delivery.clone()));
        let (status, value) = body(
            state
                .fulfill(
                    &headers("goods"),
                    "acme",
                    "sales-1",
                    None,
                    br#"{"completionMode":"goods","expectedVersion":3,"date":"2026-11-05","lines":[{"lineId":"line-lot","quantity":"1","allocations":[{"lotId":"lot-1","quantity":"1","disposition":"quarantine","quarantineReason":"Damaged","quarantineDueDate":"2026-11-20"}]}]}"#,
                )
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(value["error"]["code"], "QUARANTINE_DELIVERY_FORBIDDEN");
        assert!(delivery.lock().unwrap().is_empty());
    }
}
