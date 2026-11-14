//! Discord Rich Presence (`discord/`): once a second, the session's facts
//! become the wanted activity, which the presence's worker sends to the
//! Discord client when it changed. FSDiscordConnect::Tick runs
//! updateRichPresence on every frame of the main loop; once a second is
//! enough for a text Discord shows at most every few seconds.
use super::*;
use crate::discord::{self, activity};

impl App {
    /// Each frame; nothing to do between two checks.
    pub(super) fn discord_presence(&mut self) {
        let now = Instant::now();
        if self.discord.next_check.is_some_and(|t| now < t) {
            return;
        }
        self.discord.next_check = Some(now + Duration::from_secs(1));
        let on = self.settings.discord.on || (self.demo && crate::demo::discord_on());
        let in_world = self.in_world() && self.world.main().is_some();
        if !on || !in_world {
            self.discord_since = None;
            self.discord.show(None);
            return;
        }
        let Some(app_id) = discord::app_id(&self.settings.discord) else {
            self.discord.set_no_app_id();
            return;
        };
        let since = *self.discord_since.get_or_insert_with(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs())
        });
        let grid = if self.second_life() {
            match self.settings.grid {
                crate::settings::GridChoice::SecondLifeBeta => activity::Grid::SecondLifeBeta,
                _ => activity::Grid::SecondLife,
            }
        } else {
            activity::Grid::Other
        };
        let w = &self.world;
        let handle = w.main_region;
        let info = w.main().and_then(|r| r.info.as_ref());
        let region = w.region_name();
        let name = w.person_name(&w.agent_id);
        // the limit is only known while the region window shows this region
        let max_avatars = w
            .estate
            .region
            .as_ref()
            .filter(|_| handle.is_some() && w.estate.open == handle)
            .map(|d| d.max_agents)
            .filter(|max| *max > 0);
        let facts = activity::Facts {
            region: &region,
            region_id: info.map(|i| i.region_id).unwrap_or_default(),
            sim_access: info.map(|i| i.sim_access),
            pos: w.agent.position,
            avatars: handle.and_then(|h| w.coarse.get(&h)).map_or(1, Vec::len),
            max_avatars,
            name: name.as_deref(),
            hide_location: w.rlva.has("showloc"),
            hide_names: w.rlva.has("shownames") || w.rlva.has("shownametags"),
            grid,
            since,
        };
        let texts = activity::Texts {
            hidden_region: aurora_i18n::tr!("discord-hidden-region").to_owned(),
            via: aurora_i18n::tr!("discord-via").to_owned(),
            button: aurora_i18n::tr!("discord-button").to_owned(),
            second_life: "Second Life".to_owned(),
            second_life_beta: aurora_i18n::tr!("discord-grid-beta").to_owned(),
            opensim: "OpenSimulator".to_owned(),
        };
        let activity = activity::activity(&self.settings.discord, &facts, &texts);
        self.discord.show(Some(discord::Wanted { app_id, activity }));
    }
}
