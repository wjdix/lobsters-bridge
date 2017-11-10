#![cfg_attr(feature="clippy", feature(plugin))]
#![cfg_attr(feature="clippy", plugin(clippy))]

extern crate reqwest;
extern crate rss;
extern crate xml;
extern crate csv;
extern crate serde_json;
extern crate hyper;

#[macro_use]
extern crate serde_derive;

use reqwest::StatusCode;

use std::{env, io, fs};
use std::collections::HashSet;
use std::io::Write;

#[derive(Serialize, Deserialize, Debug)]
struct RwxPost {
    title: String,
    url: String,
    tags_a: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct RwxTag {
    id: f32,
    tag: String,
    description: Option<String>,
    privileged: bool,
    is_media: bool,
    inactive: bool,
    hotness_mod: f32,
}

fn rwx_auth() -> hyper::header::Authorization<String> {
    let token = env::var("RWX_TOKEN").expect("Must set token in env");
    reqwest::header::Authorization(token)
}

fn rwx_tags() -> HashSet<String> {
    let resp: Vec<RwxTag> = reqwest::Client::new()
        .get("http://localhost:3000/api/tags")
        .header(reqwest::header::ContentType::json())
        .header(rwx_auth())
        .send()
        .expect("Bad response")
        .json()
        .unwrap();

    resp.iter()
        .map(|tag| tag.tag.to_owned())
        .collect::<HashSet<String>>()
}

fn already_posted() -> HashSet<String> {
    let mut set = HashSet::new();
    if let Ok(f) = fs::File::open("lobste.rs") {
        use std::io::BufRead;

        let f = io::BufReader::new(f);
        set.extend(f.lines().map(Result::unwrap))
    }
    set
}

fn posted(guid: &str) {
    println!("Posting: {}", guid);
    let mut f = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open("lobste.rs")
        .unwrap();
    writeln!(f, "{}", guid).unwrap();
}

fn post_to_rwx(
    title: String,
    url: String,
    tags: Vec<String>,
) -> reqwest::Result<reqwest::Response> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::RedirectPolicy::none())
        .build()
        .unwrap();
    let mut request = std::collections::HashMap::new();
    request.insert(
        "story",
        RwxPost {
            title: title,
            url: url,
            tags_a: tags,
        },
    );
    client
        .post("http://localhost:3000/api/stories")
        .header(rwx_auth())
        .json(&request)
        .send()
}

fn main() {
    let stories = reqwest::get("http://lobste.rs/rss").expect("Bad response");
    let buf_stories = io::BufReader::new(stories);

    let channel = rss::Channel::read_from(buf_stories).expect("Couldn't read channel");

    let tags = rwx_tags();
    let already_posted = already_posted();

    channel
        .items()
        .into_iter()
        .filter_map(|item| {
            let guid = item.guid().unwrap().value();
            if already_posted.contains(guid) {
                None
            } else {
                let title = item.title().unwrap();
                let link = item.link().unwrap();
                let categories = item.categories()
                    .iter()
                    .map(|category| category.name().to_owned())
                    .collect::<HashSet<String>>();
                let common = categories
                    .intersection(&tags)
                    .map(|string| string.to_owned())
                    .collect::<Vec<String>>();
                let mut final_categories = if common.is_empty() {
                    vec!["random".to_owned()]
                } else {
                    common
                };
                final_categories.push("automated".to_owned());

                Some((guid, title, link, final_categories))
            }
        })
        .take(5)
        .for_each(|(guid, title, link, categories)| match post_to_rwx(
            title.to_owned(),
            link.to_owned(),
            categories,
        ) {
            Result::Ok(resp) => {
                if StatusCode::Created == resp.status() {
                    posted(guid)
                } else {
                    println!("status: {}", resp.status())
                }
            }
            Result::Err(_) => {}
        });
}
