//! # Dashboard Template Render Regression Test
//!
//!   Ensures `dashboard.html` renders on a small thread stack without overflow.
//!
//! INPUT:
//!   - `pepper-web/templates/dashboard.html` and `partials/header.html`.
//!   - Minimal Tera context mirroring production dashboard variables.
//!
//! OUTPUT:
//!   - Passing test when HTML renders and key sections are present.
//!
//! NOTES:
//!   - Guards against deep Tera recursion or template growth regressions.
//!   - Run: `cargo test -p pepper-web --test dashboard_render`
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use std::path::PathBuf;
use tera::{Context, Tera};

fn tera_with_dashboard() -> Tera {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates");
    let mut tera = Tera::default();
    let header = std::fs::read_to_string(root.join("partials/header.html")).expect("header");
    let dashboard = std::fs::read_to_string(root.join("dashboard.html")).expect("dashboard");
    tera.add_raw_template("partials/header.html", &header)
        .expect("compile header");
    tera.add_raw_template("dashboard.html", &dashboard)
        .expect("compile dashboard");
    tera.autoescape_on(vec!["html"]);
    tera
}

#[test]
fn render_dashboard_does_not_stack_overflow() {
    let tera = tera_with_dashboard();
    let mut context = Context::new();
    context.insert("nav_active", "dashboard");
    context.insert("date", "test");
    context.insert("tasks", &Vec::<(String, String)>::new());
    context.insert("reconnects", &Vec::<(String, String, String, String)>::new());
    context.insert("task_count", &0usize);
    context.insert("reconnect_count", &0usize);
    context.insert("reconnect_window_days", &7u32);
    context.insert("reconnect_snoozed", &false);
    context.insert("task_completed", &false);
    context.insert("random_pick_count", &1usize);
    context.insert("random_week_label", "May 19 – May 25, 2026");
    context.insert("random_eligible_count", &1548usize);
    context.insert("random_pick_target", &3usize);
    context.insert("random_can_shuffle", &true);
    context.insert("random_fewer_than_target", &false);
    context.insert("random_shuffled", &false);
    context.insert("random_just_shuffled", &false);
    context.insert("random_category_saved", &false);
    context.insert(
        "random_picks",
        &serde_json::json!([{
            "uid": "test-uid",
            "full_name": "Test User",
            "org": "Acme",
            "email": "test@example.com",
            "phone": "",
            "location": "Boston, MA",
            "reconnect_tag": "",
            "categories": "Reconnect: 1 month",
            "note": "Met at a conference.",
            "linkedin_url": "",
        }]),
    );
    context.insert(
        "random_pick_category_options",
        &serde_json::json!([
            {"value": "1 week", "label": "Reconnect: 1 week"},
            {"value": "Do Not Engage", "label": "Do Not Engage"},
        ]),
    );
    context.insert("travel_search_location", "Chicago, IL");
    context.insert("travel_search_near", "");
    context.insert("travel_ready", &false);
    context.insert("travel_trips", &Vec::<serde_json::Value>::new());
    context.insert("travel_match_count", &0usize);
    context.insert("travel_built_at", "");
    context.insert("travel_week_label", "");
    context.insert("travel_error", &Option::<String>::None);
    context.insert("metro_radius_mi", &50u32);
    context.insert("metro_radius_built", &false);
    context.insert("has_calendar_ics", &false);
    context.insert("metro_radius_min", &5u32);
    context.insert("metro_radius_max", &200u32);
    context.insert("reconnect_snooze_options", &Vec::<(String, String)>::new());
    context.insert("travel_snoozed", &false);
    context.insert("travel_just_refreshed", &false);
    context.insert("travel_refresh_error", &false);
    context.insert("geo_just_finished", &false);
    context.insert("geo_refresh_error", &false);
    context.insert("enrichment_geo_failed", &false);
    context.insert("contacts_read_only", &false);
    context.insert("geo_geocoded_count", &0usize);
    context.insert("geo_failed_count", &0usize);
    context.insert("geo_with_coords", &0usize);
    context.insert("geo_with_address", &0usize);
    context.insert("birthdays", &Vec::<serde_json::Value>::new());
    context.insert("birthday_count", &0usize);
    context.insert("birthday_window_days", &14u32);
    context.insert("enrichment_picks", &Vec::<serde_json::Value>::new());
    context.insert("enrichment_pick_count", &0usize);
    context.insert("enrichment_eligible_count", &0usize);
    context.insert("enrichment_pick_target", &3usize);
    context.insert("enrichment_fewer_than_target", &false);
    context.insert("enrichment_location_saved", &false);
    context.insert("enrichment_geo_ok", &false);
    context.insert("enrichment_geo_failed", &false);

    let html = tera.render("dashboard.html", &context).expect("render");
    assert!(html.contains("Random People of the Week"));
    assert!(html.contains("Categories"));
    assert!(html.contains("Reconnect: 1 month"));
    assert!(html.contains("Met at a conference."));
    assert!(html.contains("action=\"/random/category\""));
    assert!(!html.contains("Add notes"));
    assert!(!html.contains("Add location"));
    assert!(!html.contains("Suggested actions"));
}
