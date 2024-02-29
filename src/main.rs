#![allow(unused)]

use std::{
    collections::{hash_map::Entry, HashMap},
    path::PathBuf,
};

use notify_rust::{Hint, Notification};
use reqwest::header;
use serde::Deserialize;
use serde_json::json;
use strum::Display;

#[derive(Debug, Deserialize)]
struct Health(Status);

#[derive(Debug, Deserialize)]
#[serde(tag = "status")]
enum Status {
    OK,
}

#[derive(Debug, Display, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ChangeType {
    File,
}

#[derive(Debug, Display, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Action {
    Deleted,
    Modified,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "data")]
enum EventData {
    RemoteChangeDetected(RemoteChangeDetected),
    StateChanged(StateChanged),
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

impl std::fmt::Display for RemoteChangeDetected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {}: {}",
            self.change_type,
            self.action,
            self.path.display(),
            //self.modified_by
        )
    }
}

#[derive(Debug, Deserialize)]
struct StateChanged {
    folder: PathBuf,
    from: String,
    duration: f64,
    to: String,
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

// Ignore the rest.
#[derive(Debug, Deserialize)]
struct Folder {
    path: PathBuf,
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
    let base = reqwest::Url::parse("http://localhost:8384")?;

    let mut url = base.clone();
    url.set_path("rest/noauth/health");

    let ok: Health = client.get(url).send().await?.json().await?;
    println!("{:?}", ok);

    let mut url = base.clone();
    url.set_path("/rest/events");
    url.query_pairs_mut()
        .clear()
        .extend_pairs(&[("events", "RemoteChangeDetected")]); // ,StateChanged

    // Fetch the last id, so that we can catch up, by setting the limit to 1.
    let mut last_id = {
        let mut url = url.clone();
        url.query_pairs_mut().extend_pairs(&[
            ("since", 0.to_string()),
            ("limit", 1.to_string()),
            ("timeout", 1.to_string()),
        ]);

        let mut events: Vec<Event> = client.get(url).send().await?.json().await?;
        let last = events.pop();
        println!("last event: {last:?}");
        last.map(|e| e.id).unwrap_or(0)
    };

    // Flush on ConfigSaved? https://docs.syncthing.net/events/configsaved.html
    let mut folder_paths: HashMap<String, PathBuf> = HashMap::new();

    loop {
        let mut url = url.clone();
        url.query_pairs_mut()
            //.clear()
            .extend_pairs(&[
                ("since", last_id.to_string()),
                //("limit", 1.to_string())
            ]);

        let events: Vec<Event> = client.get(url).send().await?.json().await?;

        for e in events {
            match e.data {
                EventData::RemoteChangeDetected(mut data) => {
                    // Lookup local folder path.
                    let mut path: PathBuf = match folder_paths.entry(data.folder.clone()) {
                        Entry::Occupied(e) => e.get().clone(),
                        Entry::Vacant(e) => {
                            // Get local path from the folder id.
                            let mut url = base.clone();
                            url.set_path(&format!("/rest/config/folders/{}", data.folder));
                            let f: Folder = client.get(url).send().await?.json().await?;
                            println!("folder for id {:?}: {:#?}", data.folder, f.path);
                            e.insert(f.path).clone()
                        }
                    };

                    // TODO relative to HOME, Path::strip_prefix
                    path.push(&data.path);
                    data.path = path;

                    match Notification::new()
                        .summary("sten")
                        .body(&data.to_string())
                        .icon("document-open")
                        .action("default", "default")
                        .show()
                    {
                        Ok(n) => n.wait_for_action(|action| match action {
                            "default" => {
                                if let Err(err) = open::that(&data.path) {
                                    eprintln!("error on open: {err}")
                                }
                            }
                            _ => println!("unhandled response: {action}"),
                        }),
                        Err(e) => eprintln!("error: {:?}", e),
                    }
                }
                _ => println!("{:#?}", e),
            }

            last_id = e.id;
        }
    }

    Ok(())
}
