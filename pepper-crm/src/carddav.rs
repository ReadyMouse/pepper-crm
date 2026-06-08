//! # CardDAV Client
//!
//!   Reads and writes vCards via Radicale (or any CardDAV server) using HTTP Basic auth.
//!
//! INPUT: `CARDDAV_URL`, `CARDDAV_USER`, `CARDDAV_PASS` (optional `CARDDAV_INSECURE=true`).
//! OUTPUT: vCard bodies from `addressbook-query` REPORT; PUT for write-back.
//!
//! NOTES:
//!   - When env vars are set, `parse_contacts` uses this instead of scanning `CONTACTS_DIR`.
//!   - Blocking reqwest client — vCard I/O runs on pepper-web's blocking thread pool.
//!
//! Written by Cursor for Ready Mouse and Pepper CRM. May 2026. All rights reserved.

use anyhow::{Context, Result};
use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use reqwest::Url;
use std::time::Duration;
use tracing::debug;

const ADDRESSBOOK_QUERY: &str = r#"<?xml version="1.0" encoding="utf-8" ?>
<C:addressbook-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:prop>
    <C:address-data/>
  </D:prop>
</C:addressbook-query>"#;

const PROPFIND_BODY: &str = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:getetag/>
    <D:getcontenttype/>
  </D:prop>
</D:propfind>"#;

/// CardDAV collection URL and credentials from the environment.
#[derive(Debug, Clone)]
pub struct CardDavConfig {
    pub collection_url: String,
    pub user: String,
    pub pass: String,
    pub insecure: bool,
}

/// HTTP client for one CardDAV address book collection.
pub struct CardDavClient {
    config: CardDavConfig,
    http: Client,
    collection_path: String,
}

impl CardDavConfig {
    /// Load config when `CARDDAV_URL`, `CARDDAV_USER`, and `CARDDAV_PASS` are all set.
    pub fn from_env() -> Option<Self> {
        let url = std::env::var("CARDDAV_URL").ok()?;
        let user = std::env::var("CARDDAV_USER").ok()?;
        let pass = std::env::var("CARDDAV_PASS").ok()?;
        if url.trim().is_empty() || user.is_empty() {
            return None;
        }
        let insecure = std::env::var("CARDDAV_INSECURE")
            .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);
        Some(Self {
            collection_url: normalize_collection_url(&url),
            user,
            pass,
            insecure,
        })
    }
}

impl CardDavClient {
    pub fn from_env() -> Result<Option<Self>> {
        Ok(CardDavConfig::from_env().map(|config| Self::new(config)))
    }

    pub fn new(config: CardDavConfig) -> Self {
        let collection_path = Url::parse(&config.collection_url)
            .map(|u| u.path().trim_end_matches('/').to_string())
            .unwrap_or_default();

        let mut builder = Client::builder()
            .timeout(Duration::from_secs(120))
            .user_agent("pepper-crm/1.0 (CardDAV)");
        if config.insecure {
            builder = builder.danger_accept_invalid_certs(true);
        }
        let http = builder
            .build()
            .expect("CardDAV HTTP client should build");

        Self {
            config,
            http,
            collection_path,
        }
    }

    pub fn collection_url(&self) -> &str {
        &self.config.collection_url
    }

    /// Fetch all vCards in the collection (REPORT, with PROPFIND+GET fallback).
    pub fn fetch_all_vcards(&self) -> Result<Vec<(String, String)>> {
        match self.fetch_via_report() {
            Ok(vcards) if !vcards.is_empty() => Ok(vcards),
            Ok(_) => self.fetch_via_propfind_get(),
            Err(e) => {
                debug!("CardDAV REPORT failed ({e}); trying PROPFIND+GET");
                self.fetch_via_propfind_get()
            }
        }
    }

    pub fn get_resource(&self, href: &str) -> Result<String> {
        let url = self.absolute_url(href)?;
        self.http
            .get(url)
            .headers(self.auth_headers())
            .send()
            .with_context(|| format!("CardDAV GET failed for {href}"))?
            .error_for_status()
            .with_context(|| format!("CardDAV GET returned error for {href}"))?
            .text()
            .with_context(|| format!("CardDAV GET body read failed for {href}"))
    }

    pub fn put_resource(&self, href: &str, body: &str) -> Result<()> {
        let url = self.absolute_url(href)?;
        let body = vcard_to_crlf(body);
        let response = self
            .http
            .put(url)
            .headers(self.auth_headers())
            .header(CONTENT_TYPE, "text/vcard; charset=utf-8")
            .body(body.clone())
            .send()
            .with_context(|| format!("CardDAV PUT failed for {href}"))?;

        if response.status().is_success() {
            return Ok(());
        }

        let status = response.status();
        let err_text = response.text().unwrap_or_default();
        if self.put_persisted_despite_error(href, &body)? {
            tracing::warn!(
                href,
                status = %status,
                "CardDAV PUT returned error but GET confirms vCard was stored"
            );
            return Ok(());
        }

        anyhow::bail!(
            "CardDAV PUT HTTP {status} for {href}: {}",
            err_text.trim()
        );
    }

    /// FreedomBox/Radicale sometimes returns 4xx/5xx on PUT even when the vCard was saved.
    fn put_persisted_despite_error(&self, href: &str, expected: &str) -> Result<bool> {
        for attempt in 0..4 {
            if attempt > 0 {
                std::thread::sleep(Duration::from_millis(75 * attempt as u64));
            }
            let stored = match self.get_resource(href) {
                Ok(content) => content,
                Err(e) => {
                    tracing::debug!(href, attempt, error = %e, "CardDAV GET after failed PUT");
                    continue;
                }
            };
            if vcard_body_contains_expected(&stored, expected) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// PUT URL for a contact — uses stored href or `{collection}/{uid}.vcf`.
    pub fn put_url_for_contact(&self, carddav_href: Option<&str>, uid: &str) -> Result<String> {
        if let Some(href) = carddav_href {
            return self.absolute_url(href);
        }
        let filename = format!("{uid}.vcf");
        let base = self.config.collection_url.trim_end_matches('/');
        Ok(format!("{base}/{filename}"))
    }

    fn fetch_via_report(&self) -> Result<Vec<(String, String)>> {
        let response = self
            .http
            .request(
                reqwest::Method::from_bytes(b"REPORT").expect("REPORT"),
                self.config.collection_url.clone(),
            )
            .headers(self.auth_headers())
            .header("Depth", "1")
            .header(CONTENT_TYPE, "application/xml; charset=utf-8")
            .body(ADDRESSBOOK_QUERY.to_string())
            .send()
            .context("CardDAV addressbook-query REPORT request failed")?
            .error_for_status()
            .context("CardDAV addressbook-query REPORT returned error")?
            .text()
            .context("CardDAV REPORT response body read failed")?;

        parse_addressbook_multistatus(&response)
    }

    fn fetch_via_propfind_get(&self) -> Result<Vec<(String, String)>> {
        let response = self
            .http
            .request(
                reqwest::Method::from_bytes(b"PROPFIND").expect("PROPFIND"),
                self.config.collection_url.clone(),
            )
            .headers(self.auth_headers())
            .header("Depth", "1")
            .header(CONTENT_TYPE, "application/xml; charset=utf-8")
            .body(PROPFIND_BODY.to_string())
            .send()
            .context("CardDAV PROPFIND request failed")?
            .error_for_status()
            .context("CardDAV PROPFIND returned error")?
            .text()
            .context("CardDAV PROPFIND response body read failed")?;

        let hrefs = parse_propfind_hrefs(&response, &self.collection_path)?;
        let mut out = Vec::with_capacity(hrefs.len());
        for href in hrefs {
            let content = self.get_resource(&href)?;
            if content.contains("BEGIN:VCARD") {
                out.push((href, content));
            }
        }
        debug!("CardDAV PROPFIND+GET fetched {} vCards", out.len());
        Ok(out)
    }

    fn auth_headers(&self) -> reqwest::header::HeaderMap {
        use base64::Engine;
        let token = base64::engine::general_purpose::STANDARD
            .encode(format!("{}:{}", self.config.user, self.config.pass));
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            format!("Basic {token}")
                .parse()
                .expect("Basic auth header should parse"),
        );
        headers
    }

    fn absolute_url(&self, href: &str) -> Result<String> {
        resolve_carddav_href(&self.config.collection_url, href)
    }
}

/// Normalize vCard text to CRLF for CardDAV PUT (Radicale stores CRLF).
fn vcard_to_crlf(body: &str) -> String {
    body.replace("\r\n", "\n").replace('\n', "\r\n")
}

fn vcard_property_lines(s: &str) -> std::collections::BTreeSet<String> {
    s.replace("\r\n", "\n")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// True when every property line we PUT is present after read-back (Radicale may reorder).
fn vcard_body_contains_expected(stored: &str, expected: &str) -> bool {
    let stored_lines = vcard_property_lines(stored);
    let expected_lines: Vec<String> = expected
        .replace("\r\n", "\n")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && is_vcard_property_line(line))
        .map(ToString::to_string)
        .collect();

    for line in &expected_lines {
        let key = line.split(':').next().unwrap_or("");
        if key == "NOTE" || key.starts_with("NOTE;") {
            continue;
        }
        if key == "REV" || key.starts_with("REV;") {
            if !stored_lines.iter().any(|s| s.starts_with("REV")) {
                return false;
            }
            continue;
        }
        if !vcard_line_present_in_stored(&stored_lines, line) {
            return false;
        }
    }

    match (unfolded_note_text(expected), unfolded_note_text(stored)) {
        (Some(expected_note), Some(stored_note)) => {
            normalize_note_for_compare(&expected_note) == normalize_note_for_compare(&stored_note)
        }
        (None, None) => true,
        _ => false,
    }
}

fn unfolded_note_text(content: &str) -> Option<String> {
    let mut note = String::new();
    let mut in_note = false;
    for line in content.replace("\r\n", "\n").lines() {
        if let Some(value) = line.strip_prefix("NOTE:") {
            note.push_str(value);
            in_note = true;
        } else if in_note && (line.starts_with(' ') || line.starts_with('\t')) {
            note.push_str(line.trim_start());
        } else {
            in_note = false;
        }
    }
    if note.is_empty() {
        None
    } else {
        Some(note)
    }
}

fn normalize_note_for_compare(note: &str) -> String {
    note.chars().filter(|c| !c.is_whitespace()).collect()
}

fn is_vcard_property_line(line: &str) -> bool {
    let key = line.split(':').next().unwrap_or("").trim();
    !key.is_empty()
        && key.bytes().all(|b| b.is_ascii_uppercase() || b == b'-' || b == b';')
}

fn normalize_vcard_line_for_compare(line: &str) -> String {
    line.replace("\\;", ";").replace("\\,", ",")
}

fn vcard_property_key_value(line: &str) -> Option<(&str, String)> {
    let (key, value) = line.split_once(':')?;
    Some((key.trim(), normalize_vcard_line_for_compare(value.trim())))
}

fn geo_property_points_match(expected_line: &str, stored_line: &str) -> bool {
    fn parse_geo_coords(line: &str) -> Option<(f64, f64)> {
        let (_, value) = vcard_property_key_value(line)?;
        let coords = value.strip_prefix("geo:").unwrap_or(&value);
        let sep = if coords.contains(';') { ';' } else { ',' };
        let mut parts = coords.split(sep);
        let lat: f64 = parts.next()?.trim().parse().ok()?;
        let lng: f64 = parts.next()?.trim().parse().ok()?;
        Some((lat, lng))
    }
    match (
        parse_geo_coords(expected_line),
        parse_geo_coords(stored_line),
    ) {
        (Some((elat, elng)), Some((slat, slng))) => {
            (elat - slat).abs() < 1e-6 && (elng - slng).abs() < 1e-6
        }
        _ => false,
    }
}

fn vcard_line_present_in_stored(
    stored_lines: &std::collections::BTreeSet<String>,
    expected_line: &str,
) -> bool {
    let expected_norm = normalize_vcard_line_for_compare(expected_line);
    if stored_lines
        .iter()
        .any(|stored| normalize_vcard_line_for_compare(stored) == expected_norm)
    {
        return true;
    }
    let key = expected_norm.split(':').next().unwrap_or("");
    if key == "GEO" || key.starts_with("GEO;") {
        return stored_lines
            .iter()
            .any(|stored| geo_property_points_match(&expected_norm, stored));
    }
    if key.starts_with("X-PEPPER-GEO-SOURCE") {
        let Some((_, expected_val)) = vcard_property_key_value(&expected_norm) else {
            return false;
        };
        return stored_lines.iter().any(|stored| {
            vcard_property_key_value(stored)
                .filter(|(k, _)| k.starts_with("X-PEPPER-GEO-SOURCE"))
                .map(|(_, v)| crate::geo::geo_source_covers_query(&v, &expected_val))
                .unwrap_or(false)
        });
    }
    false
}

/// Resolve a CardDAV href from REPORT/PROPFIND or a relative filename against `CARDDAV_URL`.
fn resolve_carddav_href(collection_url: &str, href: &str) -> Result<String> {
    if href.starts_with("http://") || href.starts_with("https://") {
        return Ok(href.to_string());
    }
    let base = Url::parse(collection_url).context("Invalid CARDDAV_URL")?;
    // FreedomBox/Radicale hrefs are server-root paths (`/radicale/.../uid.vcf`). Do not strip
    // the leading slash — that would duplicate path segments when joined to the collection URL.
    base.join(href)
        .map(|u| u.to_string())
        .with_context(|| format!("Could not resolve CardDAV href {href}"))
}

fn normalize_collection_url(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.ends_with('/') {
        trimmed.to_string()
    } else {
        format!("{trimmed}/")
    }
}

/// Parse CardDAV multistatus XML from addressbook-query (href + vCard text).
pub fn parse_addressbook_multistatus(xml: &str) -> Result<Vec<(String, String)>> {
    use quick_xml::events::Event;

    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut results = Vec::new();
    let mut in_response = false;
    let mut current_href: Option<String> = None;
    let mut in_href = false;
    let mut in_address_data = false;
    let mut address_data = String::new();
    let mut href_buf = String::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.local_name().as_ref() {
                b"response" => {
                    in_response = true;
                    current_href = None;
                    address_data.clear();
                }
                b"href" if in_response => {
                    in_href = true;
                    href_buf.clear();
                }
                b"address-data" if in_response => {
                    in_address_data = true;
                    address_data.clear();
                }
                _ => {}
            },
            Ok(Event::Text(e)) => {
                let text = e.unescape().unwrap_or_default().into_owned();
                if in_href {
                    href_buf.push_str(&text);
                } else if in_address_data {
                    address_data.push_str(&text);
                }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"href" if in_href => {
                    in_href = false;
                    current_href = Some(href_buf.trim().to_string());
                }
                b"address-data" => {
                    in_address_data = false;
                    if let Some(href) = current_href.clone() {
                        let body = address_data.trim().to_string();
                        if body.contains("BEGIN:VCARD") {
                            results.push((href, body));
                        }
                    }
                }
                b"response" => {
                    in_response = false;
                    current_href = None;
                    address_data.clear();
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(e) => anyhow::bail!("CardDAV XML parse error: {e}"),
            _ => {}
        }
        buf.clear();
    }

    if results.is_empty() {
        results = parse_addressbook_multistatus_naive(xml);
    }

    debug!("CardDAV REPORT parsed {} vCards", results.len());
    Ok(results)
}

fn parse_addressbook_multistatus_naive(xml: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for response in xml.split("<D:response").skip(1) {
        let href = extract_xml_text(response, "href");
        let data = extract_xml_text(response, "address-data");
        if let (Some(h), Some(d)) = (href, data) {
            let body = decode_xml_text(&d);
            if body.contains("BEGIN:VCARD") {
                out.push((h, body));
            }
        }
    }
    out
}

fn parse_propfind_hrefs(xml: &str, collection_path: &str) -> Result<Vec<String>> {
    let collection_path = collection_path.trim_end_matches('/');
    let mut hrefs = Vec::new();

    for response in xml.split("<D:response").skip(1) {
        let Some(href) = extract_xml_text(response, "href") else {
            continue;
        };
        let href = href.trim().to_string();
        if href.ends_with('/') {
            continue;
        }
        if href == collection_path || href.trim_end_matches('/') == collection_path {
            continue;
        }
        if href.contains("BEGIN:VCARD") {
            continue;
        }
        hrefs.push(href);
    }

    if hrefs.is_empty() {
        // Some servers use lowercase dav: prefix
        for response in xml.split("<d:response").skip(1) {
            let Some(href) = extract_xml_text(response, "href") else {
                continue;
            };
            let href = href.trim().to_string();
            if href.ends_with('/') {
                continue;
            }
            if href.trim_end_matches('/') == collection_path {
                continue;
            }
            hrefs.push(href);
        }
    }

    Ok(hrefs)
}

fn extract_xml_text(fragment: &str, local_name: &str) -> Option<String> {
    for prefix in ["D:", "C:", "d:", "c:"] {
        let open = format!("<{prefix}{local_name}");
        if let Some(start) = fragment.find(&open) {
            let rest = &fragment[start..];
            let content_start = rest.find('>')? + 1;
            let close = format!("</{prefix}{local_name}>");
            let content_end = rest.find(&close)?;
            return Some(rest[content_start..content_end].to_string());
        }
    }
    None
}

fn decode_xml_text(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_REPORT: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:response>
    <D:href>/alice/contacts/</D:href>
  </D:response>
  <D:response>
    <D:href>/alice/contacts/test-uid.vcf</D:href>
    <D:propstat>
      <D:prop>
        <C:address-data>BEGIN:VCARD
VERSION:3.0
FN:Ada Lovelace
UID:test-uid
END:VCARD</C:address-data>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

    #[test]
    fn parse_report_extracts_href_and_vcard() {
        let pairs = parse_addressbook_multistatus(SAMPLE_REPORT).unwrap();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "/alice/contacts/test-uid.vcf");
        assert!(pairs[0].1.contains("FN:Ada Lovelace"));
    }

    #[test]
    fn parse_propfind_skips_collection_href() {
        let xml = r#"<D:multistatus xmlns:D="DAV:">
  <D:response><D:href>/alice/contacts/</D:href></D:response>
  <D:response><D:href>/alice/contacts/a.vcf</D:href></D:response>
</D:multistatus>"#;
        let hrefs = parse_propfind_hrefs(xml, "/alice/contacts").unwrap();
        assert_eq!(hrefs, vec!["/alice/contacts/a.vcf"]);
    }

    #[test]
    fn normalize_collection_url_adds_trailing_slash() {
        assert_eq!(
            normalize_collection_url("https://pi.example/contacts"),
            "https://pi.example/contacts/"
        );
    }

    #[test]
    fn resolve_carddav_href_server_root_path() {
        let collection = "https://localhost:8443/radicale/admin/test-contacts/";
        let href = "/radicale/admin/test-contacts/2f1571de-a405-4be7-b497-f150813a53aa.vcf";
        assert_eq!(
            resolve_carddav_href(collection, href).unwrap(),
            "https://localhost:8443/radicale/admin/test-contacts/2f1571de-a405-4be7-b497-f150813a53aa.vcf"
        );
    }

    #[test]
    fn resolve_carddav_href_relative_filename() {
        let collection = "https://localhost:8443/radicale/admin/test-contacts/";
        assert_eq!(
            resolve_carddav_href(collection, "test-contact.vcf").unwrap(),
            "https://localhost:8443/radicale/admin/test-contacts/test-contact.vcf"
        );
    }

    #[test]
    fn vcard_to_crlf_normalizes_mixed_endings() {
        assert_eq!(
            vcard_to_crlf("BEGIN:VCARD\nFN:Ada\r\nEND:VCARD\n"),
            "BEGIN:VCARD\r\nFN:Ada\r\nEND:VCARD\r\n"
        );
    }

    #[test]
    fn vcard_body_contains_expected_ignores_order_and_endings() {
        let expected = "BEGIN:VCARD\nFN:Ada\nUID:1\nADR;TYPE=HOME:;;;Austin;TX;;\nEND:VCARD\n";
        let stored = "BEGIN:VCARD\r\nUID:1\r\nADR;TYPE=HOME:;;;Austin;TX;;\r\nFN:Ada\r\nEND:VCARD\r\n";
        assert!(vcard_body_contains_expected(stored, expected));
    }

    #[test]
    fn vcard_body_contains_expected_matches_radicale_folded_note() {
        let expected = "BEGIN:VCARD\nUID:1\nNOTE:May 2025: Met at event.\nCATEGORIES:Reconnect: 3 months\nREV:20260608T120000Z\nEND:VCARD\n";
        let stored = "BEGIN:VCARD\r\nUID:1\r\nNOTE:May 2025: Met at event.\r\nCATEGORIES:Reconnect: 3 months\r\nREV:20260608T120001Z\r\nEND:VCARD\r\n";
        assert!(vcard_body_contains_expected(stored, expected));
    }

    #[test]
    fn vcard_body_contains_expected_matches_radicale_geo_escaping() {
        let expected = "BEGIN:VCARD\nUID:1\nGEO:41.8755616;-87.6244212\nX-PEPPER-GEO-SOURCE:chicago\nEND:VCARD\n";
        let stored = "BEGIN:VCARD\r\nUID:1\r\nGEO:41.8755616\\;-87.6244212\r\nX-PEPPER-GEO-SOURCE:chicago\r\nEND:VCARD\r\n";
        assert!(vcard_body_contains_expected(stored, expected));
    }
}
