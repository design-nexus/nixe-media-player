//! Media Player's tools on the control interface (`nixe_core::control`), for Nixe
//! Assistant: find movies and shows, play one, and read or steer what's playing.

use crate::library::{store, Video};
use crate::player::{self, queue::Repeat, State};
use nixe_core::control::{text, Reply};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::rc::Rc;

pub fn tools() -> Value {
    json!([
        {"name": "video.search", "description": "Search the Media Player library: movies, shows and episodes matching the words.", "inputSchema": {"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]}},
        {"name": "video.play", "description": "Play a movie, a show (its next episode), an episode or a playlist from the Media Player library, by name.", "inputSchema": {"type": "object", "properties": {"what": {"enum": ["movie", "show", "episode", "playlist"]}, "name": {"type": "string"}}, "required": ["what", "name"]}},
        {"name": "video.status", "description": "What Media Player is playing: title, show, position, playing or paused.", "inputSchema": {"type": "object", "properties": {}}},
        {"name": "video.control", "description": "Play, pause, toggle, next, previous or stop in Media Player.", "inputSchema": {"type": "object", "properties": {"action": {"enum": ["play", "pause", "toggle", "next", "previous", "stop"]}}, "required": ["action"]}},
    ])
}

fn video(v: &Video) -> Value {
    json!({"title": v.title, "year": v.year, "show": v.show, "season": v.season, "episode": v.episode, "minutes": (v.duration / 60.0).round()})
}

/// How well `name` fits `query`: 3 the same, 2 starts with it, 1 contains it.
fn fit(name: &str, query: &str) -> u8 {
    let (n, q) = (name.to_lowercase(), query.trim().to_lowercase());
    if q.is_empty() {
        0
    } else if n == q {
        3
    } else if n.starts_with(&q) {
        2
    } else if n.contains(&q) {
        1
    } else {
        0
    }
}

fn best<T>(items: Vec<T>, name: &str, label: impl Fn(&T) -> String) -> Result<T, String> {
    let top = items.iter().map(|i| fit(&label(i), name)).max().unwrap_or(0);
    if top == 0 {
        return Err(format!("nothing in the library matches “{name}”"));
    }
    let mut found: Vec<T> = items.into_iter().filter(|i| fit(&label(i), name) == top).collect();
    let mut names: Vec<String> = found.iter().take(5).map(&label).collect();
    names.dedup();
    if names.len() > 1 {
        return Err(format!("several match: {}", names.join("; ")));
    }
    Ok(found.remove(0))
}

fn paths(videos: &[Rc<Video>]) -> Vec<PathBuf> {
    videos.iter().map(|v| v.path.clone()).collect()
}

fn status() -> Value {
    let state = match player::state() {
        State::Playing => "playing",
        State::Paused => "paused",
        State::Stopped => "stopped",
    };
    let repeat = match player::repeat() {
        Repeat::Off => "off",
        Repeat::All => "all",
        Repeat::One => "one",
    };
    json!({"state": state, "video": player::current().map(|v| video(&v)), "position": player::position().round(), "duration": player::duration().round(), "repeat": repeat})
}

pub fn handle(name: &str, args: Value, reply: Reply) {
    if !store::loaded() && matches!(name, "video.search" | "video.play") {
        return reply(Err("the library is still loading; try again in a moment".into()));
    }
    match name {
        "video.search" => reply(text(&args, "query").map(|q| {
            let hit = |s: &str| fit(s, &q) > 0;
            let movies: Vec<Value> = store::movies().iter().filter(|v| hit(&v.title)).take(10).map(|v| video(v)).collect();
            let shows: Vec<Value> = store::shows().iter().filter(|s| hit(&s.name)).take(10).map(|s| json!({"name": s.name, "year": s.year, "seasons": s.seasons.len()})).collect();
            let episodes: Vec<Value> = store::videos().iter().filter(|v| !v.show.is_empty() && hit(&v.title)).take(10).map(|v| video(v)).collect();
            json!({"movies": movies, "shows": shows, "episodes": episodes})
        })),
        "video.play" => {
            let (what, wanted) = match (text(&args, "what"), text(&args, "name")) {
                (Ok(w), Ok(n)) => (w, n),
                (Err(e), _) | (_, Err(e)) => return reply(Err(e)),
            };
            let play = |label: String, list: Vec<Rc<Video>>, start: usize| -> Result<Value, String> {
                if list.is_empty() {
                    return Err(format!("{label} has nothing to play"));
                }
                player::play_paths(paths(&list), start);
                Ok(json!({"playing": label, "items": list.len()}))
            };
            match what.as_str() {
                "movie" => reply(best(store::movies(), &wanted, |v| v.title.clone()).and_then(|v| play(v.title.clone(), vec![v], 0))),
                "episode" => reply(
                    best(store::videos().into_iter().filter(|v| !v.show.is_empty()).collect(), &wanted, |v| v.title.clone())
                        .and_then(|v| play(format!("{} ({})", v.title, v.show), vec![v], 0)),
                ),
                "show" => reply(best(store::shows(), &wanted, |s| s.name.clone()).and_then(|s| {
                    let all: Vec<Rc<Video>> = s.episodes().cloned().collect();
                    // Where the person left off, else the first episode.
                    let start = s.next_episode().and_then(|n| all.iter().position(|e| e.path == n.path)).unwrap_or(0);
                    play(s.name.clone(), all, start)
                })),
                "playlist" => match best(store::playlists(), &wanted, |p| p.name.clone()) {
                    Ok(p) => {
                        let label = p.name.clone();
                        store::playlist_videos(p.id, move |list| reply(play(label, list, 0)));
                    }
                    Err(e) => reply(Err(e)),
                },
                other => reply(Err(format!("unknown kind {other}"))),
            }
        }
        "video.status" => reply(Ok(status())),
        "video.control" => reply((|| {
            match text(&args, "action")?.as_str() {
                "play" => player::play(),
                "pause" => player::pause(),
                "toggle" => player::toggle(),
                "next" => player::next(),
                "previous" => player::previous(),
                "stop" => player::stop(),
                other => return Err(format!("unknown action {other}")),
            }
            Ok(status())
        })()),
        _ => reply(Err(format!("unknown tool {name}"))),
    }
}
