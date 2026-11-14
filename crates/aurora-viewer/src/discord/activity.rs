//! What the Discord activity says of the session: pure rules, from the
//! viewer's state to the JSON of SET_ACTIVITY.
//!
//! Port of FSDiscordConnect::updateRichPresence and isRegionVisible
//! (indra/newview/fsdiscordconnect.cpp, originally LGPL 2.1):
//! - the state line is "Region (x, y, z)" when the region may be shown, a
//!   hidden-region text otherwise: RLVa @showloc, a rating above the one
//!   chosen (FSMaxSharedMaturity) or a name in the list
//!   (FSBlacklistedRegionNames, compared ignoring case);
//! - the details line is our complete name only when the user shares it
//!   (FSShareNameToDiscord) and RLVa does not hide names;
//! - the elapsed time counts from the start of the session (the Legacy
//!   Viewer: from the connection to Discord, which restarts the clock when
//!   Discord restarts);
//! - the large image is the grid's (Second Life or OpenSimulator) with the
//!   grid's name, the small one the viewer's.
//!
//! Deliberate differences, all stricter: the party (the region's id and its
//! number of avatars) goes only with a region that may be shown, where the
//! Legacy Viewer sends the region's id under "Hidden Region" too; a region
//! whose rating is not known yet is hidden; "Don't share" hides every
//! region, an unrated one included; a custom grid is named "OpenSimulator",
//! never by its address. Added: a button to the project's website, which
//! the people looking at the profile see.

use super::{Settings, Shared};
use aurora_net::search::{SIM_ACCESS_ADULT, SIM_ACCESS_MATURE, SIM_ACCESS_PG};
use glam::Vec3;
use serde_json::{Value, json};
use uuid::Uuid;

/// The grid the session is on, for the large image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grid {
    SecondLife,
    SecondLifeBeta,
    Other,
}

/// The Discord application's images (the "Rich Presence › Art Assets" of
/// its page on the Discord Developer Portal), by the names docs/DISCORD.md
/// asks to upload them under.
pub const IMAGE_SECOND_LIFE: &str = "secondlife";
pub const IMAGE_OPENSIM: &str = "opensimulator";
pub const IMAGE_AURORA: &str = "aurora";

/// What the viewer knows of the session, taken once per update.
#[derive(Debug, Clone)]
pub struct Facts<'a> {
    pub region: &'a str,
    pub region_id: Uuid,
    /// The region's SIM_ACCESS_* rating; None until RegionInfo came.
    pub sim_access: Option<u8>,
    /// Region coordinates.
    pub pos: Vec3,
    /// Avatars in the region, ourselves included.
    pub avatars: usize,
    /// The region's avatar limit, when known (About Land, the region window).
    pub max_avatars: Option<u32>,
    /// Our complete name ("Display (user.name)"), when known.
    pub name: Option<&'a str>,
    /// RLVa @showloc.
    pub hide_location: bool,
    /// RLVa @shownames or @shownametags.
    pub hide_names: bool,
    pub grid: Grid,
    /// Start of the session, in seconds since 1970.
    pub since: u64,
}

/// The words of the activity, in the user's language.
#[derive(Debug, Clone)]
pub struct Texts {
    pub hidden_region: String,
    /// "via Aurora Viewer" (the small image's text).
    pub via: String,
    pub button: String,
    pub second_life: String,
    pub second_life_beta: String,
    pub opensim: String,
}

/// The highest rating shown for a choice (the values of FSMaxSharedMaturity).
fn ceiling(shared: Shared) -> Option<u8> {
    match shared {
        Shared::None => None,
        Shared::General => Some(SIM_ACCESS_PG),
        Shared::Moderate => Some(SIM_ACCESS_MATURE),
        Shared::Adult => Some(SIM_ACCESS_ADULT),
    }
}

/// isRegionVisible, with RLVa's canShowLocation: whether the region's name
/// and our position may go to Discord.
pub fn location_shown(s: &Settings, region: &str, sim_access: Option<u8>, hide_location: bool) -> bool {
    if hide_location || region.trim().is_empty() {
        return false;
    }
    let (Some(max), Some(rating)) = (ceiling(s.max_maturity), sim_access) else {
        return false;
    };
    // an unknown rating (0, SIM_ACCESS_MIN) is not "General"
    if rating < SIM_ACCESS_PG || rating > max {
        return false;
    }
    let name = region.trim().to_lowercase();
    !s.hidden_regions.iter().any(|h| h.trim().to_lowercase() == name)
}

/// Discord refuses a text over 128 characters, and a state or details line
/// under 2.
fn line(text: &str) -> Option<String> {
    let text: String = text.trim().chars().take(128).collect();
    (text.chars().count() >= 2).then_some(text)
}

/// The activity of SET_ACTIVITY.
pub fn activity(s: &Settings, f: &Facts, t: &Texts) -> Value {
    let shown = location_shown(s, f.region, f.sim_access, f.hide_location);
    let state = if shown {
        format!(
            "{} ({}, {}, {})",
            f.region.trim(),
            f.pos.x.round().max(0.0) as i32,
            f.pos.y.round().max(0.0) as i32,
            f.pos.z.round().max(0.0) as i32
        )
    } else {
        t.hidden_region.clone()
    };
    let (large, grid) = match f.grid {
        Grid::SecondLife => (IMAGE_SECOND_LIFE, &t.second_life),
        Grid::SecondLifeBeta => (IMAGE_SECOND_LIFE, &t.second_life_beta),
        Grid::Other => (IMAGE_OPENSIM, &t.opensim),
    };
    let mut out = json!({
        "type": 0,
        "timestamps": { "start": f.since },
        "assets": {
            "large_image": large,
            "large_text": grid,
            "small_image": IMAGE_AURORA,
            "small_text": t.via,
        },
        "buttons": [ { "label": t.button, "url": crate::links::WEBSITE } ],
    });
    if let Some(state) = line(&state) {
        out["state"] = Value::String(state);
    }
    if s.share_name
        && !f.hide_names
        && let Some(name) = f.name.and_then(line)
    {
        out["details"] = Value::String(name);
    }
    if shown && !f.region_id.is_nil() {
        let mut party = json!({ "id": f.region_id.to_string() });
        let size = f.avatars.max(1) as u64;
        // Discord shows "(1 of 40)"; it refuses a size above the limit
        if let Some(max) = f.max_avatars.map(u64::from).filter(|max| *max >= size) {
            party["size"] = json!([size, max]);
        }
        out["party"] = party;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts() -> Texts {
        Texts {
            hidden_region: "Hidden Region".into(),
            via: "via Aurora Viewer".into(),
            button: "Aurora Viewer".into(),
            second_life: "Second Life".into(),
            second_life_beta: "Second Life Beta".into(),
            opensim: "OpenSimulator".into(),
        }
    }

    fn facts() -> Facts<'static> {
        Facts {
            region: "Aurora Plaza",
            region_id: Uuid::from_u128(7),
            sim_access: Some(SIM_ACCESS_PG),
            pos: Vec3::new(128.4, 64.6, 22.0),
            avatars: 3,
            max_avatars: Some(40),
            name: Some("Jane (jane.resident)"),
            hide_location: false,
            hide_names: false,
            grid: Grid::SecondLife,
            since: 1_700_000_000,
        }
    }

    #[test]
    fn the_defaults_share_a_general_region_but_not_the_name() {
        let s = Settings::default();
        let a = activity(&s, &facts(), &texts());
        assert_eq!(a["state"], "Aurora Plaza (128, 65, 22)");
        assert!(a.get("details").is_none());
        assert_eq!(a["party"]["id"], Uuid::from_u128(7).to_string());
        assert_eq!(a["party"]["size"], json!([3, 40]));
        assert_eq!(a["timestamps"]["start"], 1_700_000_000);
        assert_eq!(a["assets"]["large_image"], IMAGE_SECOND_LIFE);
        assert_eq!(a["assets"]["large_text"], "Second Life");
    }

    #[test]
    fn a_rating_above_the_choice_hides_the_region_and_the_party() {
        let s = Settings::default();
        let mut f = facts();
        f.sim_access = Some(SIM_ACCESS_MATURE);
        let a = activity(&s, &f, &texts());
        assert_eq!(a["state"], "Hidden Region");
        assert!(a.get("party").is_none());
        let s = Settings {
            max_maturity: Shared::Moderate,
            ..Settings::default()
        };
        assert_eq!(activity(&s, &f, &texts())["state"], "Aurora Plaza (128, 65, 22)");
        f.sim_access = Some(SIM_ACCESS_ADULT);
        assert_eq!(activity(&s, &f, &texts())["state"], "Hidden Region");
        let s = Settings {
            max_maturity: Shared::Adult,
            ..Settings::default()
        };
        assert_eq!(activity(&s, &f, &texts())["state"], "Aurora Plaza (128, 65, 22)");
    }

    #[test]
    fn dont_share_and_an_unknown_rating_hide_every_region() {
        let s = Settings {
            max_maturity: Shared::None,
            ..Settings::default()
        };
        assert!(!location_shown(&s, "Aurora Plaza", Some(SIM_ACCESS_PG), false));
        let s = Settings {
            max_maturity: Shared::Adult,
            ..Settings::default()
        };
        assert!(!location_shown(&s, "Aurora Plaza", None, false));
        assert!(!location_shown(&s, "Aurora Plaza", Some(0), false));
        assert!(!location_shown(&s, "  ", Some(SIM_ACCESS_PG), false));
    }

    #[test]
    fn a_listed_region_is_hidden_whatever_its_case() {
        let s = Settings {
            hidden_regions: vec!["  aurora PLAZA ".into()],
            ..Settings::default()
        };
        assert!(!location_shown(&s, "Aurora Plaza", Some(SIM_ACCESS_PG), false));
        assert!(location_shown(&s, "Aurora Bay", Some(SIM_ACCESS_PG), false));
    }

    #[test]
    fn rlva_hides_the_location_and_the_name() {
        let s = Settings {
            share_name: true,
            ..Settings::default()
        };
        let mut f = facts();
        assert_eq!(activity(&s, &f, &texts())["details"], "Jane (jane.resident)");
        f.hide_location = true;
        f.hide_names = true;
        let a = activity(&s, &f, &texts());
        assert_eq!(a["state"], "Hidden Region");
        assert!(a.get("details").is_none());
        assert!(a.get("party").is_none());
    }

    #[test]
    fn the_party_size_needs_a_known_limit_above_it() {
        let s = Settings::default();
        let mut f = facts();
        f.max_avatars = None;
        assert!(activity(&s, &f, &texts())["party"].get("size").is_none());
        f.max_avatars = Some(2);
        assert!(activity(&s, &f, &texts())["party"].get("size").is_none());
        f.max_avatars = Some(40);
        f.avatars = 0;
        assert_eq!(activity(&s, &f, &texts())["party"]["size"], json!([1, 40]));
    }

    #[test]
    fn another_grid_shows_opensimulator_and_long_lines_are_cut() {
        let s = Settings {
            share_name: true,
            ..Settings::default()
        };
        let long = "x".repeat(200);
        let mut f = facts();
        f.grid = Grid::Other;
        f.name = Some(&long);
        let a = activity(&s, &f, &texts());
        assert_eq!(a["assets"]["large_image"], IMAGE_OPENSIM);
        assert_eq!(a["assets"]["large_text"], "OpenSimulator");
        assert_eq!(a["details"].as_str().map(str::len), Some(128));
        f.name = Some("x");
        assert!(activity(&s, &f, &texts()).get("details").is_none());
    }
}
