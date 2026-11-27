//! HTTP client for the spatial launcher's Kama-Foundry project tickets API.
//!
//! Fetches projects and active tickets from `https://foundry.vuidart.com/api/v1`
//! authenticated with `Authorization: Bearer <token>`.

use std::collections::HashMap;
use std::time::Duration;

use serde::Deserialize;

use crate::model::widgets::{ProjectStatus, ProjectTask};

/// Web platform URL for Kama-Foundry.
pub const PLATFORM_URL: &str = "https://foundry.vuidart.com";

/// Base URL of the Kama-Foundry v1 API.
pub const API_BASE: &str = "https://foundry.vuidart.com/api/v1";

/// Active statuses fetched for the « Tâches des projets » card.
const ACTIVE_STATUSES: &str = "waiting_human,in_progress,review,queued,todo,backlog,failed";
const TICKETS_LIMIT: usize = 50;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Why loading tickets from Kama-Foundry failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FoundryError {
    /// HTTP 401/403: missing, invalid, or revoked API key.
    Unauthorized,
    /// Transport, HTTP status, or JSON decoding failure.
    Failed(String),
}

impl FoundryError {
    /// Short user-facing French label shown inside the card.
    pub fn user_message(&self) -> &'static str {
        match self {
            Self::Unauthorized => {
                "Clé API Kama-Foundry invalide ou expirée. Vérifiez-la dans Paramètres."
            }
            Self::Failed(_) => "Impossible de charger les tickets Kama-Foundry depuis le serveur.",
        }
    }
}

#[derive(Debug, Deserialize)]
struct ProjectsResponse {
    #[serde(default)]
    projects: Vec<ApiProject>,
}

#[derive(Debug, Deserialize)]
struct ApiProject {
    slug: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct TicketsResponse {
    #[serde(default)]
    tickets: Vec<ApiTicket>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiTicket {
    id: String,
    #[serde(default)]
    project_slug: String,
    title: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    assigned_agent: String,
    #[serde(default)]
    url: String,
}

fn get_json(agent: &ureq::Agent, url: &str, token: &str) -> Result<String, FoundryError> {
    let mut response = agent
        .get(url)
        .header("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(|error| FoundryError::Failed(error.to_string()))?;

    let status = response.status().as_u16();
    if status == 401 || status == 403 {
        return Err(FoundryError::Unauthorized);
    }
    if !(200..300).contains(&status) {
        return Err(FoundryError::Failed(format!("HTTP {status}")));
    }

    response
        .body_mut()
        .read_to_string()
        .map_err(|error| FoundryError::Failed(error.to_string()))
}

/// Fetches active tickets (and project display names) from Kama-Foundry using
/// `token` as a Bearer token.
pub fn fetch(token: &str) -> Result<Vec<ProjectTask>, FoundryError> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .http_status_as_error(false)
        .build();
    let agent: ureq::Agent = config.into();

    let tickets_url = format!("{API_BASE}/tickets?status={ACTIVE_STATUSES}&limit={TICKETS_LIMIT}");
    let tickets_json = get_json(&agent, &tickets_url, token)?;

    let project_names = get_json(&agent, &format!("{API_BASE}/projects"), token)
        .ok()
        .and_then(|json| parse_projects(&json).ok())
        .unwrap_or_default();

    parse_tickets(&tickets_json, &project_names)
}

/// Parses `GET /projects` into a `slug -> display name` lookup map.
pub fn parse_projects(json: &str) -> Result<HashMap<String, String>, FoundryError> {
    let parsed: ProjectsResponse =
        serde_json::from_str(json).map_err(|error| FoundryError::Failed(error.to_string()))?;
    Ok(parsed
        .projects
        .into_iter()
        .map(|project| (project.slug, project.name))
        .collect())
}

/// Parses `GET /tickets` into sorted [`ProjectTask`] items.
pub fn parse_tickets(
    json: &str,
    project_names: &HashMap<String, String>,
) -> Result<Vec<ProjectTask>, FoundryError> {
    let parsed: TicketsResponse =
        serde_json::from_str(json).map_err(|error| FoundryError::Failed(error.to_string()))?;
    let mut tasks: Vec<ProjectTask> = parsed
        .tickets
        .into_iter()
        .map(|ticket| {
            let project = project_names
                .get(&ticket.project_slug)
                .cloned()
                .unwrap_or(ticket.project_slug);
            let status = ProjectStatus::from_api(&ticket.status);
            ProjectTask {
                id: ticket.id,
                project,
                title: ticket.title,
                status,
                assigned_agent: (!ticket.assigned_agent.is_empty())
                    .then_some(ticket.assigned_agent),
                url: (!ticket.url.is_empty()).then_some(ticket.url),
            }
        })
        .collect();
    tasks.sort_by_key(|task| task.status.sort_rank());
    Ok(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_projects_and_tickets_and_sorts_by_priority() {
        let projects_json = r#"{"total":1,"projects":[{"id":"0rpab56yu882is4","slug":"kama-foundry","name":"Kama-Foundry","ticker":"FOUNDRY"}]}"#;
        let project_names = parse_projects(projects_json).unwrap();
        assert_eq!(
            project_names.get("kama-foundry").map(String::as_str),
            Some("Kama-Foundry")
        );

        let tickets_json = r#"{
            "total": 4,
            "tickets": [
                {"id":"FOUNDRY-31","projectId":"0rpab56yu882is4","projectSlug":"kama-foundry","title":"Nettoyer workspace","status":"backlog","assignedAgent":"","url":"https://foundry.vuidart.com/projects/kama-foundry/tickets/FOUNDRY-31"},
                {"id":"FOUNDRY-20","projectId":"0rpab56yu882is4","projectSlug":"kama-foundry","title":"Multi PR","status":"review","assignedAgent":"antigravity","url":"https://foundry.vuidart.com/projects/kama-foundry/tickets/FOUNDRY-20"},
                {"id":"FOUNDRY-28","projectId":"0rpab56yu882is4","projectSlug":"kama-foundry","title":"Réponse agent","status":"waiting_human","assignedAgent":"claude","url":"https://foundry.vuidart.com/projects/kama-foundry/tickets/FOUNDRY-28"},
                {"id":"FOUNDRY-29","projectId":"0rpab56yu882is4","projectSlug":"kama-foundry","title":"Fix build","status":"in_progress","assignedAgent":"antigravity","url":"https://foundry.vuidart.com/projects/kama-foundry/tickets/FOUNDRY-29"}
            ]
        }"#;

        let tasks = parse_tickets(tickets_json, &project_names).unwrap();
        assert_eq!(tasks.len(), 4);
        assert_eq!(tasks[0].id, "FOUNDRY-28");
        assert_eq!(tasks[0].status, ProjectStatus::WaitingHuman);
        assert_eq!(tasks[0].project, "Kama-Foundry");
        assert_eq!(tasks[0].assigned_agent.as_deref(), Some("claude"));
        assert_eq!(tasks[1].id, "FOUNDRY-29");
        assert_eq!(tasks[1].status, ProjectStatus::InProgress);
        assert_eq!(tasks[2].id, "FOUNDRY-20");
        assert_eq!(tasks[2].status, ProjectStatus::Review);
        assert_eq!(tasks[3].id, "FOUNDRY-31");
        assert_eq!(tasks[3].status, ProjectStatus::Backlog);
        assert_eq!(tasks[3].assigned_agent, None);
    }

    #[test]
    fn parses_empty_tickets_list() {
        let tasks = parse_tickets(r#"{"total":0,"tickets":[]}"#, &HashMap::new()).unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn rejects_invalid_json() {
        let err = parse_tickets("not json", &HashMap::new()).unwrap_err();
        assert!(matches!(err, FoundryError::Failed(_)));
    }
}
