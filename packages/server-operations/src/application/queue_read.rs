//! Native Operations queue read use cases. The repository owns only
//! Operations tables; source visibility and people labels remain public ports
//! implemented by composition using the current Records and People owners.
use crate::domain::queue_read::{
    ExceptionView, MAX_EXCEPTIONS, MAX_HISTORY, MAX_PEOPLE, MAX_POLICIES, OperationsDetail,
    OperationsEventRow, OperationsReadActor, OperationsWorkspace, PersonOption, QueuePolicyRow,
    SourceView, WorkspaceData, not_found, read_limit, summary, validate_source,
};
use erpx_server_shared::public::BusinessError;
use std::future::Future;
use std::pin::Pin;

pub type OperationsReadFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait OperationsReadRepository: Send {
    type Error: From<BusinessError> + Send;
    fn workspace<'a>(
        &'a mut self,
        company_id: &'a str,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
    ) -> OperationsReadFuture<'a, WorkspaceData, Self::Error>;
    fn history<'a>(
        &'a mut self,
        exception_id: &'a str,
    ) -> OperationsReadFuture<'a, Vec<OperationsEventRow>, Self::Error>;
}

/// Records/public source port. Returning `Ok(None)` is an expected hidden
/// source (the existing 403/404 visibility contract); transport failures must
/// propagate so the workspace never silently drops an unreadable owner.
pub trait OperationsSourceVisibility {
    type Error: Send;
    fn visible<'a>(
        &'a mut self,
        row: &'a crate::domain::queue_read::QueueException,
    ) -> OperationsReadFuture<'a, Option<SourceView>, Self::Error>;
}

/// Identity/People public recipient reference. Selection is current; this
/// projection grants no assignment or command authority.
pub trait OperationsPeopleDirectory {
    type Error: Send;
    fn people(&mut self) -> OperationsReadFuture<'_, Vec<PersonOption>, Self::Error>;
}

fn people(mut rows: Vec<PersonOption>) -> Result<Vec<PersonOption>, BusinessError> {
    if rows.len() > MAX_PEOPLE {
        return Err(read_limit(
            "The Operations recipient list exceeds its bounded read.",
        ));
    }
    rows.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.id.cmp(&right.id))
    });
    rows.dedup_by(|left, right| left.id == right.id);
    if rows
        .iter()
        .any(|row| row.id.is_empty() || row.id.len() > 200 || row.name.len() > 400)
    {
        return Err(read_limit(
            "The Operations recipient list contains an invalid value.",
        ));
    }
    Ok(rows)
}

pub async fn operations_workspace<R, V, P>(
    actor: &OperationsReadActor,
    repository: &mut R,
    sources: &mut V,
    people_directory: &mut P,
    now: i64,
) -> Result<OperationsWorkspace, R::Error>
where
    R: OperationsReadRepository,
    V: OperationsSourceVisibility,
    P: OperationsPeopleDirectory,
    R::Error: From<V::Error> + From<P::Error>,
{
    actor.require_read(now)?;
    let data = repository
        .workspace(
            &actor.company_id,
            actor.branch_id.as_deref(),
            actor.department_id.as_deref(),
        )
        .await?;
    if data.exceptions.len() > MAX_EXCEPTIONS || data.policies.len() > MAX_POLICIES {
        return Err(read_limit("The Operations queue exceeds its bounded read.").into());
    }
    let mut exceptions = Vec::with_capacity(data.exceptions.len());
    for row in data.exceptions {
        let source = match sources.visible(&row).await {
            Ok(value) => value,
            Err(error) => return Err(R::Error::from(error)),
        };
        let Some(source) = source else {
            continue;
        };
        validate_source(&row, &source, actor)?;
        exceptions.push(ExceptionView { row, source });
    }
    let policies: Vec<QueuePolicyRow> = data.policies;
    let people_rows = match people_directory.people().await {
        Ok(value) => value,
        Err(error) => return Err(R::Error::from(error)),
    };
    let people = people(people_rows)?;
    Ok(OperationsWorkspace {
        summary: summary(&exceptions, now),
        exceptions,
        policies,
        people,
        actor_id: actor.user_id.clone(),
        can_write: actor.can_write(),
        can_review: actor.can_review(),
    })
}

pub async fn operations_detail<R, V, P>(
    actor: &OperationsReadActor,
    repository: &mut R,
    sources: &mut V,
    people_directory: &mut P,
    id: &str,
    now: i64,
) -> Result<OperationsDetail, R::Error>
where
    R: OperationsReadRepository,
    V: OperationsSourceVisibility,
    P: OperationsPeopleDirectory,
    R::Error: From<V::Error> + From<P::Error>,
{
    let workspace = operations_workspace(actor, repository, sources, people_directory, now).await?;
    let row = workspace
        .exceptions
        .iter()
        .find(|view| view.row.id == id)
        .cloned()
        .ok_or_else(not_found)?;
    let history = repository.history(id).await?;
    if history.len() > MAX_HISTORY {
        return Err(read_limit("The Operations history exceeds its bounded read.").into());
    }
    Ok(OperationsDetail {
        workspace,
        row,
        history,
    })
}
