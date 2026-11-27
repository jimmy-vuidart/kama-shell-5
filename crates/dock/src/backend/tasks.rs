//! HTTP client for the spatial launcher's daily tasks API.
//!
//! Currently only `GET /api/kama/public/tasks` is implemented on the server,
//! authenticated with `X-API-Key: <token>`.

use std::time::Duration;

use serde::Deserialize;

use crate::model::widgets::DailyTask;

/// Endpoint returning the user's daily tasks.
pub const TASKS_ENDPOINT: &str = "https://home.vuidart.com/api/kama/public/tasks";

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Why loading tasks from the remote API failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TasksError {
    /// HTTP 401/403: missing, invalid, or revoked token.
    Unauthorized,
    /// Transport, HTTP status, or JSON decoding failure.
    Failed(String),
}

impl TasksError {
    /// Short user-facing French label shown inside the card.
    pub fn user_message(&self) -> &'static str {
        match self {
            Self::Unauthorized => "Token API invalide ou expiré. Vérifiez-le dans Paramètres.",
            Self::Failed(_) => "Impossible de charger les tâches depuis le serveur.",
        }
    }
}

#[derive(Debug, Deserialize)]
struct TasksResponse {
    #[serde(default)]
    items: Vec<ApiTask>,
}

#[derive(Debug, Deserialize)]
struct ApiTask {
    id: String,
    title: String,
    #[serde(default)]
    completed: bool,
    #[serde(default)]
    priority: bool,
}

/// Fetches daily tasks from [`TASKS_ENDPOINT`] using `token` in `X-API-Key`.
pub fn fetch(token: &str) -> Result<Vec<DailyTask>, TasksError> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .http_status_as_error(false)
        .build();
    let agent: ureq::Agent = config.into();

    let mut response = agent
        .get(TASKS_ENDPOINT)
        .header("X-API-Key", token)
        .call()
        .map_err(|error| TasksError::Failed(error.to_string()))?;

    let status = response.status().as_u16();
    if status == 401 || status == 403 {
        return Err(TasksError::Unauthorized);
    }
    if !(200..300).contains(&status) {
        return Err(TasksError::Failed(format!("HTTP {status}")));
    }

    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|error| TasksError::Failed(error.to_string()))?;
    parse_response(&body)
}

/// Parses the JSON payload returned by [`TASKS_ENDPOINT`] into [`DailyTask`] items.
pub fn parse_response(json: &str) -> Result<Vec<DailyTask>, TasksError> {
    let parsed: TasksResponse =
        serde_json::from_str(json).map_err(|error| TasksError::Failed(error.to_string()))?;
    Ok(parsed
        .items
        .into_iter()
        .map(|item| DailyTask {
            id: item.id,
            title: item.title,
            meta: if item.priority {
                "Prioritaire".to_owned()
            } else {
                String::new()
            },
            done: item.completed,
            read_only: true,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_api_payload() {
        let json = r#"{"items":[{"id":"60db8c8fc99be33","title":"saaaaaaaluuut","completed":true,"priority":false,"created":"2026-08-26 20:53:12.360Z","updated":"2026-08-26 20:53:12.360Z"},{"id":"abc","title":"Acheter du café","completed":false,"priority":true}]}"#;
        let tasks = parse_response(json).unwrap();
        assert_eq!(tasks.len(), 2);

        assert_eq!(tasks[0].id, "60db8c8fc99be33");
        assert_eq!(tasks[0].title, "saaaaaaaluuut");
        assert!(tasks[0].done);
        assert_eq!(tasks[0].meta, "");
        assert!(tasks[0].read_only);

        assert_eq!(tasks[1].id, "abc");
        assert_eq!(tasks[1].title, "Acheter du café");
        assert!(!tasks[1].done);
        assert_eq!(tasks[1].meta, "Prioritaire");
        assert!(tasks[1].read_only);
    }

    #[test]
    fn parses_empty_items_list() {
        let tasks = parse_response(r#"{"items":[]}"#).unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn rejects_invalid_json() {
        let err = parse_response("not json").unwrap_err();
        assert!(matches!(err, TasksError::Failed(_)));
    }
}
