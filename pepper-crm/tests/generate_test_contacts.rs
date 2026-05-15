use chrono::{Duration, Local};
use fake::faker::address::en::{CityName, CountryName, StateAbbr, ZipCode};
use fake::faker::company::en::CompanyName;
use fake::faker::internet::en::SafeEmail;
use fake::faker::phone_number::en::PhoneNumber;
use fake::Fake;
use std::fs;
use std::path::Path;
use uuid::Uuid;

#[test]
#[ignore]
fn generate_test_contacts() {
    let contacts_dir = Path::new("../contacts");
    fs::create_dir_all(contacts_dir).expect("Failed to create contacts directory");

    let contacts = vec![
        // Scenario 1: No tags at all (3 contacts)
        generate_contact_no_tags("Emma Wilson"),
        generate_contact_no_tags("James Martinez"),
        generate_contact_no_tags("Sofia Chen"),
        
        // Scenario 2: TODO only, no reconnect (3 contacts)
        generate_contact_todos_only("Liam Anderson", vec![
            "send intro to team",
            "share grant proposal template",
        ]),
        generate_contact_todos_only("Olivia Brown", vec![
            "follow up on collaboration",
        ]),
        generate_contact_todos_only("Noah Taylor", vec![
            "send meeting notes",
            "schedule call",
        ]),
        
        // Scenario 3: Reconnect only, due this week (3 contacts)
        generate_contact_reconnect_soon("Ava Johnson", "3 days"),
        generate_contact_reconnect_soon("Ethan Davis", "5 days"),
        generate_contact_reconnect_soon("Isabella Moore", "1 week"),
        
        // Scenario 4: Multiple TODOs + Reconnect (3 contacts)
        generate_contact_full("Mason Garcia", 
            vec!["intro to ZK team", "send ZCG grant template"],
            "3 months",
        ),
        generate_contact_full("Charlotte Rodriguez", 
            vec!["share research paper", "schedule demo"],
            "2 months",
        ),
        generate_contact_full("Lucas Martinez", 
            vec!["follow up on proposal"],
            "6 weeks",
        ),
        
        // Scenario 5: City trigger (deferred) (2 contacts)
        generate_contact_city_trigger("Amelia Wilson", "before NY trip"),
        generate_contact_city_trigger("Benjamin Thomas", "before Berlin trip"),
        
        // Scenario 6: Already has CRM Log block (2 contacts)
        generate_contact_with_log("Mia Jackson", vec![
            "2026-04-15: Sent intro to research team. Reset to 3 months.",
            "2026-03-10: Initial meeting at ETHDenver.",
        ]),
        generate_contact_with_log("Elijah White", vec![
            "2026-05-01: Follow-up call about grant application.",
        ]),
        
        // Scenario 7: Overdue reconnect (2 contacts)
        generate_contact_overdue("Harper Harris", "-2 weeks"),
        generate_contact_overdue("Alexander Martin", "-10 days"),
        
        // Scenario 8: Incomplete records (2 contacts)
        generate_contact_incomplete("Evelyn Thompson"),
        generate_contact_incomplete("William Lee"),
    ];

    for (i, contact) in contacts.iter().enumerate() {
        let filename = format!("contact_{:02}.vcf", i + 1);
        let filepath = contacts_dir.join(filename);
        fs::write(filepath, contact).expect("Failed to write VCF file");
    }

    println!("✓ Generated {} test contacts in ./contacts/", contacts.len());
}

fn generate_contact_no_tags(name: &str) -> String {
    let uid = Uuid::new_v4().to_string();
    let email: String = SafeEmail().fake();
    let phone: String = PhoneNumber().fake();
    let org: String = CompanyName().fake();
    let city: String = CityName().fake();
    let state: String = StateAbbr().fake();
    let zip: String = ZipCode().fake();
    let country: String = CountryName().fake();

    format!(
        r#"BEGIN:VCARD
VERSION:3.0
UID:{}
FN:{}
EMAIL;TYPE=INTERNET:{}
TEL;TYPE=CELL:{}
ORG:{}
ADR;TYPE=HOME:;;123 Main St;{};{};{};{}
NOTE:Met at a networking event. Works in distributed systems.
END:VCARD"#,
        uid, name, email, phone, org, city, state, zip, country
    )
}

fn generate_contact_todos_only(name: &str, todos: Vec<&str>) -> String {
    let uid = Uuid::new_v4().to_string();
    let email: String = SafeEmail().fake();
    let phone: String = PhoneNumber().fake();
    let org: String = CompanyName().fake();
    let city: String = CityName().fake();
    let country: String = CountryName().fake();

    let todo_lines = todos
        .iter()
        .map(|todo| format!(" TODO: {}", todo))  // Add leading space for vCard folding
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"BEGIN:VCARD
VERSION:3.0
UID:{}
FN:{}
EMAIL;TYPE=INTERNET:{}
TEL;TYPE=CELL:{}
ORG:{}
ADR;TYPE=HOME:;;;{};CA;94000;{}
NOTE:Active collaborator on crypto research.
{}
END:VCARD"#,
        uid, name, email, phone, org, city, country, todo_lines
    )
}

fn generate_contact_reconnect_soon(name: &str, reconnect_tag: &str) -> String {
    let uid = Uuid::new_v4().to_string();
    let email: String = SafeEmail().fake();
    let phone: String = PhoneNumber().fake();
    let org: String = CompanyName().fake();
    let city: String = CityName().fake();

    format!(
        r#"BEGIN:VCARD
VERSION:3.0
UID:{}
FN:{}
EMAIL;TYPE=INTERNET:{}
TEL;TYPE=CELL:{}
ORG:{}
ADR;TYPE=HOME:;;;{};NY;10001;USA
NOTE:Met at conference. Interested in zero-knowledge proofs.
 Reconnect: {}
END:VCARD"#,
        uid, name, email, phone, org, city, reconnect_tag
    )
}

fn generate_contact_full(name: &str, todos: Vec<&str>, reconnect_tag: &str) -> String {
    let uid = Uuid::new_v4().to_string();
    let email: String = SafeEmail().fake();
    let phone: String = PhoneNumber().fake();
    let org: String = CompanyName().fake();
    let city: String = CityName().fake();

    let todo_lines = todos
        .iter()
        .map(|todo| format!(" TODO: {}", todo))  // Add leading space for vCard folding
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"BEGIN:VCARD
VERSION:3.0
UID:{}
FN:{}
EMAIL;TYPE=INTERNET:{}
TEL;TYPE=CELL:{}
ORG:{}
ADR;TYPE=HOME:;;;{};CA;94102;USA
NOTE:Met at Consensus Miami. Works on ZK proofs at Aztec.
{}
 Reconnect: {}
END:VCARD"#,
        uid, name, email, phone, org, city, todo_lines, reconnect_tag
    )
}

fn generate_contact_city_trigger(name: &str, trigger: &str) -> String {
    let uid = Uuid::new_v4().to_string();
    let email: String = SafeEmail().fake();
    let phone: String = PhoneNumber().fake();
    let org: String = CompanyName().fake();

    format!(
        r#"BEGIN:VCARD
VERSION:3.0
UID:{}
FN:{}
EMAIL;TYPE=INTERNET:{}
TEL;TYPE=CELL:{}
ORG:{}
NOTE:Based in the city. Should meet up when I'm in town.
 Reconnect: {}
END:VCARD"#,
        uid, name, email, phone, org, trigger
    )
}

fn generate_contact_with_log(name: &str, log_entries: Vec<&str>) -> String {
    let uid = Uuid::new_v4().to_string();
    let email: String = SafeEmail().fake();
    let phone: String = PhoneNumber().fake();
    let org: String = CompanyName().fake();
    let city: String = CityName().fake();

    let log_block = log_entries
        .iter()
        .map(|entry| format!(" {}", entry))  // Add leading space for vCard folding
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"BEGIN:VCARD
VERSION:3.0
UID:{}
FN:{}
EMAIL;TYPE=INTERNET:{}
TEL;TYPE=CELL:{}
ORG:{}
ADR;TYPE=HOME:;;;{};MA;02101;USA
NOTE:Long-term collaborator on grant proposals.
 TODO: review latest draft
 Reconnect: 2 months
 --- CRM Log ---
{}
 Reconnect: 4 months
END:VCARD"#,
        uid, name, email, phone, org, city, log_block
    )
}

fn generate_contact_overdue(name: &str, time_offset: &str) -> String {
    let uid = Uuid::new_v4().to_string();
    let email: String = SafeEmail().fake();
    let phone: String = PhoneNumber().fake();
    let org: String = CompanyName().fake();
    
    // Calculate a date in the past
    let past_date = if time_offset.contains("weeks") {
        let weeks: i64 = time_offset
            .replace("-", "")
            .replace("weeks", "")
            .replace("week", "")
            .trim()
            .parse()
            .unwrap_or(2);
        Local::now() - Duration::weeks(weeks)
    } else {
        let days: i64 = time_offset
            .replace("-", "")
            .replace("days", "")
            .replace("day", "")
            .trim()
            .parse()
            .unwrap_or(10);
        Local::now() - Duration::days(days)
    };
    
    let log_date = past_date.format("%Y-%m-%d");

    format!(
        r#"BEGIN:VCARD
VERSION:3.0
UID:{}
FN:{}
EMAIL;TYPE=INTERNET:{}
TEL;TYPE=CELL:{}
ORG:{}
NOTE:Important contact - needs follow-up.
 TODO: schedule catch-up call
 --- CRM Log ---
 {}: Last connected at conference. Set reminder to reconnect.
 Reconnect: 2 weeks
END:VCARD"#,
        uid, name, email, phone, org, log_date
    )
}

fn generate_contact_incomplete(name: &str) -> String {
    let uid = Uuid::new_v4().to_string();
    
    format!(
        r#"BEGIN:VCARD
VERSION:3.0
UID:{}
FN:{}
NOTE:Brief encounter. Need to get more contact details.
 TODO: request email and phone
 Reconnect: 1 month
END:VCARD"#,
        uid, name
    )
}
