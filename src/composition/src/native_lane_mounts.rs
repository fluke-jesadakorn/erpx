//! Lead-owned mounts for the native migration lanes.
//!
//! Owners implement their lane routers in isolated modules; this module maps
//! every registered Holding to its own database pool and merges the routers
//! into the private API. No owner business logic lives here.

use crate::private_development_api::DevelopmentApiConfiguration;
use axum::Router;
use erpx_server_records::public::record_search::{
    PrincipalHit, RecordHit, SearchActor, SearchAdmission, SearchFuture,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Composition-owned admission for the native Records reference search.
///
/// Candidate SQL scopes records by Company, branch or department and module.
/// It excludes Field-owned sources before the limit. Principal names come from
/// the Control database, with active-assignment eligibility read in the Holding.
struct RecordsSearchAdmission {
    control: DatabasePool,
    holding: DatabasePool,
}

impl SearchAdmission for RecordsSearchAdmission {
    fn visible<'a>(
        &'a self,
        actor: &'a SearchActor,
        hit: &'a RecordHit,
    ) -> SearchFuture<'a, bool> {
        Box::pin(async move {
            Ok(actor
                .modules
                .iter()
                .any(|module| module == &hit.module_id))
        })
    }

    /// Control `principals` supplies the display name; the target Holding
    /// supplies the retained active-assignment predicate. A principal without
    /// an active assignment in this Holding is omitted, never synthesized.
    fn principal_hits<'a>(
        &'a self,
        _actor: &'a SearchActor,
        needle: &'a str,
        now: i64,
    ) -> SearchFuture<'a, Vec<PrincipalHit>> {
        Box::pin(async move {
            let mut hits = Vec::new();
            for (target_id, label) in
                erpx_server_identity_access::public::records_principal_search::search_principals(
                    &self.control,
                    needle,
                )
                .await?
            {
                if erpx_server_identity_access::public::records_principal_search::active_assignment(
                    &self.holding,
                    &target_id,
                    now,
                )
                .await?
                {
                    hits.push(PrincipalHit { target_id, label });
                }
            }
            Ok(hits)
        })
    }
}

fn records_search_admission(control: DatabasePool, holding: DatabasePool) -> Arc<dyn SearchAdmission> {
    Arc::new(RecordsSearchAdmission { control, holding })
}

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

    // Account profile read (Control pool).
    router = router.merge(
        erpx_server_identity_access::public::account_profile_read::account_profile_read_router(
            config
                .holdings
                .iter()
                .map(|holding| {
                    erpx_server_identity_access::public::account_profile_read::AccountProfileReadTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                        reader: std::sync::Arc::new(
                            erpx_server_identity_access::public::account_profile_read::PgAccountProfileReader::new(
                                pools[0].clone(),
                            ),
                        ),
                    }
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_ACCOUNT_PROFILE_INVALID")?,
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

    // Runtime lane 33: holding-binding reads (Control database).
    router = router.merge(
        erpx_server_runtime::public::native_lane_33::router_for_holdings(
            config
                .holdings
                .iter()
                .map(|holding| {
                    (
                        erpx_server_runtime::public::native_lane_33::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[0].clone(),
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
            erpx_server_finance::public::native_lane_1::router_for_holdings(lane1_targets.clone())
                .map_err(|_| "PRIVATE_API_FINANCE_LANE1_EXPENSE_PROJECTION_INVALID")?,
        );
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

    // Work Projection calendar reads: owner SELECTs on the caller Holding.
    router = router.merge(
        erpx_server_work_projection::public::calendar_read_http::calendar_read_router(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    erpx_server_work_projection::public::calendar_read_http::CalendarReadHttpTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                        origin: format!("https://{}", holding.host),
                        backend: Arc::new(
                            erpx_server_work_projection::public::calendar_read_http::PgCalendarReadBackend::new([(
                                holding.registration.slug.clone(),
                                pools[i + 1].clone(),
                            )]),
                        ),
                    }
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_WORK_CALENDAR_READ_INVALID")?,
    );

    // Workflow lane 32: inbox failed-metrics read.
    router = router.merge(
        erpx_server_workflow::public::native_lane_32::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_workflow::public::native_lane_32::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                            origin: format!("https://{}", holding.host),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_WORKFLOW_LANE32_INVALID")?,
    );

    // The generic Records search excludes the Field owner's source identities.
    let field_probe =
        erpx_server_field_service::public::record_probe::field_record_probe("sourceReadScopeProbe");
    router = router.merge(
        erpx_server_records::public::record_search_http::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| (
                erpx_server_records::public::record_search::SearchTarget {
                    host: holding.host.clone(),
                    holding_slug: holding.registration.slug.clone(),
                },
                pools[i + 1].clone(),
                records_search_admission(pools[0].clone(), pools[i + 1].clone()),
            )).collect(),
            field_probe,
        ).map_err(|_| "PRIVATE_API_RECORDS_SEARCH_INVALID")?,
    );

    // Marketing: active public form source read.
    router = router.merge(
        erpx_server_marketing::public::public_form_source::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_marketing::public::public_form_source::PublicFormTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_MARKETING_PUBLIC_FORM_INVALID")?,
    );

    // PMS: import batch list read.
    router = router.merge(
        erpx_server_pms::public::import_batches::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_pms::public::import_batches::ImportBatchTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_PMS_IMPORT_BATCHES_INVALID")?,
    );

    // People & Payroll: people-directory list read.
    router = router.merge(
        erpx_server_people_payroll::public::native_people_list::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_people_payroll::public::native_people_list::PeopleListTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_PEOPLE_LIST_INVALID")?,
    );

    // Talent: application detail read.
    router = router.merge(
        erpx_server_talent::public::native_talent_detail::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_talent::public::native_talent_detail::TalentDetailTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_TALENT_DETAIL_INVALID")?,
    );

    // Finance: owner read planner for the browser Finance route.
    router = router.merge(
        erpx_server_finance::public::finance_owner_write_http::command_plan_router(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    erpx_server_finance::public::finance_owner_write_http::FinanceOwnerWriteTarget {
                        host: holding.host.clone(),
                        holding_slug: holding.registration.slug.clone(),
                        origin: format!("https://{}", holding.host),
                        backend: Arc::new(
                            crate::finance_owner_write::FinanceOwnerWriteBackendImpl::new(
                                pools[i + 1].clone(),
                            ),
                        ),
                    }
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_FINANCE_COMMAND_PLAN_INVALID")?,
    );

    // Finance lane 6: chart-list read.
    router = router.merge(
        erpx_server_finance::public::native_lane_6::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_finance::public::native_lane_6::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_FINANCE_LANE6_INVALID")?,
    );

    // Legal: rule-impact read.
    router = router.merge(
        erpx_server_legal::public::native_rule_impact::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_legal::public::native_rule_impact::RuleImpactTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_LEGAL_RULE_IMPACT_INVALID")?,
    );

    // POS: room-charge reversal queue read.
    router = router.merge(
        erpx_server_pos::public::native_room_charges::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_pos::public::native_room_charges::RoomChargeTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_POS_ROOM_CHARGES_INVALID")?,
    );

    // Commerce: order return-line read.
    router = router.merge(
        erpx_server_commerce::public::native_return_lines::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_commerce::public::native_return_lines::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_COMMERCE_RETURN_LINES_INVALID")?,
    );

    // Inventory: stock-transfer list read.
    router = router.merge(
        erpx_server_inventory::public::native_transfer_list::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_inventory::public::native_transfer_list::TransferTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_INVENTORY_TRANSFER_LIST_INVALID")?,
    );

    // Maintenance: eligible-owner option read.
    router = router.merge(
        erpx_server_maintenance::public::native_owner_options::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_maintenance::public::native_owner_options::OwnerOptionTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_MAINTENANCE_OWNER_OPTIONS_INVALID")?,
    );

    // Agent: assistant source-reference read.
    router = router.merge(
        erpx_server_agent::public::native_assistant_sources::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_agent::public::native_assistant_sources::AssistantSourcesTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_AGENT_ASSISTANT_SOURCES_INVALID")?,
    );

    // Finance: attachment-version guard/step port keyed by Company.
    let mut attachment_versions = BTreeMap::new();
    for (i, holding) in config.holdings.iter().enumerate() {
        if let Some(company) = config.holding_company(&holding.registration.holding_id) {
            attachment_versions.entry(company).or_insert_with(|| {
                std::sync::Arc::new(
                    erpx_server_finance::public::attachment_version_http::PgAttachmentVersionBackend::new(
                        pools[i + 1].clone(),
                    ),
                ) as std::sync::Arc<dyn erpx_server_finance::public::attachment_version_http::AttachmentVersionBackend>
            });
        }
    }
    if !attachment_versions.is_empty() {
        router = router.merge(
            erpx_server_finance::public::attachment_version_http::attachment_version_router(
                attachment_versions.into_iter().collect(),
            )
            .map_err(|_| "PRIVATE_API_FINANCE_ATTACHMENT_VERSION_INVALID")?,
        );
    }

    // Operations: routine-template list read.
    router = router.merge(
        erpx_server_operations::public::native_routines_read::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_operations::public::native_routines_read::TemplateReadTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_OPERATIONS_ROUTINES_INVALID")?,
    );

    // Ecommerce: checkout detail-link read.
    router = router.merge(
        erpx_server_ecommerce::public::native_detail_links::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_ecommerce::public::native_detail_links::DetailLinkTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_ECOMMERCE_DETAIL_LINKS_INVALID")?,
    );

    // Brand: active Branch/Company management guards.
    router = router.merge(
        erpx_server_brand::public::native_lane_40::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_brand::public::native_lane_40::ManagementGuardTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_BRAND_LANE40_INVALID")?,
    );

    // Identity & Access lane 45: IAM session list read. The Account host reads
    // the same Control session table, so it selects the first Holding slug on
    // the Control pool while keeping an exact registered Host target.
    let mut lane45_targets: Vec<_> = config
        .holdings
        .iter()
        .map(|holding| {
            (
                erpx_server_identity_access::public::native_lane_45::LaneTarget {
                    host: holding.host.clone(),
                    holding_slug: holding.registration.slug.clone(),
                },
                pools[0].clone(),
            )
        })
        .collect();
    if let Some(first) = config.holdings.first() {
        lane45_targets.push((
            erpx_server_identity_access::public::native_lane_45::LaneTarget {
                host: config.account_host().to_owned(),
                holding_slug: first.registration.slug.clone(),
            },
            pools[0].clone(),
        ));
    }
    router = router.merge(
        erpx_server_identity_access::public::native_lane_45::router_for_holdings(lane45_targets)
            .map_err(|_| "PRIVATE_API_IDENTITY_LANE45_INVALID")?,
    );

    // Organization: native draft read.
    router = router.merge(
        erpx_server_organization::public::native_draft_read::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_organization::public::native_draft_read::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_ORGANIZATION_DRAFT_READ_INVALID")?,
    );

    // Field Service: part-reservation read, one pool per Holding.
    let field_targets = config.holdings.iter().map(|holding| {
        erpx_server_field_service::public::reservation_part::FieldPartReservationTarget {
            host: holding.host.clone(),
            holding_slug: holding.registration.slug.clone(),
            holding_id: holding.registration.holding_id.as_str().to_owned(),
            origin: format!("https://{}", holding.host),
        }
    }).collect();
    let field_store = std::sync::Arc::new(
        erpx_server_field_service::public::reservation_part::PgFieldPartReservationStore::for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| {
                (holding.registration.holding_id.as_str().to_owned(), pools[i + 1].raw().clone())
            }),
        ),
    );
    router = router.merge(
        erpx_server_field_service::public::reservation_part::field_part_reservation_router(
            field_targets,
            field_store,
        )
        .map_err(|_| "PRIVATE_API_FIELD_RESERVATION_PART_INVALID")?,
    );

    // Manufacturing: production order event history.
    router = router.merge(
        erpx_server_manufacturing::public::order_events_http::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_manufacturing::public::order_events_http::OrderEventsTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_MANUFACTURING_ORDER_EVENTS_INVALID")?,
    );

    // People & Payroll lane 10: active leave-policy settings read.
    router = router.merge(
        erpx_server_people_payroll::public::native_lane_10::router_for_holdings(
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
        .map_err(|_| "PRIVATE_API_PEOPLE_LEAVE_POLICIES_INVALID")?,
    );

    // Commerce: employee-request purchase-order scope read.
    router = router.merge(
        erpx_server_commerce::public::native_employee_request_order::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_commerce::public::native_employee_request_order::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_COMMERCE_EMPLOYEE_ORDER_INVALID")?,
    );

    // Finance lane 7: invoice list read.
    router = router.merge(
        erpx_server_finance::public::native_lane_7::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_finance::public::native_lane_7::LaneTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_FINANCE_LANE7_INVALID")?,
    );

    // Projects: member roster read.
    router = router.merge(
        erpx_server_projects::public::project_members_http::project_members_router(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    crate::project_members::target(
                        holding.host.clone(),
                        holding.registration.slug.clone(),
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_PROJECTS_MEMBERS_INVALID")?,
    );

    // PMS: document entrypoint and related postings.
    router = router.merge(
        erpx_server_pms::public::native_documents::router_for_holdings(
            config
                .holdings
                .iter()
                .enumerate()
                .map(|(i, holding)| {
                    (
                        erpx_server_pms::public::native_documents::DocumentTarget {
                            host: holding.host.clone(),
                            holding_slug: holding.registration.slug.clone(),
                        },
                        pools[i + 1].clone(),
                    )
                })
                .collect(),
        )
        .map_err(|_| "PRIVATE_API_PMS_DOCUMENTS_INVALID")?,
    );

    // Inventory: native stock-transfer detail.
    router = router.merge(
        erpx_server_inventory::public::native_transfer_detail::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| (
                erpx_server_inventory::public::native_transfer_detail::TransferDetailTarget {
                    host: holding.host.clone(),
                    holding_slug: holding.registration.slug.clone(),
                },
                pools[i + 1].clone(),
            )).collect(),
        ).map_err(|_| "PRIVATE_API_INVENTORY_TRANSFER_DETAIL_INVALID")?,
    );

    // Maintenance: organization-node options.
    router = router.merge(
        erpx_server_maintenance::public::native_org_nodes::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| (
                erpx_server_maintenance::public::native_org_nodes::OrgNodeTarget {
                    host: holding.host.clone(),
                    holding_slug: holding.registration.slug.clone(),
                },
                pools[i + 1].clone(),
            )).collect(),
        ).map_err(|_| "PRIVATE_API_MAINTENANCE_ORG_NODES_INVALID")?,
    );

    // POS: customer search.
    router = router.merge(
        erpx_server_pos::public::native_customer_search::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| (
                erpx_server_pos::public::native_customer_search::CustomerSearchTarget {
                    host: holding.host.clone(),
                    holding_slug: holding.registration.slug.clone(),
                },
                pools[i + 1].clone(),
            )).collect(),
        ).map_err(|_| "PRIVATE_API_POS_CUSTOMER_SEARCH_INVALID")?,
    );

    // Operations: routine-template detail read.
    router = router.merge(
        erpx_server_operations::public::native_routines_detail_read::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| (
                erpx_server_operations::public::native_routines_detail_read::TemplateDetailTarget {
                    host: holding.host.clone(),
                    holding_slug: holding.registration.slug.clone(),
                },
                pools[i + 1].clone(),
            )).collect(),
        ).map_err(|_| "PRIVATE_API_OPERATIONS_ROUTINE_DETAIL_INVALID")?,
    );

    // Quality: executive inspection evidence search.
    router = router.merge(
        erpx_server_quality::public::quality_evidence::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| (
                erpx_server_quality::public::quality_evidence::QualityEvidenceTarget {
                    host: holding.host.clone(),
                    holding_slug: holding.registration.slug.clone(),
                },
                pools[i + 1].clone(),
            )).collect(),
        ).map_err(|_| "PRIVATE_API_QUALITY_EVIDENCE_INVALID")?,
    );

    // Legal wf3a13 system planner: holdings/companies are only route keys; the
    // caller executes the returned statement on its own Holding unit.
    router = router.merge(
        crate::legal_wave3::legal_wave3_system(
            Arc::new(crate::legal_wave3::LegalWave3Planner),
            config
                .holdings
                .iter()
                .map(|holding| holding.registration.holding_id.as_str().to_owned())
                .collect(),
            Vec::new(),
        )
        .map_err(|_| "PRIVATE_API_LEGAL_WAVE3_SYSTEM_INVALID")?,
    );

    // Legal: processing-status read.
    router = router.merge(
        erpx_server_legal::public::native_processing_status::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| (
                erpx_server_legal::public::native_processing_status::ProcessingStatusTarget {
                    host: holding.host.clone(),
                    holding_slug: holding.registration.slug.clone(),
                },
                pools[i + 1].clone(),
            )).collect(),
        ).map_err(|_| "PRIVATE_API_LEGAL_PROCESSING_STATUS_INVALID")?,
    );

    // Talent: executive evidence point read.
    router = router.merge(
        erpx_server_talent::public::native_talent_evidence::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| (
                erpx_server_talent::public::native_talent_evidence::TalentEvidenceTarget {
                    host: holding.host.clone(),
                    holding_slug: holding.registration.slug.clone(),
                },
                pools[i + 1].clone(),
            )).collect(),
        ).map_err(|_| "PRIVATE_API_TALENT_EVIDENCE_INVALID")?,
    );

    // Identity & Access lane 46: directory connection by id.
    router = router.merge(
        erpx_server_identity_access::public::native_lane_46::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| (
                erpx_server_identity_access::public::native_lane_46::LaneTarget {
                    host: holding.host.clone(),
                    holding_slug: holding.registration.slug.clone(),
                },
                pools[i + 1].clone(),
            )).collect(),
        ).map_err(|_| "PRIVATE_API_IDENTITY_LANE46_INVALID")?,
    );

    // Organization: Company setup-acceptance read.
    router = router.merge(
        erpx_server_organization::public::native_acceptance_read::router_for_holdings(
            config.holdings.iter().enumerate().map(|(i, holding)| (
                erpx_server_organization::public::native_acceptance_read::LaneTarget {
                    host: holding.host.clone(),
                    holding_slug: holding.registration.slug.clone(),
                },
                pools[i + 1].clone(),
            )).collect(),
        ).map_err(|_| "PRIVATE_API_ORGANIZATION_ACCEPTANCE_INVALID")?,
    );

    Ok(router)
}
