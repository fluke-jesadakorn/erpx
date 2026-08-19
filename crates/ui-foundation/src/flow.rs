use erpx_cqrs::{
    CommandEnvelope, CommandReceipt, QueryConsistency, QueryEnvelope, QueryResult, ReadYourWrites,
};

use crate::{CommandGateway, QueryGateway, UiError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadYourWritesRequest {
    pub projection: String,
    pub partition: String,
    pub timeout_millis: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandThenQuery<C, Q> {
    pub command: CommandReceipt<C>,
    pub query: QueryResult<Q>,
}

#[derive(Debug)]
pub struct ReadYourWritesCoordinator<'a, C, Q> {
    command_gateway: &'a C,
    query_gateway: &'a Q,
}

impl<'a, C, Q> ReadYourWritesCoordinator<'a, C, Q> {
    pub const fn new(command_gateway: &'a C, query_gateway: &'a Q) -> Self {
        Self {
            command_gateway,
            query_gateway,
        }
    }

    /// Executes a command, derives an exact projection target from its receipt,
    /// then queries the read side with read-your-writes consistency.
    ///
    /// Only the query gateway is available for the second step; there is no
    /// fallback path to command state if the projection is not ready.
    pub async fn execute<Command, Query, BuildQuery>(
        &self,
        command: CommandEnvelope<Command>,
        request: ReadYourWritesRequest,
        build_query: BuildQuery,
    ) -> Result<CommandThenQuery<C::Response, Q::Response>, UiError>
    where
        Command: Send,
        Query: Send,
        C: CommandGateway<Command>,
        Q: QueryGateway<Query>,
        BuildQuery:
            FnOnce(&CommandReceipt<C::Response>, QueryConsistency) -> QueryEnvelope<Query> + Send,
    {
        let receipt = self.command_gateway.execute(command).await?;
        let target = receipt.projection_target().ok_or_else(|| {
            UiError::not_ready("The command produced no projection position to wait for.")
        })?;
        let consistency = QueryConsistency::ReadYourWrites(ReadYourWrites {
            projection: request.projection,
            partition: request.partition,
            target,
            timeout_millis: request.timeout_millis,
        });
        consistency
            .validate()
            .map_err(|_| UiError::invalid("The read-your-writes policy is invalid."))?;
        let query = build_query(&receipt, consistency);
        query
            .validate()
            .map_err(|_| UiError::invalid("The query context is invalid."))?;
        let query = self.query_gateway.execute(query).await?;
        Ok(CommandThenQuery {
            command: receipt,
            query,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        sync::Arc,
        task::{Context, Poll, Wake, Waker},
    };

    use erpx_cqrs::{
        AggregateVersion, CommandReceipt, EventPosition, FreshnessState, IdempotencyKey,
        ProjectionMetadata, QueryMetadata, QueryResult,
    };
    use erpx_kernel::{
        ActorAttribution, ActorKind, AggregateId, AuthorityScope, CommandId, CorrelationId,
        EventId, QueryId,
    };

    use super::*;
    use crate::{FakeCommandGateway, FakeQueryGateway, RequestContextBuilder, SessionContext};

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::from(Arc::new(NoopWake));
        let mut context = Context::from_waker(&waker);
        let mut future = Box::pin(future);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(output) => return output,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    fn aggregate_id(value: u128) -> AggregateId {
        AggregateId::try_from_uuid(
            format!("{value:032x}")
                .parse()
                .expect("test UUID must parse"),
        )
        .unwrap()
    }

    fn command_id(value: u128) -> CommandId {
        CommandId::try_from_uuid(
            format!("{value:032x}")
                .parse()
                .expect("test UUID must parse"),
        )
        .unwrap()
    }

    fn correlation_id(value: u128) -> CorrelationId {
        CorrelationId::try_from_uuid(
            format!("{value:032x}")
                .parse()
                .expect("test UUID must parse"),
        )
        .unwrap()
    }

    fn event_id(value: u128) -> EventId {
        EventId::try_from_uuid(
            format!("{value:032x}")
                .parse()
                .expect("test UUID must parse"),
        )
        .unwrap()
    }

    fn query_id(value: u128) -> QueryId {
        QueryId::try_from_uuid(
            format!("{value:032x}")
                .parse()
                .expect("test UUID must parse"),
        )
        .unwrap()
    }

    fn builder() -> RequestContextBuilder {
        RequestContextBuilder::try_new(
            SessionContext {
                actor: ActorAttribution {
                    principal_ref: "user:1".into(),
                    kind: ActorKind::HoldingUser,
                    session_id: None,
                    represented_principal_ref: None,
                    support_capability_id: None,
                },
                active_scope: AuthorityScope::Platform,
            },
            "quality inspection",
            true,
            Some("quality".into()),
        )
        .unwrap()
    }

    fn command_receipt() -> CommandReceipt<&'static str> {
        CommandReceipt {
            command_id: command_id(1),
            correlation_id: correlation_id(2),
            aggregate: AggregateVersion {
                context: "quality".into(),
                aggregate_type: "inspection".into(),
                aggregate_id: aggregate_id(3),
                version: 1,
            },
            committed_events: vec![EventPosition {
                event_id: event_id(4),
                global_sequence: 17,
            }],
            committed_at: "2026-08-05T01:02:03Z"
                .parse()
                .expect("test timestamp must parse"),
            replayed: false,
            response: "approved",
        }
    }

    fn query_result() -> QueryResult<&'static str> {
        QueryResult {
            value: "inspection view",
            metadata: QueryMetadata {
                correlation_id: correlation_id(2),
                projection: ProjectionMetadata {
                    checkpoint: None,
                    freshness: FreshnessState::Current,
                    lag_millis: Some(0),
                },
                consistency: QueryConsistency::Eventual,
            },
        }
    }

    #[test]
    fn coordinator_queries_with_the_exact_command_projection_target() {
        let builder = builder();
        let command = builder
            .command(
                command_id(1),
                IdempotencyKey::try_new("quality:inspection:1").unwrap(),
                correlation_id(2),
                None,
                Some(0),
                "approve",
            )
            .unwrap();
        let commands = FakeCommandGateway::from_responses([Ok(command_receipt())]);
        let queries = FakeQueryGateway::from_responses([Ok(query_result())]);
        let coordinator = ReadYourWritesCoordinator::new(&commands, &queries);
        let result = block_on(coordinator.execute(
            command,
            ReadYourWritesRequest {
                projection: "quality.inspections".into(),
                partition: "holding:1".into(),
                timeout_millis: 500,
            },
            |receipt, consistency| {
                builder
                    .query(query_id(5), receipt.correlation_id, consistency, "get")
                    .unwrap()
            },
        ))
        .unwrap();

        assert_eq!(result.command.response, "approved");
        assert_eq!(result.query.value, "inspection view");
        assert_eq!(commands.call_count(), 1);
        assert_eq!(queries.call_count(), 1);
        let received = queries.received();
        let QueryConsistency::ReadYourWrites(requirement) = &received[0].consistency else {
            panic!("query must use read-your-writes consistency")
        };
        assert_eq!(requirement.target.global_sequence, 17);
        assert_eq!(requirement.target.event_id, event_id(4));
    }

    #[test]
    fn invalid_wait_policy_never_reaches_the_read_gateway() {
        let builder = builder();
        let command = builder
            .command(
                command_id(1),
                IdempotencyKey::try_new("quality:inspection:1").unwrap(),
                correlation_id(2),
                None,
                Some(0),
                "approve",
            )
            .unwrap();
        let commands = FakeCommandGateway::from_responses([Ok(command_receipt())]);
        let queries = FakeQueryGateway::from_responses([Ok(query_result())]);
        let coordinator = ReadYourWritesCoordinator::new(&commands, &queries);
        let error = block_on(coordinator.execute(
            command,
            ReadYourWritesRequest {
                projection: "quality.inspections".into(),
                partition: "holding:1".into(),
                timeout_millis: u64::MAX,
            },
            |receipt, consistency| {
                builder
                    .query(query_id(5), receipt.correlation_id, consistency, "get")
                    .unwrap()
            },
        ))
        .unwrap_err();
        assert_eq!(error.kind(), crate::UiErrorKind::Invalid);
        assert_eq!(commands.call_count(), 1);
        assert_eq!(queries.call_count(), 0);
    }
}
