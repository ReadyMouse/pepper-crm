//! # Weekly Digest Rendering
//!
//!   Builds Tera context and HTML for the weekly email digest: tasks, reconnects, travel,
//!   birthdays, and random people picks — in a fixed section order shared by preview and send.
//!
//! INPUT:
//!   - Due tasks/reconnects, parsed contacts, optional travel snapshot, cache root, as-of date.
//!
//! OUTPUT:
//!   - `DigestInput` serializable payload and `render_digest_email` HTML + subject line.
//!
//! NOTES:
//!   - Template: repo-root `templates/digest.html` (run binaries from workspace root).
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use crate::birthdays::{upcoming_birthdays_from_contacts, BIRTHDAY_WINDOW_DAYS};
use crate::models::{
    Contact, DueReconnectInfo, MatchReason, PendingTaskInfo, RandomPickInfo, RandomPickWeek,
    TravelWeekSnapshot,
};
use crate::random_pick::resolve_random_picks;
use anyhow::{Context, Result};
use chrono::{Local, NaiveDate};
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use std::path::Path;
use tera::{Context as TeraContext, Tera};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DigestTask {
    pub contact_name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DigestReconnect {
    pub contact_name: String,
    pub due_date: String,
    pub tag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DigestTravelMatch {
    pub full_name: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DigestTravelTrip {
    pub title: String,
    pub date_range: String,
    pub matches: Vec<DigestTravelMatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DigestBirthday {
    pub contact_name: String,
    pub date_label: String,
    pub when_label: String,
    pub age_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DigestRandomPick {
    pub full_name: String,
    pub org: String,
    pub location: String,
    pub categories: String,
    pub note: String,
}

/// Full payload for `templates/digest.html`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DigestInput {
    pub tasks: Vec<DigestTask>,
    pub reconnects: Vec<DigestReconnect>,
    pub travel_trips: Vec<DigestTravelTrip>,
    pub travel_match_count: usize,
    pub has_travel_snapshot: bool,
    pub birthdays: Vec<DigestBirthday>,
    pub birthday_window_days: u32,
    pub random_picks: Vec<DigestRandomPick>,
    pub random_week_label: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DigestOutput {
    pub html: String,
    pub subject: String,
}

impl DigestInput {
    pub fn task_count(&self) -> usize {
        self.tasks.len()
    }

    pub fn reconnect_count(&self) -> usize {
        self.reconnects.len()
    }

    pub fn birthday_count(&self) -> usize {
        self.birthdays.len()
    }

    pub fn random_pick_count(&self) -> usize {
        self.random_picks.len()
    }
}

pub fn tasks_from_pending(tasks: &[PendingTaskInfo]) -> Vec<DigestTask> {
    tasks
        .iter()
        .map(|t| DigestTask {
            contact_name: t.full_name.clone(),
            description: t.description.clone(),
        })
        .collect()
}

pub fn reconnects_from_infos(reconnects: &[DueReconnectInfo]) -> Vec<DigestReconnect> {
    reconnects
        .iter()
        .map(|r| DigestReconnect {
            contact_name: r.full_name.clone(),
            due_date: r.due_date.format("%b %-d, %Y").to_string(),
            tag: r.tag.clone(),
        })
        .collect()
}

fn format_location(city: Option<&str>, state: Option<&str>) -> String {
    match (city.filter(|s| !s.is_empty()), state.filter(|s| !s.is_empty())) {
        (Some(c), Some(s)) => format!("{c}, {s}"),
        (Some(c), None) => c.to_string(),
        (None, Some(s)) => s.to_string(),
        (None, None) => String::new(),
    }
}

fn format_date_range(start: NaiveDate, end: NaiveDate) -> String {
    format!(
        "{} – {}",
        start.format("%b %-d"),
        end.format("%b %-d, %Y")
    )
}

pub fn travel_trips_from_snapshot(snapshot: &TravelWeekSnapshot) -> (Vec<DigestTravelTrip>, usize) {
    let mut total = 0;
    let trips = snapshot
        .trips
        .iter()
        .map(|trip| {
            let matches: Vec<DigestTravelMatch> = trip
                .matches
                .iter()
                .map(|m| {
                    total += 1;
                    let city = m.city.clone().unwrap_or_else(|| "Unknown".to_string());
                    let distance_mi = format!("{:.0}", m.distance_km * 0.621371);
                    let detail = match &m.reason {
                        MatchReason::TaggedBeforeTrip => {
                            let tag = m
                                .reconnect_tag
                                .as_deref()
                                .unwrap_or("before trip");
                            format!(
                                "{city} · ~{distance_mi} mi · tagged \"Reconnect: {tag}\""
                            )
                        }
                        MatchReason::Proximity => format!("{city} · ~{distance_mi} mi"),
                    };
                    DigestTravelMatch {
                        full_name: m.full_name.clone(),
                        detail,
                    }
                })
                .collect();
            DigestTravelTrip {
                title: trip.title.clone(),
                date_range: format_date_range(trip.start, trip.end),
                matches,
            }
        })
        .collect();
    (trips, total)
}

pub fn birthdays_from_contacts(contacts: &[Contact], as_of: NaiveDate) -> Vec<DigestBirthday> {
    upcoming_birthdays_from_contacts(contacts, as_of, BIRTHDAY_WINDOW_DAYS)
        .into_iter()
        .map(|b| {
            let date_label = b.occurrence.format("%b %-d").to_string();
            let when_label = match b.days_until {
                0 => "Today".to_string(),
                1 => "Tomorrow".to_string(),
                n => format!("In {n} days"),
            };
            let age_label = b
                .turning_age
                .map(|a| format!("Turning {a}"))
                .unwrap_or_default();
            DigestBirthday {
                contact_name: b.full_name,
                date_label,
                when_label,
                age_label,
            }
        })
        .collect()
}

pub fn random_picks_for_digest(picks: &[RandomPickInfo]) -> Vec<DigestRandomPick> {
    picks
        .iter()
        .map(|p| DigestRandomPick {
            full_name: p.full_name.clone(),
            org: p.org.clone().unwrap_or_default(),
            location: format_location(p.city.as_deref(), p.state.as_deref()),
            categories: p.categories.join(", "),
            note: p.note.clone(),
        })
        .collect()
}

/// Build the full digest payload from CRM data sources.
pub fn build_digest_input(
    tasks: Vec<DigestTask>,
    reconnects: Vec<DigestReconnect>,
    contacts: &[Contact],
    snapshot: Option<&TravelWeekSnapshot>,
    random_week: RandomPickWeek,
    as_of: NaiveDate,
) -> DigestInput {
    let (travel_trips, travel_match_count) = snapshot
        .map(travel_trips_from_snapshot)
        .unwrap_or((Vec::new(), 0));

    DigestInput {
        tasks,
        reconnects,
        travel_trips,
        travel_match_count,
        has_travel_snapshot: snapshot.is_some(),
        birthdays: birthdays_from_contacts(contacts, as_of),
        birthday_window_days: BIRTHDAY_WINDOW_DAYS,
        random_picks: random_picks_for_digest(&random_week.picks),
        random_week_label: random_week.week_label,
    }
}

/// Convenience builder starting from DB task rows and due reconnect infos.
pub fn build_digest_input_from_due(
    tasks: &[PendingTaskInfo],
    reconnects: &[DueReconnectInfo],
    contacts: &[Contact],
    snapshot: Option<&TravelWeekSnapshot>,
    cache_root: &Path,
    as_of: NaiveDate,
) -> Result<DigestInput> {
    let random_week = resolve_random_picks(contacts, cache_root, as_of, crate::RANDOM_PICK_COUNT)?;
    Ok(build_digest_input(
        tasks_from_pending(tasks),
        reconnects_from_infos(reconnects),
        contacts,
        snapshot,
        random_week,
        as_of,
    ))
}

pub fn digest_subject(input: &DigestInput) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !input.tasks.is_empty() {
        parts.push(format!(
            "{} task{}",
            input.tasks.len(),
            if input.tasks.len() == 1 { "" } else { "s" }
        ));
    }
    if !input.reconnects.is_empty() {
        parts.push(format!(
            "{} reconnect{}",
            input.reconnects.len(),
            if input.reconnects.len() == 1 { "" } else { "s" }
        ));
    }
    if input.travel_match_count > 0 {
        parts.push(format!(
            "{} travel match{}",
            input.travel_match_count,
            if input.travel_match_count == 1 { "" } else { "es" }
        ));
    }
    if !input.birthdays.is_empty() {
        parts.push(format!(
            "{} birthday{}",
            input.birthdays.len(),
            if input.birthdays.len() == 1 { "" } else { "s" }
        ));
    }
    if parts.is_empty() {
        "Pepper CRM: Weekly digest".to_string()
    } else {
        format!("Pepper CRM: {}", parts.join(", "))
    }
}

pub fn digest_dashboard_url() -> Option<String> {
    std::env::var("PEPPER_DASHBOARD_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| Some("http://127.0.0.1:3000".to_string()))
}

pub fn digest_tera_context(input: &DigestInput) -> TeraContext {
    let mut context = TeraContext::new();
    context.insert("date", &Local::now().format("%B %d, %Y").to_string());
    context.insert("dashboard_url", &digest_dashboard_url());
    context.insert("tasks", &input.tasks);
    context.insert("reconnects", &input.reconnects);
    context.insert("travel_trips", &input.travel_trips);
    context.insert("travel_match_count", &input.travel_match_count);
    context.insert("has_travel_snapshot", &input.has_travel_snapshot);
    context.insert("birthdays", &input.birthdays);
    context.insert("birthday_window_days", &input.birthday_window_days);
    context.insert("random_picks", &input.random_picks);
    context.insert("random_week_label", &input.random_week_label);
    context.insert("task_count", &input.task_count());
    context.insert("reconnect_count", &input.reconnect_count());
    context.insert("birthday_count", &input.birthday_count());
    context.insert("random_pick_count", &input.random_pick_count());
    context
}

pub fn render_digest_email(input: &DigestInput) -> Result<DigestOutput> {
    let mut tera = Tera::new("templates/**/*.html").context("Failed to load digest templates")?;
    tera.autoescape_on(vec!["html"]);
    let html = tera
        .render("digest.html", &digest_tera_context(input))
        .context("Failed to render digest.html")?;
    Ok(DigestOutput {
        html,
        subject: digest_subject(input),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::RandomPickWeek;

    #[test]
    fn digest_subject_includes_sections() {
        let input = DigestInput {
            tasks: vec![DigestTask {
                contact_name: "A".into(),
                description: "todo".into(),
            }],
            reconnects: vec![DigestReconnect {
                contact_name: "B".into(),
                due_date: "May 1".into(),
                tag: "6 months".into(),
            }],
            travel_trips: vec![],
            travel_match_count: 2,
            has_travel_snapshot: true,
            birthdays: vec![DigestBirthday {
                contact_name: "C".into(),
                date_label: "May 5".into(),
                when_label: "In 3 days".into(),
                age_label: String::new(),
            }],
            birthday_window_days: BIRTHDAY_WINDOW_DAYS,
            random_picks: vec![],
            random_week_label: "May 19 – May 25, 2026".into(),
        };
        let subject = digest_subject(&input);
        assert!(subject.contains("1 task"));
        assert!(subject.contains("1 reconnect"));
        assert!(subject.contains("2 travel matches"));
        assert!(subject.contains("1 birthday"));
    }

    #[test]
    fn build_digest_input_orders_sections_data() {
        let input = build_digest_input(
            vec![],
            vec![],
            &[],
            None,
            RandomPickWeek {
                week_id: "2026-W21".into(),
                week_label: "May 19 – May 25, 2026".into(),
                picks: vec![],
                eligible_count: 0,
                shuffled: false,
            },
            NaiveDate::from_ymd_opt(2026, 5, 19).unwrap(),
        );
        assert!(!input.has_travel_snapshot);
        assert_eq!(input.random_week_label, "May 19 – May 25, 2026");
    }
}
