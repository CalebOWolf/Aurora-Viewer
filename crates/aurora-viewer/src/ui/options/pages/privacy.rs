//! Preferences › Privacy: what the grid shows of us ("Show me in search
//! results", "Only friends and groups know I'm online" of
//! panel_preferences_privacy.xml, and "Email me IMs when I'm offline" of
//! panel_preferences_chat.xml; kept by the grid, `world/user_info.rs`),
//! "Only friends and groups can call or IM me" and the objects' messages
//! from elsewhere (`world/object_im_filter.rs`), what we send of where we
//! look (the Legacy Viewer's "Privacy › LookAt"), whether our photos are
//! heard around ("Quiet snapshots"), what the scripts learn of us, the
//! content we see and whether the login screen offers our favourite
//! landmarks.

use super::super::cx::{Cx, Nothing, field};
use super::super::{maturity_choice, toggle};
use crate::ui::widgets;
use crate::world::lookat::MAX_DISTANCE;
use crate::world::object_im_filter::Scope;
use crate::world::user_info::Change;
use aurora_net::user_info::UserInfo;

/// Second Life's page for the e-mail of offline IMs (email_settings of
/// panel_preferences_chat.xml).
const SECOND_LIFE_EMAIL_SETTINGS: &str = "https://accounts.secondlife.com/change_email";

/// The account settings the grid keeps, while logged in.
#[derive(Debug, Clone, Default)]
pub struct Account {
    /// The UserInfo answer, None until it comes (the switches wait).
    pub info: Option<UserInfo>,
    /// Our profile's "show in search" flag, None until it comes.
    pub show_in_search: Option<bool>,
    /// Second Life keeps the e-mail of offline IMs on its web page.
    pub second_life: bool,
}

/// setPersonalInfo: the address in the label, cut at 30 characters.
fn shown_email(email: &str) -> String {
    if email.chars().count() > 30 {
        format!("{}...", email.chars().take(30).collect::<String>())
    } else {
        email.to_owned()
    }
}

/// The settings the grid keeps; before login, a line says so
/// (email_settings_login_to_change).
fn account_group(cx: &mut Cx) {
    use aurora_i18n::tr;
    let p = cx.p;
    cx.group(tr!("ui-options-account-visibility"), |cx| {
        let Some(account) = cx.st.account.clone() else {
            cx.row(tr!("ui-options-account-visibility"), "", &Nothing, |ui, _| {
                ui.label(
                    egui::RichText::new(tr!("ui-options-account-visibility-login"))
                        .size(11.5)
                        .color(p.muted),
                );
                false
            });
            return;
        };
        let known = account.show_in_search.is_some();
        let mut publish = account.show_in_search.unwrap_or(false);
        cx.row_with(
            tr!("ui-options-show-me-in-search"),
            tr!("ui-options-show-me-in-search-hint"),
            &Nothing,
            |ui, _, _, r| {
                let changed = ui.add_enabled_ui(known, |ui| toggle(ui, &p, &mut publish)).inner;
                if changed {
                    r.account = Some(Change::ShowInSearch(publish));
                }
                false
            },
        );
        let info = account.info.clone();
        let mut hide = info.as_ref().is_some_and(UserInfo::hide_online);
        let editable = info.as_ref().is_some_and(UserInfo::visibility_editable);
        cx.row_with(
            tr!("ui-options-online-friends-only"),
            tr!("ui-options-online-friends-only-hint"),
            &Nothing,
            |ui, _, _, r| {
                if ui.add_enabled_ui(editable, |ui| toggle(ui, &p, &mut hide)).inner {
                    r.account = Some(Change::HideOnline(hide));
                }
                false
            },
        );
        if account.second_life {
            // the setting lives on the account's web page
            cx.row(
                tr!("ui-options-offline-im-email"),
                tr!("ui-options-offline-im-email-web-hint"),
                &Nothing,
                |ui, _| {
                    if widgets::action_button(
                        ui,
                        &p,
                        "external-link",
                        tr!("ui-options-offline-im-email-open"),
                        widgets::ActionKind::Flat,
                        egui::Vec2::new(0.0, 24.0),
                    )
                    .clicked()
                    {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(SECOND_LIFE_EMAIL_SETTINGS));
                    }
                    false
                },
            );
        } else {
            let mut email = info.as_ref().is_some_and(|i| i.im_via_email);
            let address = info.as_ref().map(|i| shown_email(&i.email)).unwrap_or_default();
            let known = info.is_some();
            cx.row_with(
                &tr!("ui-options-offline-im-email-to", email = address),
                tr!("ui-options-offline-im-email-hint"),
                &Nothing,
                |ui, _, _, r| {
                    if ui.add_enabled_ui(known, |ui| toggle(ui, &p, &mut email)).inner {
                        r.account = Some(Change::ImViaEmail(email));
                    }
                    false
                },
            );
        }
    });
}

/// "Show my Favorite Landmarks at Login" (favorites_on_login_check of
/// panel_preferences_privacy.xml, ShowFavoritesOnLogin): kept per account,
/// so it waits for a login, as in the Legacy Viewer.
fn login_favorites_row(cx: &mut Cx) {
    use aurora_i18n::tr;
    let p = cx.p;
    let state = cx.st.login_favorites;
    let mut shown = state.unwrap_or(true);
    let hint = if state.is_some() {
        tr!("ui-options-login-favorites-hint")
    } else {
        tr!("ui-options-login-favorites-login")
    };
    cx.row_with(tr!("ui-options-login-favorites"), hint, &Nothing, |ui, _, _, r| {
        if ui.add_enabled_ui(state.is_some(), |ui| toggle(ui, &p, &mut shown)).inner {
            r.login_favorites = Some(shown);
        }
        false
    });
}

pub(super) fn page(cx: &mut Cx) {
    use aurora_i18n::tr;
    account_group(cx);
    login_favorites_row(cx);
    cx.choice(
        tr!("poser-privacy"),
        tr!("poser-privacy-hint"),
        &field!(pose_requests.policy),
        &[
            (crate::world::poser_consent::Policy::Everyone, tr!("poser-policy-everyone"), false),
            (crate::world::poser_consent::Policy::Friends, tr!("poser-policy-friends"), false),
            (crate::world::poser_consent::Policy::Nobody, tr!("poser-policy-nobody"), false),
        ],
    );
    // "Only friends and groups can call or IM me" (voice_call_friends_only_check,
    // VoiceCallsFriendsOnly; its hint is the FriendsAndGroupsOnly notice)
    cx.group(tr!("ui-options-calls-and-messages"), |cx| {
        cx.toggle(
            tr!("ui-options-calls-friends-only"),
            tr!("ui-options-calls-friends-only-hint"),
            &field!(autoresponse.friends_only),
        );
        cx.toggle(
            tr!("ui-options-block-distant-object-messages"),
            tr!("ui-options-block-distant-object-messages-hint"),
            &field!(object_im_filter.on),
        );
        let off = !cx.s.object_im_filter.on;
        cx.subs(off, |cx| {
            cx.choice(
                tr!("ui-options-object-messages-from"),
                tr!("ui-options-object-messages-from-hint"),
                &field!(object_im_filter.scope),
                &[
                    (Scope::Region, tr!("ui-options-object-messages-from-region"), false),
                    (Scope::Parcel, tr!("ui-options-object-messages-from-parcel"), false),
                ],
            );
            cx.toggle(
                tr!("ui-options-object-messages-allow-friends"),
                tr!("ui-options-object-messages-allow-friends-hint"),
                &field!(object_im_filter.allow_friends),
            );
        });
    });
    cx.group(tr!("ui-options-my-look-at"), |cx| {
        cx.toggle(
            tr!("ui-options-private-look-at"),
            tr!("ui-options-private-look-at-help"),
            &field!(lookat.private),
        );
        cx.toggle(
            tr!("ui-options-private-local-look-at"),
            tr!("ui-options-private-local-look-at-help"),
            &field!(lookat.private_local),
        );
        cx.toggle(
            tr!("ui-options-private-point-at"),
            tr!("ui-options-private-point-at-help"),
            &field!(lookat.private_point_at),
        );
        cx.toggle(
            tr!("ui-options-limit-look-at-distance"),
            tr!("ui-options-more-distant-targets-are-limited-to-this-distance-from"),
            &field!(lookat.limit_distance),
        );
        let off = !cx.s.lookat.limit_distance;
        cx.subs(off, |cx| {
            cx.slider_whole(
                tr!("ui-options-look-at-distance"),
                "",
                &field!(lookat.max_distance),
                MAX_DISTANCE,
                " m",
            );
        });
    });
    // Advanced › Quiet Snapshots of the Legacy Viewer (PlayModeUISndSnapshot):
    // no shutter heard around nor snapshot animation seen
    cx.group(tr!("ui-options-privacy-photos"), |cx| {
        let p = cx.p;
        cx.keywords("PlayModeUISndSnapshot").row(
            tr!("photo-quiet-snapshots"),
            tr!("photo-quiet-snapshots-hint"),
            &field!(snapshot.sound),
            |ui, s| {
                let mut quiet = !s.snapshot.sound;
                let changed = toggle(ui, &p, &mut quiet);
                s.snapshot.sound = !quiet;
                changed
            },
        );
    });
    cx.group(tr!("ui-options-scripts-and-content"), |cx| {
        cx.toggle(tr!("language-share"), tr!("language-share-help"), &field!(language_is_public));
        // the account's ceiling, while logged in
        if let Some(access) = cx.st.maturity {
            let p = cx.p;
            cx.wide().row_with(
                tr!("ui-options-content-rated"),
                tr!("ui-options-content-rated-help"),
                &Nothing,
                |ui, _, _, r| {
                    if let Some(m) = maturity_choice(ui, &p, access) {
                        r.maturity = Some(m);
                    }
                    false
                },
            );
        }
    });
    discord_group(cx);
    // What the crash window does with a report: a public GitHub issue in
    // the tester's name (crash/github.rs), never sent elsewhere.
    cx.group(tr!("ui-options-crash-reports"), |cx| {
        use crate::settings::CrashReports;
        cx.keywords("GitHub").choice(
            tr!("ui-options-crash-reports-send"),
            tr!("ui-options-crash-reports-send-hint"),
            &field!(crash_reports),
            &[
                (CrashReports::Ask, tr!("crash-reports-ask"), false),
                (CrashReports::Always, tr!("crash-reports-always"), false),
                (CrashReports::Never, tr!("crash-reports-never"), false),
            ],
        );
        // Always offered: reading the credential store at every frame to
        // hide it would cost more than a harmless click.
        let p = cx.p;
        cx.keywords("GitHub").row(
            tr!("ui-options-crash-github"),
            tr!("ui-options-crash-github-hint"),
            &Nothing,
            |ui, _| {
                let forget = tr!("ui-options-crash-github-forget");
                if widgets::action_button(ui, &p, "unlink", forget, widgets::ActionKind::Flat, egui::Vec2::new(0.0, 26.0)).clicked() {
                    crate::crash::github::forget_authorization();
                }
                false
            },
        );
    });
    // The project's privacy policy, on the website.
    cx.group(tr!("ui-options-your-data"), |cx| {
        let p = cx.p;
        cx.row(
            tr!("ui-options-privacy-policy"),
            tr!("ui-options-privacy-policy-hint"),
            &Nothing,
            |ui, _| {
                let read = tr!("ui-options-privacy-policy-open");
                if widgets::action_button(ui, &p, "external-link", read, widgets::ActionKind::Flat, egui::Vec2::new(0.0, 24.0)).clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(crate::links::PRIVACY_POLICY));
                }
                false
            },
        );
    });
}

/// Discord Rich Presence (`discord/`): the options of FSFloaterDiscord
/// (floater_fs_discord.xml) on this page, beside the other things the
/// viewer tells about us. The connection follows the switch, so the
/// floater's Connect / Disconnect buttons become a status line.
fn discord_group(cx: &mut Cx) {
    use crate::discord::{Shared, Status};
    use aurora_i18n::tr;
    let p = cx.p;
    cx.group(tr!("ui-options-discord"), |cx| {
        cx.keywords("Discord Rich Presence")
            .toggle(tr!("ui-options-discord-on"), tr!("ui-options-discord-on-hint"), &field!(discord.on));
        let off = !cx.s.discord.on;
        cx.subs(off, |cx| {
            cx.keywords("Discord").row_with(
                tr!("ui-options-discord-status"),
                tr!("ui-options-discord-status-hint"),
                &Nothing,
                |ui, s, st, _| {
                    let text = match &st.discord {
                        Status::Off if s.discord.on => tr!("ui-options-discord-status-in-world").to_owned(),
                        Status::Off => tr!("ui-options-discord-status-off").to_owned(),
                        Status::NoAppId => tr!("ui-options-discord-status-no-app").to_owned(),
                        Status::NotRunning => tr!("ui-options-discord-status-not-running").to_owned(),
                        Status::Connecting => tr!("ui-options-discord-status-connecting").to_owned(),
                        Status::Connected(name) if name.is_empty() => tr!("ui-options-discord-status-connected").to_owned(),
                        Status::Connected(name) => tr!("ui-options-discord-status-connected-as", name = name.as_str()),
                        Status::Refused(message) => tr!("ui-options-discord-status-refused", message = message.as_str()),
                    };
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(text).size(12.0).color(p.muted));
                        let guide = tr!("ui-options-discord-guide");
                        if widgets::action_button(
                            ui,
                            &p,
                            "help-circle-filled",
                            guide,
                            widgets::ActionKind::Flat,
                            egui::Vec2::new(0.0, 24.0),
                        )
                        .clicked()
                        {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(crate::links::DISCORD_GUIDE));
                        }
                    });
                    false
                },
            );
            cx.toggle(
                tr!("ui-options-discord-share-name"),
                tr!("ui-options-discord-share-name-hint"),
                &field!(discord.share_name),
            );
            cx.keywords("FSMaxSharedMaturity").combo(
                tr!("ui-options-discord-maturity"),
                tr!("ui-options-discord-maturity-hint"),
                &field!(discord.max_maturity),
                &[
                    (Shared::Adult, tr!("ui-options-discord-maturity-adult")),
                    (Shared::Moderate, tr!("ui-options-discord-maturity-moderate")),
                    (Shared::General, tr!("ui-options-discord-maturity-general")),
                    (Shared::None, tr!("ui-options-discord-maturity-none")),
                ],
            );
            let listed = cx.s.discord.hidden_regions.join(" ");
            cx.wide().below().keywords(&listed).row_with(
                tr!("ui-options-discord-hidden-regions"),
                tr!("ui-options-discord-hidden-regions-hint"),
                &field!(discord.hidden_regions),
                |ui, s, st, _| hidden_regions(ui, &p, &mut s.discord, &mut st.discord_region_typed),
            );
            cx.keywords("client id").row(
                tr!("ui-options-discord-app-id"),
                tr!("ui-options-discord-app-id-hint"),
                &field!(discord.app_id),
                |ui, s| {
                    let id = s.discord.app_id.trim();
                    let wrong = !id.is_empty() && !crate::discord::valid_app_id(id);
                    let built_in = crate::discord::built_in_app_id().is_some();
                    let hint = if built_in {
                        tr!("ui-options-discord-app-id-built-in")
                    } else {
                        tr!("ui-options-discord-app-id-none")
                    };
                    ui.vertical(|ui| {
                        let changed = ui
                            .add(
                                egui::TextEdit::singleline(&mut s.discord.app_id)
                                    .desired_width(180.0)
                                    .hint_text(hint),
                            )
                            .changed();
                        if wrong {
                            ui.label(
                                egui::RichText::new(tr!("ui-options-discord-app-id-wrong"))
                                    .size(11.5)
                                    .color(p.danger),
                            );
                        }
                        changed
                    })
                    .inner
                },
            );
        });
    });
}

/// The regions never shared (FSFloaterDiscord's blacklisted_names): each
/// removable, then a field to add one. True on a change.
fn hidden_regions(ui: &mut egui::Ui, p: &crate::theme::Palette, s: &mut crate::discord::Settings, typed: &mut String) -> bool {
    use aurora_i18n::tr;
    let mut changed = false;
    ui.vertical(|ui| {
        if s.hidden_regions.is_empty() {
            ui.label(
                egui::RichText::new(tr!("ui-options-discord-hidden-regions-none"))
                    .size(12.0)
                    .color(p.muted),
            );
        }
        let mut removed = None;
        for (i, name) in s.hidden_regions.iter().enumerate() {
            ui.horizontal(|ui| {
                if widgets::icon_button(ui, p, "trash-filled", tr!("common-remove"), true) {
                    removed = Some(i);
                }
                ui.label(egui::RichText::new(name).size(12.5).color(p.ink));
            });
        }
        if let Some(i) = removed {
            s.hidden_regions.remove(i);
            changed = true;
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let field = ui.add(
                egui::TextEdit::singleline(typed)
                    .desired_width(220.0)
                    .hint_text(tr!("ui-options-discord-hidden-regions-add-hint")),
            );
            let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let add = ui
                .add_enabled_ui(!typed.trim().is_empty(), |ui| {
                    widgets::action_button(
                        ui,
                        p,
                        "circle-plus-filled",
                        tr!("common-add"),
                        widgets::ActionKind::Flat,
                        egui::Vec2::new(90.0, 26.0),
                    )
                })
                .inner
                .clicked();
            if (add || entered) && s.hide_region(typed) {
                typed.clear();
                changed = true;
            }
        });
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_address_is_cut_like_the_legacy_viewer() {
        assert_eq!(shown_email("resident@example.com"), "resident@example.com");
        let long = "a.very.long.resident.name@example-provider.com";
        assert_eq!(shown_email(long), "a.very.long.resident.name@exam...");
    }
}
