//! Lead-owned mounts for the native migration lanes.
//!
//! Owners implement their lane routers in isolated modules; this module maps
//! every registered Holding to its own database pool and merges the routers
//! into the private API. No owner business logic lives here.

use crate::private_development_api::DevelopmentApiConfiguration;
use axum::Router;
use erpx_server_runtime::public::postgres::DatabasePool;

/// Merge every ready native lane router into the private API router.
///
/// `pools[0]` is the control plane; `pools[i + 1]` is the pool for
/// `config.holdings[i]`. Each lane resolves a request by its exact
/// `(Host, holding_slug)` target, so one Holding can never read another
/// Holding's database.
pub fn mount(
    mut router: Router,
    config: &DevelopmentApiConfiguration,
    pools: &[DatabasePool],
) -> Result<Router, &'static str> {
    if pools.len() != config.holdings.len() + 1 {
        return Err("PRIVATE_API_NATIVE_LANE_POOL_MISMATCH");
    }

    // People & Payroll lane 6: calendar availability leave days.
    router = router.merge(
        erpx_server_people_payroll::public::native_lane_6::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_people_payroll::public::native_lane_6::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_PEOPLE_PAYROLL_LANE6_INVALID")?,
    );

    // People & Payroll lane 9: person-image evidence.
    router = router.merge(
        erpx_server_people_payroll::public::native_lane_9::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_people_payroll::public::native_lane_9::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_PEOPLE_PAYROLL_LANE9_INVALID")?,
    );

    // Identity & Access lane 11: IAM person identity reads.
    router = router.merge(
        erpx_server_identity_access::public::native_lane_11::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_identity_access::public::native_lane_11::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_IDENTITY_LANE11_INVALID")?,
    );

    // Identity & Access lane 12: people template attributes.
    router = router.merge(
        erpx_server_identity_access::public::native_lane_12::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_identity_access::public::native_lane_12::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_IDENTITY_LANE12_INVALID")?,
    );

    // Identity: private-file denial authority witness.
    router = router.merge(
        erpx_server_identity_access::public::private_file_denial_http::private_file_denial_http_router(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    erpx_server_identity_access::public::private_file_denial_http::PrivateFileDenialHttpTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                        origin: format!("https://{}", holding.host),
                        backend: std::sync::Arc::new(
                            crate::private_file_denial::PrivateFileDenialBackend::new(
                                pools[i + 1].clone(),
                            ),
                        ),
                    }
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_IDENTITY_PRIVATE_FILE_DENIAL_INVALID")?,
    );

    // Identity: Control read-plan port (caller executes on its own Control unit).
    router = router.merge(
        erpx_server_identity_access::public::control_read_plan::router_for_holdings(
            config
                .holdings
                .iter()
                .map(|holding| {
                    erpx_server_identity_access::public::control_read_plan::ControlReadTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                        origin: format!("https://{}", holding.host),
                    }
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_IDENTITY_CONTROL_READ_PLAN_INVALID")?,
    );

    // Commerce lane 13: entrypoint identity facts.
    router = router.merge(
        erpx_server_commerce::public::native_lane_13::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_commerce::public::native_lane_13::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_COMMERCE_LANE13_INVALID")?,
    );

    // Commerce lane 14: workflow order reads.
    router = router.merge(
        erpx_server_commerce::public::native_lane_14::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_commerce::public::native_lane_14::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_COMMERCE_LANE14_INVALID")?,
    );

    // PMS lane 19: guest-account finance evidence.
    router = router.merge(
        erpx_server_pms::public::native_lane_19::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_pms::public::native_lane_19::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_PMS_LANE19_INVALID")?,
    );

    // PMS lane 20: property readiness facts.
    router = router.merge(
        erpx_server_pms::public::native_lane_20::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_pms::public::native_lane_20::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_PMS_LANE20_INVALID")?,
    );

    // Manufacturing production accounting-profile read.
    router = router.merge(
        erpx_server_manufacturing::public::production_profiles_http::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_manufacturing::public::production_profiles_http::ProductionProfilesTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_MANUFACTURING_PRODUCTION_PROFILES_INVALID")?,
    );

    // Platform executive generation-job usage read (control pool).
    router = router.merge(
        erpx_server_platform_administration::public::native_platform_usage::platform_usage_router_for_hosts(
            config.product_hosts(),
            erpx_server_platform_administration::public::native_platform_usage::shared_reader(
                pools[0].clone(),
            ),
        )
        .map_err(|_| "PRIVATE_API_PLATFORM_USAGE_INVALID")?,
    );

    // PMS home hotel-metrics read.
    router = router.merge(
        erpx_server_pms::public::home_metrics::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_pms::public::home_metrics::HomeMetricsTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_PMS_HOME_METRICS_INVALID")?,
    );

    // Files lane 22: work evidence reads.
    router = router.merge(
        erpx_server_files::public::native_lane_22::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_files::public::native_lane_22::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_FILES_LANE22_INVALID")?,
    );

    // Organization lane 23: directory hierarchy facts.
    router = router.merge(
        erpx_server_organization::public::native_lane_23::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_organization::public::native_lane_23::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_ORGANIZATION_LANE23_INVALID")?,
    );

    // POS lane 25: executive transaction evidence.
    router = router.merge(
        erpx_server_pos::public::native_lane_25::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_pos::public::native_lane_25::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_POS_LANE25_INVALID")?,
    );

    // Legal lane 26: executed-copy reads.
    router = router.merge(
        erpx_server_legal::public::native_lane_26::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_legal::public::native_lane_26::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_LEGAL_LANE26_INVALID")?,
    );

    // Projects lane 27: actuals list.
    router = router.merge(
        erpx_server_projects::public::native_lane_27::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_projects::public::native_lane_27::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_PROJECTS_LANE27_INVALID")?,
    );

    // Sales lane 30: customer note projections.
    router = router.merge(
        erpx_server_sales::public::native_lane_30::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_sales::public::native_lane_30::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_SALES_LANE30_INVALID")?,
    );

    // Learning lane 31: evidence detail.
    router = router.merge(
        erpx_server_learning::public::native_lane_31::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_learning::public::native_lane_31::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_LEARNING_LANE31_INVALID")?,
    );

    // Benefits lane 32: released payroll inputs.
    router = router.merge(
        erpx_server_benefits::public::native_lane_32::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_benefits::public::native_lane_32::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                            origin: format!("https://{}", holding.host),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_BENEFITS_LANE32_INVALID")?,
    );

    // Customer 360 native read-plan route.
    let c360_read_targets: Vec<_> = config
        .holdings
        .iter()
        .filter_map(|holding| {
            config
                .holding_company(&holding.registration.holding_id)
                .map(|company_id| {
                    erpx_server_customer_360::public::native_360_read_http::Customer360ReadPlanTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                        origin: format!("https://{}", holding.host),
                        company_id: company_id.as_str().to_owned(),
                    }
                })
        })
        .collect();
    if !c360_read_targets.is_empty() {
        router = router.merge(
            erpx_server_customer_360::public::native_360_read_http::router_for_holdings(
                c360_read_targets,
            )
            .map_err(|_| "PRIVATE_API_CUSTOMER_360_READ_PLAN_INVALID")?,
        );
    }

    // Benefits: native settlement event history.
    router = router.merge(
        erpx_server_benefits::public::native_settlement_events::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_benefits::public::native_settlement_events::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                            origin: format!("https://{}", holding.host),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_BENEFITS_SETTLEMENT_EVENTS_INVALID")?,
    );

    // Runtime lane 33: holding-binding reads.
    router = router.merge(
        erpx_server_runtime::public::native_lane_33::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_runtime::public::native_lane_33::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_RUNTIME_LANE33_INVALID")?,
    );

    // Manage ERPX lane 34: beta feedback.
    router = router.merge(
        erpx_server_manage_erpx::public::native_lane_34::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_manage_erpx::public::native_lane_34::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_MANAGE_ERPX_LANE34_INVALID")?,
    );

    // Records lane 35: work-report reads.
    router = router.merge(
        erpx_server_records::public::native_lane_35::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_records::public::native_lane_35::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_RECORDS_LANE35_INVALID")?,
    );

    // Planning lane 36: scoped workspace sources.
    router = router.merge(
        erpx_server_planning::public::native_lane_36::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_planning::public::native_lane_36::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_PLANNING_LANE36_INVALID")?,
    );

    // Maintenance lane 36: attachment target descriptor.
    router = router.merge(
        erpx_server_maintenance::public::native_lane_36::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_maintenance::public::native_lane_36::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_MAINTENANCE_LANE36_INVALID")?,
    );

    // Commercial lane 37: holding seat history.
    router = router.merge(
        erpx_server_commercial::public::native_lane_37::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_commercial::public::native_lane_37::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                            holding_id: holding.registration.holding_id.clone().into(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_COMMERCIAL_LANE37_INVALID")?,
    );

    // Finance lane 2: invoice aging reads (public aging branch).
    router = router.merge(
        erpx_server_finance::public::native_lane_2::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| {
                (
                    erpx_server_finance::public::native_lane_2::LaneTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                    },
                    pools[i + 1].clone(),
                )
            }).collect(),
        ).map_err(|_| "PRIVATE_API_FINANCE_LANE2_INVALID")?,
    );

    // Finance lane 3: invoice-line reads.
    router = router.merge(
        erpx_server_finance::public::native_lane_3::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| {
                (
                    erpx_server_finance::public::native_lane_3::LaneTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                    },
                    pools[i + 1].clone(),
                )
            }).collect(),
        ).map_err(|_| "PRIVATE_API_FINANCE_LANE3_INVALID")?,
    );

    // Finance lane 4: reference-option reads.
    router = router.merge(
        erpx_server_finance::public::native_lane_4::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| {
                (
                    erpx_server_finance::public::native_lane_4::LaneTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                    },
                    pools[i + 1].clone(),
                )
            }).collect(),
        ).map_err(|_| "PRIVATE_API_FINANCE_LANE4_INVALID")?,
    );

    // Finance lane 5: customer master reads.
    router = router.merge(
        erpx_server_finance::public::native_lane_5::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| {
                (
                    erpx_server_finance::public::native_lane_5::LaneTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                    },
                    pools[i + 1].clone(),
                )
            }).collect(),
        ).map_err(|_| "PRIVATE_API_FINANCE_LANE5_INVALID")?,
    );

    // Finance lane 5B: record-details owned facts read.
    router = router.merge(
        erpx_server_finance::public::native_lane_5::record_facts_router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| {
                (
                    erpx_server_finance::public::native_lane_5::LaneTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                    },
                    pools[i + 1].clone(),
                )
            }).collect(),
        ).map_err(|_| "PRIVATE_API_FINANCE_LANE5_RECORD_FACTS_INVALID")?,
    );

    // Finance lane 1: posting document preview reads. Requires the exact
    // registered Company per Holding; skip Holdings without a configured Company.
    let lane1_targets: Vec<_> = config
        .holdings
        .iter()
        .enumerate()
        .filter_map(|(i, holding)| {
            config
                .holding_company(&holding.registration.holding_id)
                .map(|company_id| {
                    (
                        erpx_server_finance::public::native_lane_1::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                            company_id,
                        },
                        pools[i + 1].clone(),
                    )
                })
        })
        .collect();
    if !lane1_targets.is_empty() {
        router = router.merge(
            erpx_server_finance::public::native_lane_1::doc_preview_router_for_holdings(lane1_targets)
                .map_err(|_| "PRIVATE_API_FINANCE_LANE1_DOC_PREVIEW_INVALID")?,
        );
    }

    // People & Payroll lane 8: app-entrypoint reads.
    router = router.merge(
        erpx_server_people_payroll::public::native_lane_8::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| {
                (
                    erpx_server_people_payroll::public::native_lane_8::LaneTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                    },
                    pools[i + 1].clone(),
                )
            }).collect(),
        ).map_err(|_| "PRIVATE_API_PEOPLE_PAYROLL_LANE8_INVALID")?,
    );

    // Finance lane 2: overview company counts.
    router = router.merge(
        erpx_server_finance::public::native_lane_2::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| {
                (
                    erpx_server_finance::public::native_lane_2::LaneTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                    },
                    pools[i + 1].clone(),
                )
            }).collect(),
        ).map_err(|_| "PRIVATE_API_FINANCE_LANE2_INVALID")?,
    );

    // People & Payroll lane 7: legacy people-access clock.
    router = router.merge(
        erpx_server_people_payroll::public::native_lane_7::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| {
                (
                    erpx_server_people_payroll::public::native_lane_7::LaneTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                    },
                    pools[i + 1].clone(),
                )
            }).collect(),
        ).map_err(|_| "PRIVATE_API_PEOPLE_PAYROLL_LANE7_INVALID")?,
    );

    // Identity & Access lane 10: account invitation reads (Control database).
    router = router.merge(
        erpx_server_identity_access::public::native_lane_10::router_for_holdings(
            config.holdings.iter().map(|holding| {
                (
                    erpx_server_identity_access::public::native_lane_10::LaneTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                    },
                    pools[0].clone(),
                )
            }).collect(),
        ).map_err(|_| "PRIVATE_API_IDENTITY_LANE10_INVALID")?,
    );

    // Collaboration lane 15: beta management facts.
    router = router.merge(
        erpx_server_collaboration::public::native_lane_15::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| {
                (
                    erpx_server_collaboration::public::native_lane_15::LaneTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                    },
                    pools[i + 1].clone(),
                )
            }).collect(),
        ).map_err(|_| "PRIVATE_API_COLLABORATION_LANE15_INVALID")?,
    );

    // Inventory lane 17: landed-cost reads.
    router = router.merge(
        erpx_server_inventory::public::native_lane_17::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| {
                (
                    erpx_server_inventory::public::native_lane_17::LaneTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                    },
                    pools[i + 1].clone(),
                )
            }).collect(),
        ).map_err(|_| "PRIVATE_API_INVENTORY_LANE17_INVALID")?,
    );

    // Inventory lane 18: POS stock-availability facts.
    router = router.merge(
        erpx_server_inventory::public::native_lane_18::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| {
                (
                    erpx_server_inventory::public::native_lane_18::LaneTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                    },
                    pools[i + 1].clone(),
                )
            }).collect(),
        ).map_err(|_| "PRIVATE_API_INVENTORY_LANE18_INVALID")?,
    );

    // Operations lane 28: staff-level acceptance facts.
    router = router.merge(
        erpx_server_operations::public::native_lane_28::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| {
                (
                    erpx_server_operations::public::native_lane_28::Lane28Target {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                    },
                    pools[i + 1].clone(),
                )
            }).collect(),
        ).map_err(|_| "PRIVATE_API_OPERATIONS_LANE28_INVALID")?,
    );

    // Marketing lane 40: workspace candidate and attribution reads.
    router = router.merge(
        erpx_server_marketing::public::native_lane_40::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| {
                (
                    erpx_server_marketing::public::native_lane_40::LaneTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                    },
                    pools[i + 1].clone(),
                )
            }).collect(),
        ).map_err(|_| "PRIVATE_API_MARKETING_LANE40_INVALID")?,
    );

    // Billing lane 38: executive snapshot watermarks.
    router = router.merge(
        erpx_server_billing::public::native_lane_38::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| {
                (
                    erpx_server_billing::public::native_lane_38::LaneTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                    },
                    pools[i + 1].clone(),
                )
            }).collect(),
        ).map_err(|_| "PRIVATE_API_BILLING_LANE38_INVALID")?,
    );

    // Brand lane 39: workspace-choice publication reads.
    router = router.merge(
        erpx_server_brand::public::native_lane_39::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| {
                (
                    erpx_server_brand::public::native_lane_39::LaneTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                    },
                    pools[i + 1].clone(),
                )
            }).collect(),
        ).map_err(|_| "PRIVATE_API_BRAND_LANE39_INVALID")?,
    );

    // Brand lane 39: executive evidence search.
    router = router.merge(
        erpx_server_brand::public::native_lane_39::evidence_router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| {
                (
                    erpx_server_brand::public::native_lane_39::EvidenceTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                    },
                    pools[i + 1].clone(),
                )
            }).collect(),
        ).map_err(|_| "PRIVATE_API_BRAND_LANE39_EVIDENCE_INVALID")?,
    );

    // Ecommerce lane 39: workspace list reads.
    router = router.merge(
        erpx_server_ecommerce::public::native_lane_39::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| {
                (
                    erpx_server_ecommerce::public::native_lane_39::LaneTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                    },
                    pools[i + 1].clone(),
                )
            }).collect(),
        ).map_err(|_| "PRIVATE_API_ECOMMERCE_LANE39_INVALID")?,
    );

    // Organization lane 24: structural and evaluation reference reads.
    router = router.merge(
        erpx_server_organization::public::native_lane_24::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| {
                (
                    erpx_server_organization::public::native_lane_24::LaneTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                    },
                    pools[i + 1].clone(),
                )
            }).collect(),
        ).map_err(|_| "PRIVATE_API_ORGANIZATION_LANE24_INVALID")?,
    );

    // Collaboration lane 16: sent-notice reads. The owner requires the exact
    // registered Company per Holding; skip Holdings without a configured Company.
    let lane16_targets: Vec<_> = config
        .holdings
        .iter()
        .enumerate()
        .filter_map(|(i, holding)| {
            config
                .holding_company(&holding.registration.holding_id)
                .map(|company_id| {
                    (
                        erpx_server_collaboration::public::native_lane_16::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                            company_id,
                        },
                        pools[i + 1].clone(),
                    )
                })
        })
        .collect();
    if !lane16_targets.is_empty() {
        router = router.merge(
            erpx_server_collaboration::public::native_lane_16::router_for_holdings(lane16_targets)
                .map_err(|_| "PRIVATE_API_COLLABORATION_LANE16_INVALID")?,
        );
    }

    // Work projection lane 29: calendar owner reads.
    router = router.merge(
        erpx_server_work_projection::public::native_lane_29::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| {
                (
                    erpx_server_work_projection::public::native_lane_29::LaneTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                    },
                    pools[i + 1].clone(),
                )
            }).collect(),
        ).map_err(|_| "PRIVATE_API_WORK_PROJECTION_LANE29_INVALID")?,
    );

    // Files lane 21: commercial storage-reconciliation usage facts.
    router = router.merge(
        erpx_server_files::public::native_lane_21::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| {
                (
                    erpx_server_files::public::native_lane_21::LaneTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                    },
                    pools[i + 1].clone(),
                )
            }).collect(),
        ).map_err(|_| "PRIVATE_API_FILES_LANE21_INVALID")?,
    );

    // Records lane 35 wave 2: owner-scope attachment read.
    router = router.merge(
        erpx_server_records::public::native_lane_35::owner_scope_router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| {
                (
                    erpx_server_records::public::native_lane_35::OwnerScopeTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                    },
                    pools[i + 1].clone(),
                )
            }).collect(),
        ).map_err(|_| "PRIVATE_API_RECORDS_LANE35_OWNER_SCOPE_INVALID")?,
    );

    Ok(router)
}
