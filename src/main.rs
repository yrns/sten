#![allow(unused)]

use std::path::PathBuf;

use reqwest::header;
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
struct Status {
    status: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ChangeType {
    File,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Action {
    Deleted,
    Modified,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "data")]
enum EventData {
    RemoteChangeDetected(RemoteChangeDetected),
}

#[derive(Debug, Deserialize)]
struct RemoteChangeDetected {
    #[serde(rename = "type")]
    change_type: ChangeType,
    action: Action,
    // folderID is deprecated.
    #[serde(skip, rename = "folderID")]
    folder_id: (),
    folder: String,
    path: PathBuf,
    label: String,
    #[serde(rename = "modifiedBy")]
    modified_by: String,
}

#[derive(Debug, Deserialize)]
struct Event {
    id: usize,
    #[serde(flatten)]
    data: EventData,
    #[serde(rename = "globalID")]
    global_id: usize,
    time: String, // time? "2017-03-06T23:58:21.844739891+01:00",
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let mut headers = header::HeaderMap::new();
    // TODO clap
    let mut api_key = header::HeaderValue::from_str(&std::env::var("STEN_KEY")?)?;
    api_key.set_sensitive(true);
    headers.insert("X-API-Key", api_key);

    let client = reqwest::ClientBuilder::new()
        .default_headers(headers)
        .build()?;

    // TODO clap with default port
    let mut url = reqwest::Url::parse("http://localhost:8384")?;
    url.set_path("rest/noauth/health");

    let ok: Status = client.get(url.clone()).send().await?.json().await?;

    println!("{:#?}", ok.status);

    let mut last_id = 0;
    url.set_path("/rest/events");
    url.query_pairs_mut()
        .clear()
        .extend_pairs(&[("events", "RemoteChangeDetected")]);

    loop {
        let mut url = url.clone();
        url.query_pairs_mut()
            //.clear()
            .extend_pairs(&[("since", last_id.to_string())]);

        let events: Vec<Event> = client.get(url).send().await?.json().await?;

        for e in events {
            println!("{:#?}", e);
            last_id = e.id;
        }
    }

    Ok(())
}
