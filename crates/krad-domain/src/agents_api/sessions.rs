use super::{ApiError, ErrorCode, SessionInfo};
use crate::environment::EnvironmentData;
use crate::ids::GroupId;

const NO_SUCH_GROUP: &str = "No group with that name in your environment";
const NO_CALLER_GROUP: &str = "The caller belongs to no group; name one with --group";

#[must_use]
pub fn session_infos(data: &EnvironmentData) -> Vec<SessionInfo> {
    data.sessions
        .iter()
        .map(|session| SessionInfo {
            session: session.id.as_u64(),
            name: session.name.clone(),
            group: session
                .group
                .and_then(|id| data.group(id))
                .map(|group| group.name.clone()),
            is_current: data.active_session == Some(session.id),
            terminals: session
                .tabs
                .tabs
                .iter()
                .flat_map(|tab| tab.tiles.iter().map(|tile| tile.id.as_u64()))
                .collect(),
        })
        .collect()
}

/// # Errors
///
/// Returns the refusal when no group is named and the caller has none, or when the named group
/// is not in the environment.
pub fn target_group(
    data: &EnvironmentData,
    caller: Option<GroupId>,
    named: Option<&str>,
) -> Result<GroupId, ApiError> {
    let Some(name) = named else {
        return caller.ok_or_else(|| ApiError::new(ErrorCode::Unsupported, NO_CALLER_GROUP));
    };
    data.groups
        .iter()
        .find(|group| group.name == name)
        .map(|group| group.id)
        .ok_or_else(|| ApiError::new(ErrorCode::NotFound, NO_SUCH_GROUP))
}

#[cfg(test)]
mod tests;
