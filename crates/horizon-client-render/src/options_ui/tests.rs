use super::*;
use crate::extras_ui::ExtraRow;
use crate::gfx::EffectiveOutput;

/// A renderer whose destination offers nothing and whose display reports nothing.
const NONE: EffectiveOutput = EffectiveOutput::NONE;

#[test]
fn pack_options_leave_with_a_change_and_with_nothing_after_its_undo() {
    use crate::settings_shell::Screen as _;
    let mut s = Settings::default();
    let atlas = HudAtlas::build(&crate::resource_source::ResourceSource::directory(
        std::path::PathBuf::from("/nonexistent/horizon-test-assets"),
    ));
    let atlas = atlas.unwrap();
    let mut ui = OptionsUi::default();
    let mut configs = horizon_polytone::pack::Configs::default();
    let birds = serde_json::json!({"default_value": true});
    configs.add_entry("p:birds".into(), &birds);
    ui.pack_configs.insert("pack".into(), configs);
    for undone in [false, true] {
        ui.open_pack_options("pack");
        assert!(ui.shell().is_some(), "Pack Options is on the shell");
        let birds = crate::pack_options_ui::Item::Entry(0);
        ui.pack_options.screen.press(&mut s, birds, false);
        if undone {
            let before = s.clone();
            ui.pack_options.screen.undo(&mut s, &before);
        }
        let events = ui.key(&mut s, &atlas, winit::keyboard::KeyCode::Escape, None);
        let sent = events
            .iter()
            .any(|e| matches!(e, Event::PackOptions { .. }));
        assert_eq!(sent, !undone, "undone: {undone}");
    }
}

/// The window allows the input method only while `typing` says a search
/// box has the focus (`Game::text_field_focused`), so a page without one
/// must not claim the keys: the movement and menu keys keep working there.
#[test]
fn only_a_focused_search_box_takes_text() {
    let mut ui = OptionsUi::default();
    assert!(!ui.typing(), "the main page has no text box");
    ui.show(Page::Language);
    assert!(ui.typing(), "the language list's search takes the keys");
    ui.show(Page::Video);
    assert!(!ui.typing(), "the settings search is not focused");
    ui.settings_search.focused = true;
    assert!(ui.typing(), "the settings search, once focused, does");
    ui.settings_search.focused = false;
    ui.show(Page::Packs);
    assert!(!ui.typing(), "the pack screen's search is not open yet");
}

/// The page layout is made once per size and revision, not per frame,
/// draw, hit test or scroll that changed nothing it reads.
#[test]
fn the_page_layout_is_built_once_per_size_and_revision() {
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    let size = (427.0, 240.0);
    let first = ui.built_layout(&s, size.0, size.1);
    assert_eq!(ui.layouts_built, 1);
    // Frames and hit tests read the built one.
    for _ in 0..3 {
        assert!(std::sync::Arc::ptr_eq(
            &first,
            &ui.built_layout(&s, size.0, size.1)
        ));
    }
    assert_eq!(ui.layouts_built, 1);
    // A click on empty space changed nothing the layout reads.
    let events = ui.click(&mut s, &atlas(), 2.0, 2.0, size, [854, 480], false, false);
    assert!(events.is_empty());
    assert_eq!(ui.layouts_built, 1);
    // A scroll is a revision: one rebuild, then quiet again.
    ui.scroll(&s, size, 1.0);
    ui.built_layout(&s, size.0, size.1);
    assert_eq!(ui.layouts_built, 2);
    ui.built_layout(&s, size.0, size.1);
    assert_eq!(ui.layouts_built, 2);
    // A page change is a revision too.
    ui.show(Page::Video);
    ui.built_layout(&s, size.0, size.1);
    assert_eq!(ui.layouts_built, 3);
    // A resize makes the layout once at the new size.
    ui.built_layout(&s, 854.0, 480.0);
    assert_eq!(ui.layouts_built, 4);
    ui.built_layout(&s, 854.0, 480.0);
    assert_eq!(ui.layouts_built, 4);
    // Search text changes the video page's rows.
    ui.settings_search.focused = true;
    ui.type_text(&atlas(), "abc");
    ui.built_layout(&s, 854.0, 480.0);
    assert_eq!(ui.layouts_built, 5);
}

/// Every operation that changes an input the page layout reads must show
/// in the cached layout: after each step it equals one built fresh from
/// the same state. Covers the wheel and a scrollbar drag, Tab and arrow
/// focus movement that scrolls the list, search typing, clipboard paste
/// and cut, a modifier-only key bind and the Agent page's status.
#[test]
fn the_cached_layout_follows_every_input() {
    use crate::clipboard::{EditShortcut, MemoryClipboard};
    use winit::keyboard::KeyCode as K;

    let atlas = atlas();
    let mut s = Settings::default();
    s.mods.installed.push(crate::mod_list::Installed {
        id: "sample".into(),
        title: "Sample".into(),
        description: String::new(),
        problem: None,
        dev: false,
        native: Some(crate::mod_list::NativePage {
            mappings: vec![crate::mod_list::KeyMapping {
                id: "zoom".into(),
                ..Default::default()
            }],
            ..Default::default()
        }),
    });
    let mut ui = OptionsUi::default();
    ui.open_at(false, Page::Main);
    ui.size = (427.0, 240.0);
    fn fresh(ui: &mut OptionsUi, s: &Settings, step: &str, stale: &mut Vec<String>) {
        let size = ui.size;
        let built = ui.built_layout(s, size.0, size.1);
        let now = ui.layout_now(s, size.0, size.1);
        if *built != now {
            stale.push(step.to_owned());
        }
    }
    let mut stale = Vec::new();

    fresh(&mut ui, &s, "open", &mut stale);
    ui.size = (854.0, 480.0);
    fresh(&mut ui, &s, "resize", &mut stale);

    // A long page: the wheel, a scrollbar drag, Tab and the arrows all
    // move the list under the drawing.
    ui.show(Page::KeyBinds);
    fresh(&mut ui, &s, "Key Binds", &mut stale);
    ui.wheel(&mut s, ui.size, (200.0, 200.0), [1708, 960], -3.0);
    fresh(&mut ui, &s, "wheel scroll", &mut stale);
    let sx = ui.scrollbar_x(ui.size.0);
    ui.scrollbar_drag_y = Some(100.0);
    ui.click(
        &mut s,
        &atlas,
        sx + 1.0,
        120.0,
        ui.size,
        [1708, 960],
        true,
        false,
    );
    fresh(&mut ui, &s, "scrollbar drag", &mut stale);
    for step in 0..60 {
        ui.key(&mut s, &atlas, K::Tab, None);
        fresh(&mut ui, &s, &format!("Tab {step}"), &mut stale);
    }
    for step in 0..40 {
        ui.key(&mut s, &atlas, K::ArrowDown, None);
        fresh(&mut ui, &s, &format!("arrow down {step}"), &mut stale);
    }

    // Search typing, then the clipboard shortcuts in the same box.
    ui.show(Page::Video);
    ui.settings_search.focused = true;
    for (step, c) in ["a", "b", "c"].iter().enumerate() {
        ui.key(&mut s, &atlas, K::KeyA, Some(c));
        fresh(&mut ui, &s, &format!("typing {step}"), &mut stale);
    }
    // A query that matches options: the rows change with the text.
    let mut clipboard = MemoryClipboard("render".into());
    ui.shortcut(&atlas, EditShortcut::SelectAll, &mut clipboard);
    fresh(&mut ui, &s, "select all", &mut stale);
    ui.shortcut(&atlas, EditShortcut::Paste, &mut clipboard);
    assert_eq!(
        ui.settings_search.value, "render",
        "the clipboard went into the box"
    );
    fresh(&mut ui, &s, "paste", &mut stale);
    ui.shortcut(&atlas, EditShortcut::SelectAll, &mut clipboard);
    ui.shortcut(&atlas, EditShortcut::Cut, &mut clipboard);
    assert!(ui.settings_search.value.is_empty());
    fresh(&mut ui, &s, "cut", &mut stale);

    ui.languages = languages();
    ui.show(Page::Language);
    fresh(&mut ui, &s, "language open", &mut stale);
    let (widgets, top, bottom, _) = ui.layout(&s, ui.size.0, ui.size.1);
    let row = widgets
        .iter()
        .find(|w| {
            matches!(
                &w.kind,
                WKind::LanguageRow {
                    selected: false,
                    ..
                }
            ) && w.y >= top
                && w.y + w.h <= bottom
        })
        .unwrap();
    let WKind::LanguageRow { code, .. } = &row.kind else {
        unreachable!()
    };
    let code = code.clone();
    ui.click(
        &mut s,
        &atlas,
        row.x + 1.0,
        row.y + 1.0,
        ui.size,
        [1708, 960],
        false,
        false,
    );
    fresh(&mut ui, &s, "language selection", &mut stale);
    assert!(ui.built_layout(&s, ui.size.0, ui.size.1).parts().0.iter().any(|w|
        matches!(&w.kind, WKind::LanguageRow { code: selected, selected: true, .. } if selected == &code)));

    // A modifier-only key bind takes the waiting selection away.
    ui.show(Page::KeyBinds);
    ui.selected_key = Some(MOD_KEY);
    ui.selected_mod_key = Some((0, 0));
    ui.pending_mod_key = Some(K::ShiftLeft);
    fresh(&mut ui, &s, "waiting for a mod key", &mut stale);
    ui.key_released(&mut s, K::ShiftLeft);
    fresh(&mut ui, &s, "modifier-only bind", &mut stale);

    // The Agent page's status and recording, as the game's poll sets them.
    ui.show(Page::Agent);
    fresh(&mut ui, &s, "agent page", &mut stale);
    ui.set_agent_status(vec!["Running.".into()], true);
    fresh(&mut ui, &s, "agent status", &mut stale);

    // The language page's search click moves the list back to the top.
    ui.show(Page::Language);
    ui.wheel(&mut s, ui.size, (200.0, 200.0), [1708, 960], -3.0);
    fresh(&mut ui, &s, "language wheel", &mut stale);
    let (x, y) = (ui.language_search.x, ui.language_search.y);
    ui.click(
        &mut s,
        &atlas,
        x + 5.0,
        y + 5.0,
        ui.size,
        [1708, 960],
        false,
        false,
    );
    fresh(&mut ui, &s, "language search click", &mut stale);

    assert!(
        stale.is_empty(),
        "the cached layout is stale after: {stale:?}"
    );
}

#[test]
fn accessibility_sliders_and_toggles_survive_options_round_trip() {
    let mut s = Settings::default();
    for opt in [
        Opt::FovEffects,
        Opt::DarknessPulsing,
        Opt::DamageTilt,
        Opt::PanoramaSpeed,
    ] {
        for position in [0.0, 0.5, 1.0] {
            opt.set_slider(&mut s, position);
            let loaded = Settings::parse(&s.serialize());
            assert_eq!(opt.slider_position(&loaded), Some(position));
            let expected = if matches!(opt, Opt::FovEffects | Opt::DarknessPulsing) {
                f64::from(position).powi(2)
            } else {
                f64::from(position)
            };
            assert_eq!(loaded.file.double(opt.file_key().unwrap()), expected);
        }
    }
    assert!(Opt::MonochromeLogo.available());
    for opt in [
        Opt::TextBackground,
        Opt::HideLightningFlashes,
        Opt::HideSplashTexts,
        Opt::MonochromeLogo,
    ] {
        let key = opt.file_key().unwrap();
        let before = s.file.bool(key);
        opt.cycle(&mut s, false, [854, 480]);
        assert_eq!(Settings::parse(&s.serialize()).file.bool(key), !before);
    }
    Opt::TextBackground.cycle(&mut s, false, [854, 480]);
    let chat_only = s.text_background(0.0);
    Opt::TextBackground.cycle(&mut s, false, [854, 480]);
    assert_ne!(s.text_background(0.0), chat_only);
}

#[test]
fn mouse_settings_match_java_26_3_option_order() {
    let mut ui = OptionsUi::default();
    ui.show(Page::Mouse);
    let mut options = Vec::new();
    for row in ui.list_rows(&Settings::default()) {
        if let Row::Pair(a, b) = row {
            options.push(a);
            if let Some(b) = b {
                options.push(b);
            }
        }
    }
    assert_eq!(
        options,
        vec![
            Opt::Sensitivity,
            Opt::WheelSensitivity,
            Opt::DiscreteScroll,
            Opt::InvertX,
            Opt::InvertY,
            Opt::AllowCursorChanges,
        ]
    );
}

#[test]
fn mouse_settings_round_trip_with_java_defaults() {
    let mut settings = Settings::default();
    assert!(!settings.invert_x);
    assert!(settings.allow_cursor_changes);
    Opt::InvertX.cycle(&mut settings, false, [854, 480]);
    Opt::AllowCursorChanges.cycle(&mut settings, false, [854, 480]);
    let loaded = Settings::parse(&settings.serialize());
    assert!(loaded.invert_x);
    assert!(!loaded.allow_cursor_changes);
    assert!(!loaded.file.bool("allowCursorChanges"));
}

#[test]
fn accessibility_has_all_options_and_disabled_placeholders() {
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::Accessibility);
    let (widgets, _, _, _) = ui.layout(&s, 480.0, 1200.0);
    let options: Vec<_> = widgets
        .iter()
        .filter_map(|w| match w.kind {
            WKind::Button {
                act: Act::Cycle(opt),
                ..
            }
            | WKind::Slider { opt, .. } => Some(opt),
            _ => None,
        })
        .collect();
    assert_eq!(options, ACCESSIBILITY_OPTIONS);
    for &opt in ACCESSIBILITY_OPTIONS {
        let widget = widgets
            .iter()
            .find(|w| match w.kind {
                WKind::Button {
                    act: Act::Cycle(o), ..
                }
                | WKind::Slider { opt: o, .. } => o == opt,
                _ => false,
            })
            .expect("every accessibility option has a control");
        if !opt.available() {
            assert!(matches!(widget.kind, WKind::Button { active: false, .. }));
            let before = s.serialize();
            opt.cycle(&mut s, false, [854, 480]);
            opt.set_slider(&mut s, 0.0);
            assert_eq!(s.serialize(), before);
        }
    }
    assert!(widgets.iter().any(|w| matches!(
        w.kind,
        WKind::Button {
            act: Act::Push(Page::Controls),
            active: true,
            ..
        }
    )));
    // `AccessibilityOptionsScreen.addFooter`: the "Accessibility Help"
    // link button sits beside Done.
    assert!(widgets.iter().any(|w| matches!(
        w.kind,
        WKind::Button {
            act: Act::AccessibilityHelp,
            active: true,
            ..
        }
    )));
}

#[test]
fn the_narrator_option_cycles_its_four_statuses_and_announces_them() {
    use horizon_narrator::Status;
    let mut s = Settings::default();
    for want in [Status::All, Status::Chat, Status::System, Status::Off] {
        Opt::Narrator.cycle(&mut s, false, [854, 480]);
        assert_eq!(s.narrator(), want);
    }
    Opt::Narrator.cycle(&mut s, true, [854, 480]);
    assert_eq!(s.narrator(), Status::System);
    let mut ui = OptionsUi::default();
    assert_eq!(
        ui.cycle_option(&mut s, Opt::Narrator, false, [854, 480]),
        [Event::Changed, Event::Narrator]
    );
    assert_eq!(
        ui.cycle_option(&mut s, Opt::NarratorHotkey, false, [854, 480]),
        [Event::Changed]
    );
    assert!(!s.file.bool("narratorHotkey"));
}

#[test]
fn without_speech_the_narrator_button_is_inactive_and_not_available() {
    // Tests never open the narrator: text2speech's Narrator.EMPTY.
    let s = Settings::default();
    let widget = option_widget(&s, Opt::Narrator, 0.0, 0.0, 150.0);
    assert!(matches!(widget.kind, WKind::Button { active: false, .. }));
    assert_eq!(
        Opt::Narrator.value_text(&s).plain(),
        tr("options.narrator.notavailable").plain()
    );
    let hotkey = option_widget(&s, Opt::NarratorHotkey, 0.0, 0.0, 150.0);
    assert!(matches!(hotkey.kind, WKind::Button { active: true, .. }));
}

#[test]
fn chat_settings_show_the_narrator_where_vanilla_does() {
    let mut ui = OptionsUi::default();
    ui.show(Page::Chat);
    let options: Vec<Opt> = ui
        .list_rows(&Settings::default())
        .into_iter()
        .flat_map(|row| match row {
            Row::Big(opt) => vec![opt],
            Row::Pair(a, b) => std::iter::once(a).chain(b).collect(),
            _ => Vec::new(),
        })
        .collect();
    let at = options.iter().position(|o| *o == Opt::Narrator).unwrap();
    assert_eq!(options[at - 1], Opt::ChatHeightUnfocused);
    assert_eq!(options[at + 1], Opt::AutoSuggestions);
}

#[test]
fn controls_match_java_26_3_option_order() {
    let mut ui = OptionsUi::default();
    ui.show(Page::Controls);
    let options =
        ui.list_rows(&Settings::default())
            .into_iter()
            .fold(Vec::new(), |mut options, row| {
                match row {
                    Row::Big(opt) => options.push(opt),
                    Row::Pair(a, b) => {
                        options.push(a);
                        options.extend(b);
                    }
                    _ => {}
                }
                options
            });
    assert_eq!(options, vanilla_control_options());
}

#[test]
fn sprint_window_and_new_control_switches_round_trip() {
    let mut settings = Settings::default();
    set_slider(Opt::SprintWindow, &mut settings, 0.5);
    settings.operator_items_tab = true;
    settings.quit_shortcuts = false;
    settings.ctrl_click_emulates_right_click = true;
    settings.toggle_attack = true;
    settings.toggle_use = true;
    let loaded = Settings::parse(&settings.serialize());
    assert_eq!(loaded.sprint_window, 5);
    assert!(loaded.operator_items_tab);
    assert!(!loaded.quit_shortcuts);
    assert!(loaded.ctrl_click_emulates_right_click);
    assert!(loaded.toggle_attack && loaded.toggle_use);
}

#[test]
fn quit_shortcuts_option_can_be_switched() {
    assert!(Opt::QuitShortcuts.available());
    let s = Settings::default();
    assert_eq!(Opt::QuitShortcuts.caption(), "options.quitShortcuts");
    assert_eq!(
        Opt::QuitShortcuts.tooltip_key(&s),
        "options.quitShortcuts.tooltip"
    );
}

/// The gating is for genuinely unread keys only: every option a system
/// already consumes must stay clickable.
#[test]
fn read_options_are_not_greyed_out() {
    for opt in [
        Opt::ChunkFade,
        Opt::ImprovedTransparency,
        Opt::ChatDelay,
        Opt::SaveChatDrafts,
        Opt::MusicToast,
        Opt::CtrlClickEmulatesRightClick,
    ] {
        assert!(opt.available(), "{opt:?} is read and must be clickable");
    }
    for opt in [
        Opt::HideMatchedNames,
        Opt::OnlySecureChat,
        Opt::RealmsNotifications,
        Opt::InGameNotification,
        Opt::SharePresence,
        Opt::DirectionalAudio,
        Opt::ChunkBuilder,
    ] {
        assert!(!opt.available(), "{opt:?} still has no reader");
    }
}

#[test]
fn operator_items_tab_option_can_be_switched() {
    // The creative screen reads it (CreativeModeInventoryScreen.hasPermissions).
    assert!(Opt::OperatorItemsTab.available());
    let mut s = Settings::default();
    assert!(!s.operator_items_tab);
    Opt::OperatorItemsTab.cycle(&mut s, false, [854, 480]);
    assert!(s.operator_items_tab);
}

#[test]
fn render_distance_slider_round_trips_through_extended_options_range() {
    let mut s = Settings::default();
    for (position, distance) in [(0.0, 2), (0.5, 65), (1.0, 128)] {
        set_slider(Opt::RenderDistance, &mut s, position);
        assert_eq!(s.render_distance, distance);
        let reloaded = Settings::parse(&s.serialize());
        assert_eq!(reloaded.render_distance, distance);
        assert_eq!(
            Opt::RenderDistance.slider_position(&reloaded),
            Some(position)
        );
    }
    let mut file = crate::options_file::OptionsFile::default();
    file.set_text("renderDistance", "128").unwrap();
    assert!(file.set_text("renderDistance", "129").is_err());
    assert!(file.set_text("renderDistance", "1").is_err());
    assert_eq!(file.int("renderDistance"), 128);
}
pub(super) fn atlas() -> HudAtlas {
    HudAtlas::build(&horizon_assets::resource_source::ResourceSource::directory(
        (std::path::Path::new("/nonexistent/horizon-test-assets")).to_owned(),
    ))
    .unwrap()
}

/// Picking another pack keeps the last one's changed options before the next one's are
/// read, and leaving the shader screen applies them.
#[test]
fn the_shader_screen_saves_changed_options_before_reading_the_next_packs() {
    use crate::shader_pack_list::{Entry, Kind};
    let entry = |file: &str| Entry {
        file: file.into(),
        key: file.into(),
        title: file.into(),
        kind: Kind::Classic,
        options: Vec::new(),
        values: Vec::new(),
        pages: Vec::new(),
        traces: false,
    };
    let mut s = Settings {
        shader_packs: vec![entry("A.zip"), entry("B.zip")],
        ..Settings::default()
    };
    s.horizon.shader_pack = "A.zip".into();
    let mut options = crate::shader_options::ShaderOptions::new(
        "A.zip".into(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
    );
    options.changed = true;
    s.shader_options = Some(options);
    let mut ui = OptionsUi::default();
    ui.open(false);
    ui.push(Page::ShaderPacks);
    let size = (854.0, 480.0);
    let (x, y) = ui
        .shader
        .point_at(&s, size, crate::shader_ui::Item::Pack(1));
    let events = ui.click(&mut s, &atlas(), x, y, size, [1708, 960], false, false);
    assert!(matches!(
        events[..],
        [
            Event::ButtonClick,
            Event::ShaderOptions,
            Event::ShaderPack,
            Event::OpenShaderOptions
        ]
    ));
    assert_eq!(s.horizon.shader_pack, "B.zip");
    let events = ui.back(&mut s);
    assert!(matches!(events[0], Event::ShaderOptions));
}

#[test]
fn resource_reload_error_blocks_options_until_dismissed() {
    use winit::keyboard::KeyCode;
    let atlas = atlas();
    let mut settings = Settings::default();
    let mut ui = OptionsUi {
        resource_error: Some("broken pack".into()),
        ..Default::default()
    };
    let stack = ui.stack.clone();
    assert!(
        ui.click(
            &mut settings,
            &atlas,
            10.0,
            10.0,
            (480.0, 270.0),
            [960, 540],
            false,
            false
        )
        .is_empty()
    );
    assert!(ui.resource_error.is_some());
    assert!(
        ui.key(&mut settings, &atlas, KeyCode::KeyW, None)
            .is_empty()
    );
    assert!(ui.resource_error.is_some());
    ui.key(&mut settings, &atlas, KeyCode::Escape, None);
    assert!(ui.resource_error.is_none());
    assert_eq!(ui.stack, stack);
    ui.resource_error = Some("broken pack".into());
    assert_eq!(
        ui.click(
            &mut settings,
            &atlas,
            240.0,
            250.0,
            (480.0, 270.0),
            [960, 540],
            false,
            false
        ),
        vec![Event::ButtonClick]
    );
    assert!(ui.resource_error.is_none());
}

/// The Extras page joins caption and value with the language's `options.generic_value`
/// (26.3's templates), not an English ": " (#6072).
#[test]
fn extras_captions_use_the_languages_separator() {
    struct Vanilla;
    impl lang::ResourceSource for Vanilla {
        fn read(&self, path: &str) -> Option<Vec<u8>> {
            let generic = match path {
                "assets/minecraft/lang/fr_fr.json" => "%s : %s",
                "assets/minecraft/lang/zh_cn.json" => "%s：%s",
                _ => return None,
            };
            serde_json::to_vec(&serde_json::json!({ "options.generic_value": generic })).ok()
        }
    }
    let s = Settings::default();
    let ui = OptionsUi::default();
    for (code, shown) in [("fr_fr", "Sans IA : Non"), ("zh_cn", "关闭 AI：关")] {
        let (label, _) = lang::scoped(lang::Language::load(&[&Vanilla], code), || {
            match ui.extra_button(&s, crate::extras::Item::NoAi).kind {
                WKind::Button { label, .. } => label.plain(),
                _ => String::new(),
            }
        });
        assert_eq!(label, shown, "{code}");
    }
}

/// The texts of an options page, as laid out in `code`'s language, and the keys it lacked.
fn page_texts(code: &str, pages: &[Page]) -> (Vec<String>, Vec<String>) {
    let s = Settings::default();
    lang::scoped(lang::Language::load(&[], code), || {
        let mut texts = Vec::new();
        for page in pages {
            let mut ui = OptionsUi::default();
            ui.show(*page);
            for w in ui.layout(&s, 480.0, 4000.0).0 {
                match w.kind {
                    WKind::Button { label: t, .. }
                    | WKind::Slider { label: t, .. }
                    | WKind::Text { text: t, .. } => texts.push(t.plain()),
                    _ => {}
                }
            }
            if ui.controller_open() {
                texts.extend(ui.controller.row_names(&s));
            }
            if ui.page() == Page::Extras {
                texts.extend(ui.extras.row_labels(&s));
            }
        }
        texts
    })
}

/// Horizon's own options pages said "Splitscreen...", "Hide block-break particles" and
/// "Hotbar: Previous Slot" in every language (#1927).
#[test]
fn horizon_option_pages_follow_the_language() {
    let (texts, missed) = page_texts("de_de", &[Page::Horizon, Page::Extras, Page::PadBinds]);
    for want in [
        "Splitscreen …",
        "Weicher Chat: Aus",
        "Nachrichtenmarkierung: An",
        "Kanäle für UI-Töne: Aus",
        "Schnellzugriffsleiste: Vorheriger Platz",
        "Schnellzugriffsleiste: Nächster Platz",
    ] {
        assert!(texts.iter().any(|t| t == want), "{want:?} not in {texts:?}");
    }
    // Every key these pages ask for is in the table; vanilla's own keys (`options.*`,
    // `key.*`: dotted ids without spaces) come from the prepared resources, which this
    // language does not have.
    let missed: Vec<_> = missed
        .into_iter()
        .filter(|k| k.starts_with("horizon.") || k.contains(' ') || !k.contains('.'))
        .collect();
    assert_eq!(missed, Vec::<String>::new());
    let (texts, _) = page_texts("pl_pl", &[Page::Extras, Page::PadBinds]);
    for want in [
        "Płynny czat: Wył.",
        "Pasek szybkiego wyboru: poprzednie pole",
    ] {
        assert!(texts.iter().any(|t| t == want), "{want:?} not in {texts:?}");
    }
}

/// The Mods page is NeoForge's list screen: no switches, a footer of folder and
/// Back, and Config on the selected native mod. Positions are the layout at 480x240:
/// 150 px buttons 4 px apart, centred 27 px above the bottom; the sidebar at x 41 with its
/// list at y 32 and 28 px rows; the panel at x 188 with its buttons at y 182, 84 px apart.
#[test]
fn the_mods_page_opens_the_folder_and_a_native_mods_settings() {
    let atlas = atlas();
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    let size = (480.0, 240.0);
    assert!(ui.layout(&s, size.0, size.1).0.iter().any(|w| {
        matches!(
            w.kind,
            WKind::Button {
                active: true,
                act: Act::Push(Page::Mods),
                ..
            }
        )
    }));
    s.mods.installed.push(crate::mod_list::Installed {
        id: "native".into(),
        title: "Native 1.0".into(),
        description: String::new(),
        problem: None,
        dev: false,
        native: Some(crate::mod_list::NativePage {
            settings: vec![crate::mod_list::NativeSetting {
                key: "trails".into(),
                label: "Trails".into(),
                tooltip: None,
                value: crate::mod_list::NativeSettingValue::Toggle(true),
            }],
            ..Default::default()
        }),
    });
    let click = |ui: &mut OptionsUi, s: &mut Settings, x: f32, y: f32| {
        ui.click(s, &atlas, x, y, size, [480, 240], false, false)
    };
    ui.show(Page::Mods);
    assert_eq!(
        click(&mut ui, &mut s, 160.0, 223.0),
        vec![Event::ButtonClick, Event::OpenModFolder]
    );
    // Config does nothing until a native mod with settings is picked from the list
    // (Horizon, Minecraft, then Native).
    assert!(click(&mut ui, &mut s, 397.0, 192.0).is_empty());
    assert!(click(&mut ui, &mut s, 61.0, 104.0).is_empty());
    let events = click(&mut ui, &mut s, 397.0, 192.0);
    // Its labels were made with the list, maybe in another language.
    assert_eq!(events, vec![Event::ButtonClick, Event::NativeSettings(0)]);
    assert_eq!(ui.page(), Page::NativeMod(0));
    ui.back(&mut s);
    assert_eq!(ui.page(), Page::Mods);
    // Back leaves the page.
    let events = click(&mut ui, &mut s, 320.0, 223.0);
    assert_eq!(events.first(), Some(&Event::ButtonClick));
    assert_eq!(ui.page(), Page::Main);
}

#[test]
fn a_mod_page_draws_its_settings_and_a_change_reaches_the_game() {
    use crate::mod_list::{NativeSetting, NativeSettingValue as V};
    let atlas = atlas();
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    let size = (480.0, 240.0);
    let setting = |key: &str, value: V| NativeSetting {
        key: key.into(),
        label: key.into(),
        tooltip: None,
        value,
    };
    s.mods.installed.push(crate::mod_list::Installed {
        id: "native".into(),
        title: "Native 1.0".into(),
        description: String::new(),
        problem: None,
        dev: false,
        native: Some(crate::mod_list::NativePage {
            settings: vec![
                setting("trails", V::Toggle(true)),
                setting(
                    "speed",
                    V::Range {
                        min: 0.0,
                        max: 4.0,
                        step: 1.0,
                        value: 1.0,
                    },
                ),
            ],
            ..Default::default()
        }),
    });
    ui.show(Page::NativeMod(0));
    let (widgets, ..) = ui.layout(&s, size.0, size.1);
    let toggle = widgets
        .iter()
        .find(|w| {
            matches!(
                w.kind,
                WKind::Button {
                    act: Act::NativeSetting(0, 0),
                    ..
                }
            )
        })
        .expect("a toggle is a button");
    let slider = widgets
        .iter()
        .find(|w| {
            matches!(
                w.kind,
                WKind::Slider {
                    opt: Opt::ModSetting(0, 1),
                    ..
                }
            )
        })
        .expect("a range is a slider");
    let (slider_x, slider_y, slider_w) = (slider.x, slider.y, slider.w);
    let events = ui.click(
        &mut s,
        &atlas,
        toggle.x + 1.0,
        toggle.y + 1.0,
        size,
        [480, 240],
        false,
        false,
    );
    assert!(events.contains(&Event::NativeSetting(0, 0)), "{events:?}");
    let page = |s: &Settings| s.mods.installed[0].native.clone().unwrap();
    assert_eq!(page(&s).settings[0].value, V::Toggle(false));
    let events = ui.click(
        &mut s,
        &atlas,
        slider_x + slider_w - 1.0,
        slider_y + 1.0,
        size,
        [480, 240],
        false,
        false,
    );
    assert!(events.contains(&Event::NativeSetting(0, 1)), "{events:?}");
    assert_eq!(
        page(&s).settings[1].value,
        V::Range {
            min: 0.0,
            max: 4.0,
            step: 1.0,
            value: 4.0
        }
    );
}

/// Typing goes to the Mods search box once a click focuses it; the one match is selected,
/// so Homepage opens that installed mod's declared link without product-global state.
#[test]
fn the_mods_search_takes_the_typing_after_a_click() {
    let atlas = atlas();
    let mut s = Settings::default();
    s.mods.installed.push(crate::mod_list::Installed {
        id: "search_fixture".into(),
        title: "Search fixture".into(),
        description: String::new(),
        problem: None,
        dev: false,
        native: None,
    });
    s.mods.info.insert(
        "search_fixture".into(),
        crate::mod_list::ModInfo {
            name: "Search fixture".into(),
            homepage: Some("https://example.org/search".into()),
            ..Default::default()
        },
    );
    let mut ui = OptionsUi::default();
    let size = (480.0, 240.0);
    ui.show(Page::Mods);
    assert!(!ui.typing());
    ui.click(&mut s, &atlas, 60.0, 16.0, size, [480, 240], false, false);
    assert!(ui.typing());
    ui.type_text(&atlas, "fixture");
    let events = ui.click(&mut s, &atlas, 229.0, 192.0, size, [480, 240], false, false);
    assert_eq!(
        events,
        vec![
            Event::ButtonClick,
            Event::OpenLink("https://example.org/search".into())
        ]
    );
    assert!(!ui.typing());
}

#[test]
fn mod_mapping_labels_and_category_search_use_translations() {
    let mut settings = Settings::default();
    settings.mods.installed.push(crate::mod_list::Installed {
        id: "sample".into(),
        title: "Sample".into(),
        description: String::new(),
        problem: None,
        dev: false,
        native: Some(crate::mod_list::NativePage {
            mappings: vec![crate::mod_list::KeyMapping {
                id: "zoom".into(),
                label: "controls.reset".into(),
                category: "controls.keybinds.title".into(),
                key: "key.keyboard.z".into(),
                ..Default::default()
            }],
            ..Default::default()
        }),
    });
    let mut ui = OptionsUi::default();
    ui.show(Page::KeyBinds);
    for (key, bound) in [("key.keyboard.z", true), ("key.keyboard.unknown", false)] {
        settings.mods.set_binding("sample", "zoom", key, 0);
        for act in [Act::ModKey(0, 0), Act::ResetModKey(0, 0)] {
            let reset = matches!(act, Act::ResetModKey(..));
            let widget = button(0.0, 0.0, 75.0, Text::literal("Z"), act);
            let mut narration = crate::widgets::Narratable::button("", [0.0; 4], true);
            narrate_widget(&mut narration, &settings, &widget, false, 0, [480, 240]);
            let name = tr("controls.reset");
            let expected = if reset {
                Text::translate("narrator.controls.reset", &[name])
            } else if bound {
                Text::translate(
                    "narrator.controls.bound",
                    &[
                        name,
                        Text::translate("gui.narrate.button", &[Text::literal("Z")]),
                    ],
                )
            } else {
                Text::translate("narrator.controls.unbound", &[name])
            };
            assert_eq!(
                narration.what,
                crate::widgets::What::Named {
                    title: expected.plain(),
                    active: true,
                }
            );
            assert_eq!(
                narration.hint,
                tooltip_text(&settings, &widget).map(|tip| tip.plain())
            );
        }
    }
    for query in [
        lang::get("controls.reset"),
        lang::get("controls.keybinds.title"),
    ] {
        ui.settings_search.set_value(&query);
        let (widgets, ..) = ui.layout(&settings, 480.0, 240.0);
        assert!(widgets.iter().any(|widget| matches!(
            widget.kind,
            WKind::Button {
                act: Act::ModKey(0, 0),
                ..
            }
        )));
        assert!(widgets.iter().any(|widget| matches!(
            &widget.kind,
            WKind::Text { text, .. } if text.plain() == lang::get("controls.reset")
        )));
    }
}

#[test]
fn conflicts_include_vanilla_mods_and_contexts() {
    use crate::mod_list::{Installed, KeyConflictContext as Context, KeyMapping, NativePage};
    let mut settings = Settings::default();
    for id in ["first", "second"] {
        settings.mods.installed.push(Installed {
            id: id.into(),
            title: id.into(),
            description: String::new(),
            problem: None,
            dev: false,
            native: Some(NativePage {
                mappings: vec![KeyMapping {
                    id: "zoom".into(),
                    label: "key.zoom".into(),
                    key: "key.keyboard.z".into(),
                    context: Context::InGame,
                    ..Default::default()
                }],
                ..Default::default()
            }),
        });
    }
    settings
        .mods
        .set_binding("first", "zoom", "key.keyboard.w", 0);
    assert!(
        !key_conflicts(
            &settings,
            "first",
            "zoom",
            "key.keyboard.w",
            0,
            Context::InGame,
            false
        )
        .is_empty()
    );
    assert!(
        !key_conflicts(
            &settings,
            "",
            "key.forward",
            "key.keyboard.w",
            0,
            Context::Universal,
            true
        )
        .is_empty()
    );
    settings
        .mods
        .set_binding("first", "zoom", "key.keyboard.f24", 0);
    settings
        .mods
        .set_binding("second", "zoom", "key.keyboard.f24", 0);
    assert_eq!(
        key_conflicts(
            &settings,
            "first",
            "zoom",
            "key.keyboard.f24",
            0,
            Context::InGame,
            false
        ),
        [lang::get("key.zoom")]
    );
    assert!(
        key_conflicts(
            &settings,
            "first",
            "zoom",
            "key.keyboard.f24",
            0,
            Context::Gui,
            false
        )
        .is_empty()
    );
    assert!(
        key_conflicts(
            &settings,
            "first",
            "zoom",
            "key.keyboard.f24",
            1,
            Context::InGame,
            false
        )
        .is_empty()
    );
    assert!(
        key_conflicts(
            &settings,
            "first",
            "zoom",
            "key.keyboard.unknown",
            0,
            Context::Universal,
            false
        )
        .is_empty()
    );
}

#[test]
fn each_options_renderer_emits_one_phase_pair_around_contents() {
    use crate::screen_draw::RenderPhase::{Background, Foreground};
    let atlas = atlas();
    let settings = Settings::default();
    for page in [
        Page::Main,
        Page::Video,
        Page::Language,
        Page::Graphics,
        Page::Controller,
        Page::ControllerRadial,
        Page::Extras,
        Page::ShaderOptions,
        Page::PackOptions,
        Page::Mods,
        Page::Packs,
        Page::SplitSetup,
        Page::SplitProfiles,
        Page::Studio,
    ] {
        let mut ui = OptionsUi::default();
        ui.show(page);
        let mut batch = HudBatch::scratch([854, 480]);
        let mut calls = Vec::new();
        // A cancelled frame must release the hosted shell's prepared view too.
        ui.draw_background(&settings, NONE, &mut batch, &atlas);
        ui.discard_draw();
        batch = HudBatch::scratch([854, 480]);
        ui.draw_test_frame(
            &settings,
            NONE,
            &mut batch,
            &atlas,
            (-1.0, -1.0),
            &mut |phase, batch| calls.push((phase, batch.vertices.len())),
        );
        assert_eq!(
            calls.iter().map(|(phase, _)| *phase).collect::<Vec<_>>(),
            [Background, Foreground],
            "{page:?}",
        );
        assert!(calls[0].1 > 0, "{page:?}: background painted first");
        assert!(calls[1].1 > calls[0].1, "{page:?}: contents between phases");
        assert_eq!(calls[1].1, batch.vertices.len(), "{page:?}: no tooltip");
    }
}

#[test]
fn mod_key_capture_waits_for_a_chord_or_modifier_release() {
    use winit::keyboard::KeyCode as K;
    let atlas = atlas();
    let mut settings = Settings::default();
    settings.mods.installed.push(crate::mod_list::Installed {
        id: "sample".into(),
        title: "Sample".into(),
        description: String::new(),
        problem: None,
        dev: false,
        native: Some(crate::mod_list::NativePage {
            mappings: vec![crate::mod_list::KeyMapping {
                id: "zoom".into(),
                ..Default::default()
            }],
            ..Default::default()
        }),
    });
    let mut ui = OptionsUi::default();
    for (keys, modifiers, mouse, expected) in [
        (
            vec![K::ShiftLeft, K::KeyZ],
            1,
            false,
            "shift+key.keyboard.z",
        ),
        (
            vec![K::ControlLeft, K::AltLeft, K::KeyZ],
            6,
            false,
            "ctrl+alt+key.keyboard.z",
        ),
        (vec![K::ShiftLeft], 1, true, "shift+key.mouse.right"),
        (vec![K::ShiftLeft], 1, false, "key.keyboard.left.shift"),
    ] {
        ui.selected_key = Some(MOD_KEY);
        ui.selected_mod_key = Some((0, 0));
        ui.modifiers = modifiers;
        for key in &keys {
            ui.key(&mut settings, &atlas, *key, None);
        }
        if mouse {
            ui.bind_mouse(&mut settings, winit::event::MouseButton::Right);
        } else if keys.len() == 1 {
            ui.key_released(&mut settings, keys[0]);
        }
        assert_eq!(settings.mods.keys["sample"]["zoom"], expected);
        assert_eq!(ui.selected_key, None);
    }
}

#[test]
fn a_native_mods_key_shows_and_keeps_the_key_pressed() {
    // #3539: Zoom's key set to R showed another key.
    let atlas = atlas();
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    let size = (480.0, 240.0);
    s.mods.installed.push(crate::mod_list::Installed {
        id: "zoom".into(),
        title: "Zoom 2.0.0".into(),
        description: String::new(),
        problem: None,
        dev: false,
        native: Some(crate::mod_list::NativePage {
            mappings: vec![crate::mod_list::KeyMapping {
                id: "zoom".into(),
                label: "key.zoom".into(),
                key: "key.keyboard.z".into(),
                ..Default::default()
            }],
            settings: Vec::new(),
        }),
    });
    ui.show(Page::KeyBinds);
    ui.scroll(&s, size, -10_000.0);
    let key_label = |ui: &mut OptionsUi, s: &Settings| {
        let (widgets, ..) = ui.layout(s, size.0, size.1);
        widgets.into_iter().find_map(|w| match w.kind {
            WKind::Button {
                label,
                act: Act::ModKey(0, 0),
                ..
            } => Some((label.plain(), w.x, w.y)),
            _ => None,
        })
    };
    let (label, x, y) = key_label(&mut ui, &s).expect("the key's row");
    let (_, top, bottom, _) = ui.layout(&s, size.0, size.1);
    assert!(y >= top && y + 20.0 <= bottom);
    assert_eq!(label, "Z");
    ui.click(
        &mut s,
        &atlas,
        x + 1.0,
        y + 1.0,
        size,
        [480, 240],
        false,
        false,
    );
    assert_eq!(ui.selected_key, Some(MOD_KEY));
    assert_eq!(
        ui.key(&mut s, &atlas, winit::keyboard::KeyCode::KeyR, Some("r")),
        vec![Event::Changed]
    );
    assert_eq!(key_label(&mut ui, &s).unwrap().0, "R");
    let saved = crate::mod_list::ModList::parse(&s.mods.serialize());
    assert_eq!(saved.keys_of("zoom"), vec![("zoom", "key.keyboard.r")]);
}

fn mode(width: i32, rate: f32) -> crate::window::VideoMode {
    crate::window::VideoMode {
        width,
        height: width * 9 / 16,
        red_bits: 8,
        green_bits: 8,
        blue_bits: 8,
        refresh_rate: rate,
    }
}

#[test]
fn video_page_lists_vanilla_display_and_quality_options_in_order() {
    let mut ui = OptionsUi::default();
    ui.show(Page::Video);
    let rows = ui.list_rows(&Settings::default());
    let opts: Vec<Opt> = rows
        .iter()
        .flat_map(|r| match r {
            Row::Big(o) => vec![*o],
            Row::Pair(a, b) => std::iter::once(*a).chain(*b).collect(),
            _ => Vec::new(),
        })
        .collect();
    // VideoSettingsScreen.addOptions: the exclusive mode row, displayOptions, the preset,
    // qualityOptions, then the preferences.
    assert_eq!(
        opts[..9],
        [
            Opt::ExclusiveMode,
            Opt::MaxFps,
            Opt::Vsync,
            Opt::InactivityFps,
            Opt::GuiScale,
            Opt::Fullscreen,
            Opt::ExclusiveFullscreen,
            Opt::Gamma,
            Opt::GraphicsBackend,
        ]
    );
    let at = |o| opts.iter().position(|x| *x == o).unwrap();
    assert_eq!(at(Opt::ImprovedTransparency) + 1, at(Opt::TextureFiltering));
    assert_eq!(at(Opt::TextureFiltering) + 1, at(Opt::MaxAnisotropy));
    assert_eq!(at(Opt::MaxAnisotropy) + 1, at(Opt::WeatherRadius));
}

#[test]
fn exclusive_fullscreen_mode_picks_a_monitor_mode_or_the_current_one() {
    let mut s = Settings::default();
    // No monitor mode: the slider changes nothing.
    Opt::ExclusiveMode.set_slider(&mut s, 1.0);
    assert_eq!(s.file.fullscreen_resolution, None);
    s.video_modes = vec![mode(1280, 60.0), mode(1920, 144.0), mode(2560, 59.94)];
    assert_eq!(s.video_mode_index(), -1);
    // IntRange(-1, 2): the slider's thirds are -1, 0, 1, 2.
    Opt::ExclusiveMode.set_slider(&mut s, 2.0 / 3.0);
    assert_eq!(s.video_mode_index(), 1);
    assert_eq!(Opt::ExclusiveMode.slider_position(&s), Some(2.0 / 3.0));
    Opt::ExclusiveMode.set_slider(&mut s, 1.0);
    assert_eq!(s.video_mode_index(), 2);
    Opt::ExclusiveMode.set_slider(&mut s, 0.0);
    assert_eq!(s.file.fullscreen_resolution, None);
}

#[test]
fn pointer_cursor_leaves_a_clicked_option_while_its_focus_stays() {
    use crate::widgets::{Cursor, take_cursor};
    let atlas = atlas();
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::Video);
    let size = (480.0, 480.0);
    let (widgets, top, bottom, _) = ui.layout(&s, size.0, size.1);
    let button = widgets
        .iter()
        .find(|wd| {
            matches!(
                wd.kind,
                WKind::Button {
                    active: true,
                    act: Act::Cycle(_),
                    ..
                }
            ) && (!wd.list || wd.y >= top && wd.y + wd.h <= bottom)
        })
        .expect("visible cycle button");
    let over = (button.x + 1.0, button.y + 1.0);
    ui.click(
        &mut s,
        &atlas,
        over.0,
        over.1,
        size,
        [480, 480],
        false,
        false,
    );
    ui.release();
    assert!(
        ui.focus.is_some(),
        "the accepted click keeps vanilla's focus"
    );
    assert!(!ui.focus_from_keyboard);
    let _ = take_cursor();
    ui.draw_test_frame(
        &s,
        NONE,
        &mut HudBatch::new(
            [480, 480],
            crate::hud::GuiScale {
                requested: 0,
                unicode: false,
            },
        ),
        &atlas,
        over,
        &mut |_, _| {},
    );
    assert_eq!(take_cursor(), Cursor::PointingHand);
    ui.draw_test_frame(
        &s,
        NONE,
        &mut HudBatch::new(
            [480, 480],
            crate::hud::GuiScale {
                requested: 0,
                unicode: false,
            },
        ),
        &atlas,
        (-10.0, -10.0),
        &mut |_, _| {},
    );
    assert_eq!(take_cursor(), Cursor::Arrow, "focus is not mouse hover");
    assert!(
        ui.focus.is_some(),
        "moving the pointer does not clear focus"
    );
    ui.focus_from_keyboard = true;
    ui.draw_test_frame(
        &s,
        NONE,
        &mut HudBatch::new(
            [480, 480],
            crate::hud::GuiScale {
                requested: 0,
                unicode: false,
            },
        ),
        &atlas,
        (-10.0, -10.0),
        &mut |_, _| {},
    );
    assert_eq!(
        take_cursor(),
        Cursor::Arrow,
        "keyboard focus is not mouse hover either"
    );
}

#[test]
fn pointer_cursor_on_the_list_scrollbar_points_then_resizes_while_dragged() {
    use crate::widgets::{Cursor, take_cursor};
    let atlas = atlas();
    let s = Settings::default();
    let mut ui = OptionsUi::default();
    // A long list at a size where it scrolls (the same page and window the
    // wheel test above uses); the bar code is shared by every list page.
    ui.show(Page::KeyBinds);
    let frame = |ui: &mut OptionsUi, cursor: (f32, f32)| {
        let _ = take_cursor();
        ui.draw_test_frame(
            &s,
            NONE,
            &mut HudBatch::new(
                [1708, 960],
                crate::hud::GuiScale {
                    requested: 0,
                    unicode: false,
                },
            ),
            &atlas,
            cursor,
            &mut |_, _| {},
        );
        take_cursor()
    };
    // The first frame lays the page out at the batch's GUI size, which is
    // what `ui.size` then holds; the bar's x comes from that size, not the
    // window's pixels.
    let _ = frame(&mut ui, (-10.0, -10.0));
    let (w, h) = ui.size;
    let (_, top, bottom, content) = ui.layout(&s, w, h);
    assert!(
        ui::scrollbar_thumb(top, bottom, content, ui.scroll).is_some(),
        "the key binds list has more rows than the window"
    );
    // Just inside the bar's left edge, near its top.
    let over = (ui.scrollbar_x(w) + 1.0, top + 1.0);
    assert_eq!(frame(&mut ui, over), Cursor::PointingHand);
    // Pressing the bar holds it: the resize cursor, wherever the pointer is.
    ui.scrollbar_drag_y = Some(over.1);
    assert_eq!(frame(&mut ui, over), Cursor::ResizeNs);
    ui.scrollbar_drag_y = None;
    assert_eq!(frame(&mut ui, over), Cursor::PointingHand);
}

#[test]
fn pointer_cursor_leaves_a_disabled_option_without_focusing_it() {
    use crate::widgets::{Cursor, take_cursor};
    let atlas = atlas();
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::KeyBinds);
    let size = (480.0, 480.0);
    let (widgets, top, bottom, _) = ui.layout(&s, size.0, size.1);
    let button = widgets
        .iter()
        .find(|wd| {
            matches!(wd.kind, WKind::Button { active: false, .. })
                && (!wd.list || wd.y >= top && wd.y + wd.h <= bottom)
        })
        .expect("visible disabled button");
    let over = (button.x + 1.0, button.y + 1.0);
    let _ = take_cursor();
    ui.draw_test_frame(
        &s,
        NONE,
        &mut HudBatch::new(
            [480, 480],
            crate::hud::GuiScale {
                requested: 0,
                unicode: false,
            },
        ),
        &atlas,
        over,
        &mut |_, _| {},
    );
    assert_eq!(take_cursor(), Cursor::NotAllowed);
    assert!(
        ui.click(
            &mut s,
            &atlas,
            over.0,
            over.1,
            size,
            [480, 480],
            false,
            false
        )
        .is_empty()
    );
    assert!(
        ui.focus.is_none(),
        "an inactive widget rejects the click before focus"
    );
    ui.draw_test_frame(
        &s,
        NONE,
        &mut HudBatch::new(
            [480, 480],
            crate::hud::GuiScale {
                requested: 0,
                unicode: false,
            },
        ),
        &atlas,
        (-10.0, -10.0),
        &mut |_, _| {},
    );
    assert_eq!(take_cursor(), Cursor::Arrow);
}

#[test]
fn a_clicked_extras_tooltip_leaves_with_the_pointer() {
    let atlas = atlas();
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::Pvp);
    let (widgets, _, _, _) = ui.layout(&s, 480.0, 480.0);
    let extra = widgets
        .iter()
        .find(|w| {
            matches!(
                &w.kind,
                WKind::Button {
                    act: Act::Extra(item),
                    ..
                } if item.key_name().is_none()
            )
        })
        .unwrap();
    ui.click(
        &mut s,
        &atlas,
        extra.x + 1.0,
        extra.y + 1.0,
        (480.0, 480.0),
        [960, 540],
        false,
        false,
    );
    // The pointer is still on the clicked widget: it carries the
    // description (`setTooltip`, drawn while hovered).
    let mut on = HudBatch::scratch([480, 480]);
    ui.draw_test_frame(
        &s,
        NONE,
        &mut on,
        &atlas,
        (extra.x + extra.w / 2.0, extra.y + extra.h / 2.0),
        &mut |_, _| {},
    );
    // The pointer moved off: the description goes with it. A mouse
    // focus a click left behind (the look-up keeps the click's widget,
    // `focus_from_keyboard` false) does not keep the tooltip against
    // the widget, like vanilla's `MenuTooltipPositioner` does for a
    // keyboard focus.
    let mut off = HudBatch::scratch([480, 480]);
    ui.draw_test_frame(
        &s,
        NONE,
        &mut off,
        &atlas,
        (extra.x + extra.w + 40.0, extra.y + extra.h + 40.0),
        &mut |_, _| {},
    );
    assert!(
        off.vertices.len() < on.vertices.len(),
        "a click left {} tooltip vertices behind when the pointer left ({} vs {})",
        on.vertices.len() - off.vertices.len(),
        on.vertices.len(),
        off.vertices.len(),
    );
}

#[test]
fn extras_reaches_the_zoom_mod_that_replaced_the_extra() {
    let atlas = atlas();
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::Extras);
    let english = || lang::Language::load(&[], "en_us");
    // Without the mod, Extras says where it is installed (a note, not a second way into
    // Options > Mods).
    let labels = lang::scoped(english(), || ui.extras.row_labels(&s)).0;
    assert!(
        labels.iter().any(|l| l == "Zoom: install from Mods"),
        "Extras has no hint for the Zoom mod: {labels:?}"
    );
    // Built in, Zoom's settings live on its page: Extras opens that page.
    s.mods.installed.push(crate::mod_list::Installed {
        id: "zoom".into(),
        title: "Zoom 2.0.0".into(),
        description: String::new(),
        problem: None,
        dev: false,
        native: Some(crate::mod_list::NativePage {
            mappings: Vec::new(),
            settings: vec![crate::mod_list::NativeSetting {
                key: "trails".into(),
                label: "Trails".into(),
                tooltip: None,
                value: crate::mod_list::NativeSettingValue::Toggle(true),
            }],
        }),
    });
    let labels = lang::scoped(english(), || ui.extras.row_labels(&s)).0;
    assert!(labels.iter().any(|l| l == "Settings"), "{labels:?}");
    // The mod's switch is beside it, so Extras keeps the Zoom setting it had.
    assert!(labels.iter().any(|l| l.starts_with("Zoom: ")), "{labels:?}");
    let size = (480.0, 480.0);
    let on = s.mods.is_on(0);
    let at = ui.extras.point_at(&s, size, ExtraRow::Zoom(0));
    assert_eq!(
        ui.click(&mut s, &atlas, at.0, at.1, size, [960, 960], false, false),
        vec![Event::ButtonClick, Event::Changed]
    );
    assert_ne!(s.mods.is_on(0), on);
    let at = ui.extras.point_at(&s, size, ExtraRow::ZoomSettings(0));
    assert_eq!(
        ui.click(&mut s, &atlas, at.0, at.1, size, [960, 960], false, false),
        vec![
            Event::ButtonClick,
            Event::ButtonClick,
            Event::NativeSettings(0)
        ]
    );
    assert_eq!(ui.page(), Page::NativeMod(0));
}

#[test]
fn exclusive_fullscreen_and_its_tooltips_follow_the_option() {
    let mut s = Settings::default();
    assert!(!s.file.bool("exclusiveFullscreen"));
    assert_eq!(
        Opt::ExclusiveFullscreen.tooltip_key(&s),
        "options.exclusiveFullscreen.off.tooltip"
    );
    Opt::ExclusiveFullscreen.cycle(&mut s, false, [854, 480]);
    assert!(s.file.bool("exclusiveFullscreen"));
    assert_eq!(
        Opt::ExclusiveFullscreen.tooltip_key(&s),
        "options.exclusiveFullscreen.on.tooltip"
    );
    // The mode row's own tooltip says it only matters with Exclusive Fullscreen.
    assert_eq!(
        Opt::ExclusiveMode.tooltip_key(&s),
        "options.fullscreen.exclusive.mode.tooltip"
    );
}

#[test]
fn the_graphics_api_option_cycles_default_opengl_vulkan_and_asks_for_a_restart() {
    use crate::graphics::GraphicsApi;
    let mut s = Settings::default();
    s.startup_api = s.horizon.api;
    let offered = GraphicsApi::choices();
    let mut seen = vec![s.horizon.api];
    for _ in 1..offered.len() {
        Opt::GraphicsBackend.cycle(&mut s, false, [854, 480]);
        seen.push(s.horizon.api);
    }
    // Dx12 is Horizon's own Graphics page's; this option has the three of vanilla.
    assert!(!seen.contains(&GraphicsApi::Dx12));
    assert_eq!(seen[0], GraphicsApi::Auto);
    if offered.len() == 3 {
        assert_eq!(seen[1..], [GraphicsApi::OpenGl, GraphicsApi::Vulkan]);
        assert_eq!(s.file.name("preferredGraphicsBackend"), "vulkan");
        Opt::GraphicsBackend.cycle(&mut s, false, [854, 480]);
        assert_eq!(s.horizon.api, GraphicsApi::Auto);
        assert_eq!(s.file.name("preferredGraphicsBackend"), "default");
    }
}

#[test]
fn texture_filtering_rows_exist_and_the_presets_set_them() {
    let mut s = Settings::default();
    for (preset, bit, filtering) in [("fast", 1, 0), ("fancy", 1, 1), ("fabulous", 2, 2)] {
        s.graphics_preset = preset.into();
        apply_preset(&mut s);
        assert_eq!(s.file.int("maxAnisotropyBit"), bit, "{preset}");
        assert_eq!(s.file.int("textureFiltering"), filtering, "{preset}");
    }
    assert!(Opt::TextureFiltering.available());
    assert!(Opt::MaxAnisotropy.available());
    // The anisotropy slider only works with Anisotropic filtering (VideoSettingsScreen.tick).
    let grey = |s: &Settings| {
        matches!(
            option_widget(s, Opt::MaxAnisotropy, 0.0, 0.0, 150.0).kind,
            WKind::Button { active: false, .. }
        )
    };
    s.file
        .set("textureFiltering", crate::options_file::V::Int(1));
    assert!(grey(&s));
    s.file
        .set("textureFiltering", crate::options_file::V::Int(2));
    assert!(!grey(&s));
}

#[test]
#[ignore = "daily"]
fn the_new_video_options_read_as_vanilla_words_them() {
    use crate::graphics::GraphicsApi;
    let root = std::path::PathBuf::from(
        std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"),
    );
    lang::scoped(lang::Language::load(&[&root], "en_us"), || {
        let mut s = Settings::default();
        assert_eq!(Opt::ExclusiveMode.label(&s).plain(), "Setting unavailable");
        s.video_modes = vec![mode(1280, 60.0), mode(1920, 144.0), mode(2560, 59.94)];
        assert_eq!(
            Opt::ExclusiveMode.label(&s).plain(),
            "Exclusive Fullscreen Mode: Current"
        );
        Opt::ExclusiveMode.set_slider(&mut s, 2.0 / 3.0);
        assert_eq!(
            Opt::ExclusiveMode.label(&s).plain(),
            "Exclusive Fullscreen Mode: 1920x1080@144 (24bit)"
        );
        Opt::ExclusiveMode.set_slider(&mut s, 1.0);
        assert_eq!(
            Opt::ExclusiveMode.label(&s).plain(),
            "Exclusive Fullscreen Mode: 2560x1440@59.94 (24bit)"
        );
        s.horizon.api = GraphicsApi::Vulkan;
        assert_eq!(
            Opt::GraphicsBackend.label(&s).plain(),
            "Graphics API: Prefer Vulkan (Experimental)"
        );
        assert!(
            graphics_api_tooltip(&s)
                .plain()
                .starts_with("Requires a game restart to take effect.\n\n")
        );
        s.startup_api = GraphicsApi::Vulkan;
        assert!(
            graphics_api_tooltip(&s)
                .plain()
                .starts_with("Which graphics API")
        );
        s.file
            .set("maxAnisotropyBit", crate::options_file::V::Int(2));
        s.file
            .set("textureFiltering", crate::options_file::V::Int(2));
        assert_eq!(
            Opt::MaxAnisotropy.label(&s).plain(),
            "Anisotropic Filtering: 4x"
        );
        assert_eq!(
            Opt::TextureFiltering.label(&s).plain(),
            "Texture Filtering: Anisotropic"
        );
    });
}

#[test]
fn option_tooltip_keys_follow_value_dependent_descriptions() {
    let mut s = Settings::default();
    assert_eq!(
        Opt::InactivityFps.tooltip_key(&s),
        "options.inactivityFpsLimit.afk.tooltip"
    );
    s.inactivity_fps_limit = "minimized".into();
    assert_eq!(
        Opt::InactivityFps.tooltip_key(&s),
        "options.inactivityFpsLimit.minimized.tooltip"
    );
    for (value, name) in ["none", "byPlayer", "nearby"].iter().enumerate() {
        s.prioritize_chunk_updates = value as u8;
        assert_eq!(
            Opt::ChunkBuilder.tooltip_key(&s),
            format!("options.prioritizeChunkUpdates.{name}.tooltip")
        );
    }
    assert_eq!(
        Opt::MenuBlur.tooltip_key(&s),
        "options.accessibility.menu_background_blurriness.tooltip"
    );
}

#[test]
#[ignore = "daily"]
fn classic_hover_draws_the_selected_options_wrapped_tooltip() {
    let root = std::path::PathBuf::from(
        std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"),
    );
    lang::scoped(lang::Language::load(&[&root], "en_us"), || {
        let atlas = atlas();
        let mut s = Settings::default();
        let mut ui = OptionsUi::default();
        ui.show(Page::Video);
        for mode in ["afk", "minimized"] {
            s.inactivity_fps_limit = mode.into();
            let (widgets, _, _, _) = ui.layout(&s, 480.0, 480.0);
            let option = widgets
                .iter()
                .find(|w| {
                    matches!(
                        w.kind,
                        WKind::Button {
                            act: Act::Cycle(Opt::InactivityFps),
                            ..
                        }
                    )
                })
                .unwrap();
            let cursor = (option.x + 1.0, option.y + 1.0);
            let mut actual = HudBatch::scratch([480, 480]);
            let mut foreground_end = None;
            ui.draw_test_frame(
                &s,
                NONE,
                &mut actual,
                &atlas,
                cursor,
                &mut |phase, batch| {
                    if phase == crate::screen_draw::RenderPhase::Foreground {
                        foreground_end = Some(batch.vertices.len());
                    }
                },
            );
            let key = format!("options.inactivityFpsLimit.{mode}.tooltip");
            assert!(lang::has(&key));
            let mut expected = HudBatch::scratch([480, 480]);
            ui::tooltip(
                &mut expected,
                &atlas,
                &atlas.font.split(&tr(&key), 170.0),
                cursor,
            );
            assert!(!expected.vertices.is_empty());
            assert_eq!(
                foreground_end,
                Some(actual.vertices.len() - expected.vertices.len()),
                "foreground precedes the tooltip",
            );
            let tail = &actual.vertices[actual.vertices.len() - expected.vertices.len()..];
            assert_eq!(
                bytemuck::cast_slice::<_, u8>(tail),
                bytemuck::cast_slice::<_, u8>(&expected.vertices)
            );
        }
    });
}

#[test]
fn classic_video_search_keeps_only_headings_with_matching_options() {
    let mut ui = OptionsUi::default();
    ui.show(Page::Video);
    let settings = Settings::default();
    let headers = |rows: Vec<Row>| {
        rows.into_iter()
            .filter_map(|row| match row {
                Row::Header(key) => Some(key),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let all_headers = vec![
        "options.video.display.header",
        "options.video.quality.header",
        "options.video.preferences.header",
    ];
    assert_eq!(headers(ui.list_rows(&settings)), all_headers);
    for (opt, header) in [
        (Opt::Fullscreen, all_headers[0]),
        (Opt::RenderDistance, all_headers[1]),
        (Opt::Preset, all_headers[1]),
        (Opt::Autosave, all_headers[2]),
    ] {
        ui.settings_search
            .set_value(&lang::get(opt.caption()).to_uppercase());
        let rows = ui.list_rows(&settings);
        assert_eq!(headers(ui.list_rows(&settings)), vec![header]);
        assert!(rows.iter().any(|row| match row {
            Row::Big(found) => *found == opt,
            Row::Pair(a, b) => *a == opt || *b == Some(opt),
            _ => false,
        }));
    }
    ui.settings_search.set_value("no matching video setting");
    let rows = ui.list_rows(&settings);
    assert!(headers(ui.list_rows(&settings)).is_empty());
    assert!(rows.is_empty(), "{}", rows.len());
    ui.settings_search.set_value("");
    assert_eq!(headers(ui.list_rows(&settings)), all_headers);
}

#[test]
fn classic_search_emits_each_matching_heading_once_in_section_order() {
    let mut ui = OptionsUi::default();
    ui.settings_search
        .set_value(&lang::get(Opt::Fullscreen.caption()));
    let rows = ui.filter_option_rows(vec![
        Row::Header("empty"),
        Row::Header("display"),
        Row::Pair(Opt::Fullscreen, Some(Opt::Fullscreen)),
        Row::Big(Opt::Fullscreen),
        Row::Header("quality"),
        Row::Big(Opt::Fullscreen),
        Row::Header("trailing empty"),
    ]);
    assert!(matches!(
        rows.as_slice(),
        [
            Row::Header("display"),
            Row::Pair(Opt::Fullscreen, Some(Opt::Fullscreen)),
            Row::Big(Opt::Fullscreen),
            Row::Header("quality"),
            Row::Big(Opt::Fullscreen),
        ]
    ));
}

#[test]
fn classic_video_gui_scale_cycles_on_press_without_dragging() {
    let atlas = atlas();
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::Video);
    let size = (480.0, 480.0);
    let (widgets, _, _, _) = ui.layout(&s, size.0, size.1);
    let button = widgets
        .iter()
        .find(|wd| {
            matches!(
                wd.kind,
                WKind::Button {
                    active: true,
                    act: Act::Cycle(Opt::GuiScale),
                    ..
                }
            )
        })
        .expect("GUI Scale must be a cycle button in classic Video Settings");
    let (x, y) = (button.x + 1.0, button.y + 1.0);
    assert!(Opt::GuiScale.slider_position(&s).is_none());
    assert!(
        ui.click(&mut s, &atlas, x, y, size, [854, 480], false, true)
            .is_empty()
    );
    assert_eq!(s.gui_scale, 0);
    for expected in [1, 2, 0] {
        assert_eq!(
            ui.click(&mut s, &atlas, x, y, size, [854, 480], false, false),
            vec![Event::ButtonClick, Event::Changed]
        );
        assert_eq!(s.gui_scale, expected);
        assert!(
            ui.click(&mut s, &atlas, x + 100.0, y, size, [854, 480], true, false)
                .is_empty()
        );
        assert_eq!(s.gui_scale, expected);
        assert!(ui.release().is_empty());
    }
}

#[test]
fn extras_from_horizon_settings_edit_shared_settings_and_return() {
    let atlas = atlas();
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    let size = (480.0, 480.0);
    let click = |ui: &mut OptionsUi, s: &mut Settings, act: Act| {
        let (widgets, _, _, _) = ui.layout(s, size.0, size.1);
        let widget = widgets
            .iter()
            .find(|w| matches!(&w.kind, WKind::Button { act: a, active: true, .. } if *a == act))
            .unwrap();
        ui.click(
            s,
            &atlas,
            widget.x + 1.0,
            widget.y + 1.0,
            size,
            [960, 960],
            false,
            false,
        )
    };
    for page in [Page::Horizon, Page::Extras] {
        assert_eq!(
            click(&mut ui, &mut s, Act::Push(page)),
            vec![Event::ButtonClick]
        );
        assert_eq!(ui.page(), page);
    }
    assert_eq!(
        ui.extras.row_labels(&s).len(),
        // And the note about the Zoom mod over them.
        1 + crate::extras::rows().iter().filter(|r| r.is_ok()).count()
    );
    let press = |ui: &mut OptionsUi, s: &mut Settings, item: crate::extras::Item| {
        let at = ui.extras.point_at(s, size, ExtraRow::Item(item));
        ui.click(s, &atlas, at.0, at.1, size, [960, 960], false, false)
    };
    assert_eq!(
        press(&mut ui, &mut s, crate::extras::Item::Sort),
        vec![Event::ButtonClick, Event::Changed]
    );
    assert!(s.extras.sort);
    assert_eq!(
        press(&mut ui, &mut s, crate::extras::Item::Pregen),
        vec![Event::ButtonClick, Event::Changed]
    );
    assert!(s.extras.pregen);
    assert_eq!(
        press(&mut ui, &mut s, crate::extras::Item::SortKey),
        vec![Event::ButtonClick]
    );
    assert_eq!(ui.selected_key, Some(crate::extras::SORT_KEY));
    assert_eq!(
        ui.key(&mut s, &atlas, winit::keyboard::KeyCode::KeyC, None),
        vec![Event::Changed]
    );
    assert_eq!(s.extras.sort_key, "key.keyboard.c");
    assert_eq!(
        crate::extras::Extras::parse(&s.extras.serialize()),
        s.extras
    );
    assert_eq!(
        ui.key(&mut s, &atlas, winit::keyboard::KeyCode::Escape, None),
        vec![Event::Changed]
    );
    assert_eq!(ui.page(), Page::Horizon);
}

/// A Bindings row pressed waits for a controller button: the next click or key cancels the
/// wait, Escape unbinds the action as on Key Binds, and none of them leaves the screen.
#[test]
fn a_controller_binding_waits_and_a_click_or_a_key_ends_the_wait() {
    use crate::controller_ui::Item;
    use winit::keyboard::KeyCode as K;
    let atlas = atlas();
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::PadBinds);
    let size = (854.0, 480.0);
    ui.size = size;
    let jump = Item::Bind("key.jump");
    let (x, y) = ui.controller.point_at(&s, size, jump);
    let press = |ui: &mut OptionsUi, s: &mut Settings| {
        ui.click(s, &atlas, x, y, size, [1708, 960], false, false)
    };
    assert_eq!(press(&mut ui, &mut s), vec![Event::ButtonClick]);
    assert_eq!(ui.selected_pad_action, Some("key.jump"));
    assert!(press(&mut ui, &mut s).is_empty());
    assert_eq!(ui.selected_pad_action, None);
    press(&mut ui, &mut s);
    assert_eq!(ui.key(&mut s, &atlas, K::KeyQ, None), vec![Event::Changed]);
    assert_eq!(
        (ui.selected_pad_action, s.controller.is_default("key.jump")),
        (None, true)
    );
    press(&mut ui, &mut s);
    assert_eq!(
        ui.key(&mut s, &atlas, winit::keyboard::KeyCode::Escape, None),
        vec![Event::Changed]
    );
    assert_eq!(s.controller.binding("key.jump"), None);
    assert_eq!(ui.page(), Page::PadBinds);
}

/// Every way a pad or a wheel scrolls a page on the shell moves the same list: Replay Studio's
/// took the wheel and missed `scroll_at`, so a stick couldn't scroll it.
#[test]
fn the_wheel_and_the_stick_scroll_a_page_on_the_shell_alike() {
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::Extras);
    // Too short for the page.
    let short = (480.0, 180.0);
    ui.wheel(&mut s, short, (0.0, 0.0), [960, 360], -1.0);
    let one = ui.extras.scroll_amount();
    assert!(one > 0.0);
    ui.scroll_at(&s, short, (0.0, 0.0), -1.0);
    assert_eq!(ui.extras.scroll_amount(), 2.0 * one);
    ui.scroll_at(&s, short, (0.0, 0.0), 2.0);
    assert_eq!(ui.extras.scroll_amount(), 0.0);
}

#[test]
fn classic_graphics_presets_set_the_distances() {
    let atlas = atlas();
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::Video);
    let size = (480.0, 480.0);
    let (widgets, _, _, _) = ui.layout(&s, size.0, size.1);
    let preset = widgets
        .iter()
        .find(|wd| {
            matches!(
                wd.kind,
                WKind::Slider {
                    opt: Opt::Preset,
                    ..
                }
            )
        })
        .expect("the Graphics preset slider is in classic Video Settings");
    // Dragging the slider across its range (GraphicsPreset.apply on each change).
    let y = preset.y + 1.0;
    let mut seen = Vec::new();
    ui.click(
        &mut s,
        &atlas,
        preset.x + 1.0,
        y,
        size,
        [854, 480],
        false,
        false,
    );
    for step in 0..=20 {
        let x = preset.x + preset.w * step as f32 / 20.0;
        ui.click(&mut s, &atlas, x, y, size, [854, 480], true, false);
        seen.push((
            s.graphics_preset.clone(),
            s.render_distance,
            s.simulation_distance,
            s.cloud_range,
        ));
    }
    ui.release();
    for expected in [
        ("fast", 8, 6, 32),
        ("fancy", 16, 12, 64),
        ("fabulous", 32, 12, 128),
    ] {
        assert!(
            seen.iter()
                .any(|(p, r, d, c)| (p.as_str(), *r, *d, *c) == expected),
            "{expected:?} in {seen:?}"
        );
    }
}

#[test]
fn gui_scale_cycle_handles_reverse_and_out_of_range_saved_values() {
    for (framebuffer, force_unicode, max) in [
        ([854, 480], false, 2),
        ([1920, 1080], false, 4),
        ([1280, 720], true, 4),
        ([3840, 2160], false, 9),
        ([320, 240], false, 1),
    ] {
        let mut s = Settings {
            force_unicode,
            ..Settings::default()
        };
        for expected in (0..=max).rev() {
            Opt::GuiScale.cycle(&mut s, true, framebuffer);
            assert_eq!(s.gui_scale, expected);
        }
        for expected in (1..=max).chain([0]) {
            Opt::GuiScale.cycle(&mut s, false, framebuffer);
            assert_eq!(s.gui_scale, expected);
        }
        s.gui_scale = u32::MAX;
        Opt::GuiScale.cycle(&mut s, false, framebuffer);
        assert_eq!(s.gui_scale, 0);
        s.gui_scale = u32::MAX;
        Opt::GuiScale.cycle(&mut s, true, framebuffer);
        assert_eq!(s.gui_scale, max - 1);
    }
    let mut s = Settings {
        gui_scale: 4,
        ..Settings::default()
    };
    // A resized window supplies a new maximum on the next interaction.
    Opt::GuiScale.cycle(&mut s, true, [854, 480]);
    assert_eq!(s.gui_scale, 1);
}

#[test]
fn classic_buttons_sound_but_disabled_and_nonprimary_clicks_are_silent() {
    let atlas = atlas();
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::Mouse);
    let size = (480.0, 320.0);
    let (widgets, _, _, _) = ui.layout(&s, size.0, size.1);
    let button = widgets
        .iter()
        .find(|wd| {
            matches!(
                wd.kind,
                WKind::Button {
                    active: true,
                    act: Act::Cycle(_),
                    ..
                }
            )
        })
        .unwrap();
    let (x, y) = (button.x + 1.0, button.y + 1.0);
    assert!(
        ui.click(&mut s, &atlas, x, y, size, [854, 480], false, true)
            .is_empty()
    );
    assert_eq!(
        ui.click(&mut s, &atlas, x, y, size, [854, 480], false, false),
        vec![Event::ButtonClick, Event::Changed]
    );
    assert!(ui.release().is_empty());
    assert!(
        ui.click(&mut s, &atlas, 0.0, 0.0, size, [854, 480], false, false)
            .is_empty()
    );
    ui.show(Page::KeyBinds);
    let (widgets, _, _, _) = ui.layout(&s, size.0, size.1);
    let disabled = widgets
        .iter()
        .find(|wd| matches!(wd.kind, WKind::Button { active: false, .. }))
        .unwrap();
    assert!(
        ui.click(
            &mut s,
            &atlas,
            disabled.x + 1.0,
            disabled.y + 1.0,
            size,
            [854, 480],
            false,
            false
        )
        .is_empty()
    );
}

#[test]
fn classic_sliders_sound_once_on_release_including_unchanged_values() {
    let atlas = atlas();
    let size = (480.0, 320.0);
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::Sound);
    let (widgets, _, _, _) = ui.layout(&s, size.0, size.1);
    let slider = widgets
        .iter()
        .find(|wd| {
            matches!(
                wd.kind,
                WKind::Slider {
                    opt: Opt::Volume(0),
                    ..
                }
            )
        })
        .unwrap();
    let (x, y) = (slider.x + slider.w - 1.0, slider.y + 1.0);
    assert_eq!(
        ui.click(&mut s, &atlas, x, y, size, [854, 480], false, false),
        vec![Event::SliderDrag]
    );
    assert_eq!(s.master_volume, 1.0);
    assert_eq!(ui.release(), vec![Event::ButtonClick, Event::Changed]);
    assert!(ui.release().is_empty());
    assert_eq!(
        ui.click(&mut s, &atlas, x, y, size, [854, 480], false, false),
        vec![Event::SliderDrag]
    );
    for cx in [-100.0, 250.0, 1000.0] {
        assert_eq!(
            ui.click(&mut s, &atlas, cx, -100.0, size, [854, 480], true, false),
            vec![Event::SliderDrag, Event::Volume(0, s.master_volume)]
        );
    }
    assert_eq!(ui.release(), vec![Event::ButtonClick, Event::Changed]);
    assert!(
        ui.click(&mut s, &atlas, x, y, size, [854, 480], true, false)
            .is_empty()
    );
}

#[test]
fn a_moved_volume_slider_reports_its_category_and_value() {
    let atlas = atlas();
    let size = (480.0, 320.0);
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::Sound);
    let hostile = SOUND_SOURCES.iter().position(|n| *n == "hostile").unwrap();
    let (widgets, _, _, _) = ui.layout(&s, size.0, size.1);
    let slider = widgets
        .iter()
        .find(|wd| matches!(wd.kind, WKind::Slider { opt: Opt::Volume(i), .. } if i == hostile))
        .unwrap();
    let y = slider.y + 1.0;
    let x = slider.x + 4.0 + (slider.w - 8.0) * 0.5;
    let events = ui.click(&mut s, &atlas, x, y, size, [854, 480], false, false);
    let volume = s.volumes[hostile];
    assert!((volume - 0.5).abs() < 1e-3, "{volume}");
    assert_eq!(
        events,
        vec![Event::SliderDrag, Event::Volume(hostile, volume)]
    );
    // OptionInstance.set: an unchanged value runs no callback.
    assert_eq!(
        ui.click(&mut s, &atlas, x, -50.0, size, [854, 480], true, false),
        vec![Event::SliderDrag]
    );
}

#[test]
fn classic_sliders_follow_the_pointer_and_snap_on_release() {
    let atlas = atlas();
    let size = (480.0, 320.0);
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::Sound);
    let (widgets, _, _, _) = ui.layout(&s, size.0, size.1);
    let slider = widgets
        .iter()
        .find(|wd| {
            matches!(
                wd.kind,
                WKind::Slider {
                    opt: Opt::Volume(0),
                    ..
                }
            )
        })
        .unwrap();
    let (x, y) = (slider.x + 4.0 + (slider.w - 8.0) * 0.3, slider.y + 1.0);
    ui.click(&mut s, &atlas, x, y, size, [854, 480], false, false);
    let raw = ui::slider_value(slider.x, slider.w, x);
    // Held: the handle stays under the pointer whatever the stepped value.
    assert_eq!(ui.slider_shown(Opt::Volume(0), 0.25), raw);
    assert_eq!(ui.slider_shown(Opt::Volume(1), 0.25), 0.25);
    ui.click(
        &mut s,
        &atlas,
        x + 10.0,
        -50.0,
        size,
        [854, 480],
        true,
        false,
    );
    let raw = ui::slider_value(slider.x, slider.w, x + 10.0);
    assert_eq!(ui.slider_shown(Opt::Volume(0), 0.25), raw);
    ui.release();
    assert_eq!(ui.slider_shown(Opt::Volume(0), 0.25), 0.25);
    // Render distance applies late (applyValueImmediately = false) and
    // keeps its position until then, even once released.
    ui.slider_value = Some((Opt::RenderDistance, 0.51, Instant::now()));
    assert_eq!(ui.slider_shown(Opt::RenderDistance, 0.5), 0.51);
    ui.slider_value = Some((
        Opt::RenderDistance,
        0.51,
        Instant::now() - std::time::Duration::from_millis(600),
    ));
    assert_eq!(ui.slider_shown(Opt::RenderDistance, 0.5), 0.5);
}

#[test]
fn classic_language_rows_and_text_fields_are_silent() {
    let atlas = atlas();
    let mut s = Settings::default();
    let mut ui = OptionsUi {
        languages: languages(),
        ..Default::default()
    };
    ui.show(Page::Language);
    let size = (480.0, 320.0);
    let (widgets, top, bottom, _) = ui.layout(&s, size.0, size.1);
    let row = widgets
        .iter()
        .find(|wd| {
            matches!(wd.kind, WKind::LanguageRow { .. }) && wd.y >= top && wd.y + wd.h <= bottom
        })
        .unwrap();
    assert!(
        ui.click(
            &mut s,
            &atlas,
            row.x + 1.0,
            row.y + 1.0,
            size,
            [854, 480],
            false,
            false
        )
        .is_empty()
    );
    assert!(ui.release().is_empty());
    let search = ui.language_search.clone();
    assert!(
        ui.click(
            &mut s,
            &atlas,
            search.x + 1.0,
            search.y + 1.0,
            size,
            [854, 480],
            false,
            false
        )
        .is_empty()
    );
}

/// `LanguageSelectionList.Entry.keyPressed`: the selection key picks the
/// row's language and finishes the screen in one press (`onDone`), so the
/// page is gone after Enter and the language is already applied.
#[test]
fn the_selection_key_picks_a_language_and_finishes_the_page() {
    use winit::keyboard::KeyCode;
    let atlas = atlas();
    let rows = languages();
    let code = {
        let s = Settings::default();
        let mut ui = OptionsUi {
            languages: rows.clone(),
            ..Default::default()
        };
        ui.size = (480.0, 320.0);
        ui.show(Page::Language);
        let (widgets, ..) = ui.layout(&s, 480.0, 320.0);
        widgets
            .iter()
            .find_map(|wd| match &wd.kind {
                WKind::LanguageRow { code, .. } => Some(code.clone()),
                _ => None,
            })
            .expect("a language row")
    };
    for key in [KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space] {
        let mut s = Settings::default();
        assert_ne!(s.language, code, "the row starts unchosen");
        let mut ui = OptionsUi {
            languages: rows.clone(),
            ..Default::default()
        };
        ui.size = (480.0, 320.0);
        ui.show(Page::Language);
        ui.focus = Some(FocusId::Lang(code.clone()));
        ui.focus_from_keyboard = true;
        let events = ui.key(&mut s, &atlas, key, None);
        assert_eq!(s.language, code, "{key:?} applies the row's language");
        assert!(events.contains(&Event::Language(code.clone())), "{key:?}");
        assert_eq!(ui.page(), Page::Main, "{key:?} finishes the language page");
    }
}

#[test]
fn language_search_hint_does_not_overlap_the_focused_caret() {
    let atlas = atlas();
    let settings = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::Language);
    let draw_hint = |ui: &mut OptionsUi| {
        let mut batch = HudBatch::scratch([960, 640]);
        ui.draw_test_frame(
            &settings,
            NONE,
            &mut batch,
            &atlas,
            (-1.0, -1.0),
            &mut |_, _| {},
        );
        let search = &ui.language_search;
        batch.vertices.iter().any(|v| {
            v.color == rgba(170, 170, 170, 255)
                && v.pos[0] >= search.text_x()
                && v.pos[0] < search.x + search.width
                && v.pos[1] >= search.text_y()
                && v.pos[1] < search.y + search.height
        })
    };
    ui.language_search.focused = false;
    assert!(draw_hint(&mut ui));
    ui.language_search.focused = true;
    assert!(!draw_hint(&mut ui));
    ui.language_search.focused = false;
    ui.language_search.set_value("english");
    assert!(!draw_hint(&mut ui));
    ui.language_search.set_value("");
    assert!(draw_hint(&mut ui));
}

fn languages() -> Vec<LanguageInfo> {
    (0..30)
        .map(|i| LanguageInfo {
            code: format!("language_{i}"),
            name: format!("Language {i}"),
            region: String::new(),
            bidirectional: false,
        })
        .collect()
}

#[test]
fn classic_scroll_stops_at_both_limits_before_layout() {
    let s = Settings::default();
    let size = (480.0, 120.0);
    for page in [Page::KeyBinds, Page::Chat, Page::Sound, Page::Language] {
        let mut ui = OptionsUi {
            languages: languages(),
            ..Default::default()
        };
        ui.show(page);
        let (_, top, bottom, content) = ui.layout(&s, size.0, size.1);
        let max = content - (bottom - top);
        assert!(max > 4.5, "{page:?}");
        for _ in 0..3 {
            ui.scroll_at(&s, size, (240.0, 60.0), 1000.0);
            assert_eq!(ui.scroll, 0.0, "{page:?}");
        }
        for _ in 0..3 {
            ui.scroll_at(&s, size, (240.0, 60.0), -1000.0);
            assert_eq!(ui.scroll, max, "{page:?}");
        }
        ui.scroll_at(&s, size, (240.0, 60.0), 0.25);
        assert_eq!(ui.scroll, max - 4.5, "{page:?}");
        ui.scroll_at(&s, size, (240.0, 60.0), 1000.0);
        ui.scroll_at(&s, size, (240.0, 60.0), -0.25);
        assert_eq!(ui.scroll, 4.5, "{page:?}");
    }
}

#[test]
fn partially_visible_keybind_rows_are_clipped_and_clickable() {
    let mut atlas = atlas();
    let button = crate::hud::Rect {
        u0: 0.25,
        v0: 0.25,
        u1: 0.5,
        v1: 0.5,
        w: 20,
        h: 20,
    };
    for name in ["button", "button_disabled", "button_highlighted"] {
        atlas
            .sprites
            .insert(format!("gui/sprites/widget/{name}"), button);
    }
    for above in [true, false] {
        let mut s = Settings::default();
        let mut ui = OptionsUi::default();
        ui.show(Page::KeyBinds);
        let mut batch = HudBatch::scratch([480, 240]);
        let size = (batch.width, batch.height);
        let (widgets, top, bottom, _) = ui.layout(&s, size.0, size.1);
        let row = widgets
            .iter()
            .filter(|w| {
                matches!(
                    w.kind,
                    WKind::Button {
                        act: Act::Key(_),
                        ..
                    }
                )
            })
            .nth(10)
            .unwrap();
        let WKind::Button {
            act: Act::Key(key), ..
        } = row.kind
        else {
            unreachable!()
        };
        let row_y = if above { top - 5.0 } else { bottom - 16.0 };
        ui.scroll = row.y - row_y;
        let (x, width, height) = (row.x, row.w, row.h);
        ui.draw_test_frame(&s, NONE, &mut batch, &atlas, (-1.0, -1.0), &mut |_, _| {});
        let vertices: Vec<_> = batch
            .vertices
            .iter()
            .filter(|v| {
                (0.25..=0.5).contains(&v.uv[0])
                    && (0.25..=0.5).contains(&v.uv[1])
                    && v.pos[0] >= x
                    && v.pos[0] <= x + width
                    && v.pos[1] >= row_y
                    && v.pos[1] <= row_y + height
            })
            .collect();
        assert!(!vertices.is_empty());
        let lo = vertices
            .iter()
            .map(|v| v.pos[1])
            .fold(f32::INFINITY, f32::min);
        let hi = vertices
            .iter()
            .map(|v| v.pos[1])
            .fold(f32::NEG_INFINITY, f32::max);
        assert_eq!(lo, row_y.max(top));
        assert_eq!(hi, (row_y + height).min(bottom));
        let hidden_y = if above {
            row_y + 1.0
        } else {
            row_y + height - 1.0
        };
        assert!(
            ui.click(
                &mut s,
                &atlas,
                x + 1.0,
                hidden_y,
                size,
                [480, 240],
                false,
                false
            )
            .is_empty()
        );
        assert_eq!(ui.selected_key, None);
        assert_eq!(
            ui.click(
                &mut s,
                &atlas,
                x + 1.0,
                (lo + hi) / 2.0,
                size,
                [480, 240],
                false,
                false
            ),
            vec![Event::ButtonClick]
        );
        assert_eq!(ui.selected_key, Some(key));
    }
}

#[test]
fn classic_scrollbar_drag_is_bounded_silent_and_stops_on_release() {
    let atlas = atlas();
    let size = (480.0, 160.0);
    for page in [Page::KeyBinds, Page::Language, Page::Chat] {
        let mut s = Settings::default();
        let mut ui = OptionsUi {
            languages: languages(),
            ..Default::default()
        };
        ui.show(page);
        let (_, top, bottom, content) = ui.layout(&s, size.0, size.1);
        let max = content - (bottom - top);
        assert!(max > 0.0, "{page:?}");
        let x = ui.scrollbar_x(size.0) + 2.0;
        let (_, thumb) = ui::scrollbar_thumb(top, bottom, content, ui.scroll).unwrap();
        let y = top + thumb / 2.0;
        assert!(
            ui.click(&mut s, &atlas, x, y, size, [960, 480], false, true)
                .is_empty()
        );
        assert_eq!(ui.scrollbar_drag_y, None);
        assert!(
            ui.click(&mut s, &atlas, x, y, size, [960, 480], false, false)
                .is_empty()
        );
        assert_eq!(ui.scrollbar_drag_y, Some(y));
        let delta = 5.0;
        assert!(
            ui.click(
                &mut s,
                &atlas,
                x - 100.0,
                y + delta,
                size,
                [960, 480],
                true,
                false
            )
            .is_empty()
        );
        assert!((ui.scroll - delta * max / (bottom - top - thumb)).abs() < 0.001);
        for (y, expected) in [(bottom + 100.0, max), (top - 100.0, 0.0)] {
            assert!(
                ui.click(&mut s, &atlas, x, y, size, [960, 480], true, false)
                    .is_empty()
            );
            assert!((ui.scroll - expected).abs() < 0.001, "{page:?}");
        }
        assert!(ui.release().is_empty());
        assert_eq!(ui.scrollbar_drag_y, None);
        assert!(
            ui.click(
                &mut s,
                &atlas,
                x,
                bottom + 100.0,
                size,
                [960, 480],
                true,
                false
            )
            .is_empty()
        );
        assert_eq!(ui.scroll, 0.0);
        assert_eq!(s.keys, Settings::default().keys);
        ui.click(&mut s, &atlas, x, top + 1.0, size, [960, 480], false, false);
        assert!(ui.scrollbar_drag_y.is_some());
        ui.show(Page::Mouse);
        assert_eq!(ui.scrollbar_drag_y, None);
    }
}

#[test]
fn extras_and_vanilla_bindings_accept_mouse_buttons() {
    use crate::extras::{Extras, SORT_KEY};
    use winit::event::MouseButton;
    for button in [
        MouseButton::Left,
        MouseButton::Right,
        MouseButton::Middle,
        MouseButton::Back,
        MouseButton::Forward,
    ] {
        for name in [SORT_KEY, "key.jump"] {
            let mut ui = OptionsUi::default();
            let mut s = Settings::default();
            ui.selected_key = Some(name);
            assert_eq!(ui.bind_mouse(&mut s, button), vec![Event::Changed]);
            assert_eq!(ui.selected_key, None);
            let value = keybinds::mouse_name(button);
            match name {
                SORT_KEY => assert_eq!(s.extras.sort_key, value),
                _ => assert_eq!(s.keys.get(name), value),
            }
            assert_eq!(Extras::parse(&s.extras.serialize()), s.extras);
            if name != "key.jump" {
                assert_eq!(s.keys, Settings::default().keys);
            }
            assert!(ui.bind_mouse(&mut s, button).is_empty());
        }
    }
}

#[test]
fn horizon_extra_mouse_binding_consumes_the_click() {
    let atlas = atlas();
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::Extras);
    for (right, expected) in [(false, "key.mouse.left"), (true, "key.mouse.right")] {
        ui.selected_key = Some(crate::extras::SORT_KEY);
        assert_eq!(
            ui.click(
                &mut s,
                &atlas,
                2.0,
                2.0,
                (480.0, 240.0),
                [960, 480],
                false,
                right
            ),
            vec![Event::Changed]
        );
        assert_eq!(s.extras.sort_key, expected);
        assert_eq!(ui.selected_key, None);
        assert_eq!(ui.page(), Page::Extras);
    }
}

#[test]
fn key_rebinding_takes_precedence_over_scrollbar_dragging() {
    let atlas = atlas();
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::KeyBinds);
    ui.selected_key = Some("key.jump");
    let x = ui.scrollbar_x(480.0) + 2.0;
    assert_eq!(
        ui.click(
            &mut s,
            &atlas,
            x,
            50.0,
            (480.0, 240.0),
            [960, 480],
            false,
            false
        ),
        vec![Event::Changed]
    );
    assert_eq!(s.keys.get("key.jump"), "key.mouse.left");
    assert_eq!(ui.scrollbar_drag_y, None);
    assert!(ui.release().is_empty());
}

/// The `Act::Key` rows of the Key Binds page, all of them on one tall screen.
fn key_bind_rows(ui: &mut OptionsUi, s: &Settings) -> Vec<&'static str> {
    let (widgets, _, _, _) = ui.layout(s, 480.0, 8000.0);
    widgets
        .iter()
        .filter_map(|w| match &w.kind {
            WKind::Button {
                act: Act::Key(name),
                ..
            } => Some(*name),
            _ => None,
        })
        .collect()
}

#[test]
fn key_binds_list_the_emote_key_and_escape_unbinds_it() {
    // `key.emote` is Horizon's own (`HORIZON_MAPPINGS`, kept in horizon-extras.txt). #18361
    let atlas = atlas();
    let dir = std::env::temp_dir().join(format!(
        "horizon-key-binds-emote-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let options = dir.join("options.txt");
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::KeyBinds);
    assert!(key_bind_rows(&mut ui, &s).contains(&"key.emote"));
    // Another key put on B warns on the emote row.
    s.keys.set("key.jump", "key.keyboard.b");
    let warned = |s: &Settings| {
        !key_conflicts(
            s,
            "",
            "key.emote",
            s.keys.get("key.emote"),
            0,
            crate::mod_list::KeyConflictContext::Universal,
            s.keys.is_default("key.emote"),
        )
        .is_empty()
    };
    assert!(warned(&s));
    s.keys.reset_all();
    assert!(!warned(&s));
    // Click the row, then Escape: "Not Bound".
    let (widgets, _, _, _) = ui.layout(&s, 480.0, 8000.0);
    let row = widgets
        .iter()
        .find(|w| {
            matches!(
                w.kind,
                WKind::Button {
                    act: Act::Key("key.emote"),
                    ..
                }
            )
        })
        .unwrap();
    let (x, y) = (row.x + row.w / 2.0, row.y + row.h / 2.0);
    assert_eq!(
        ui.click(
            &mut s,
            &atlas,
            x,
            y,
            (480.0, 8000.0),
            [480, 8000],
            false,
            false
        ),
        vec![Event::ButtonClick]
    );
    assert_eq!(ui.selected_key, Some("key.emote"));
    assert_eq!(
        ui.key(&mut s, &atlas, winit::keyboard::KeyCode::Escape, None),
        vec![Event::Changed]
    );
    assert_eq!(s.keys.get("key.emote"), "key.keyboard.unknown");
    // Saved to horizon-extras.txt, options.txt stays vanilla's, and a restart keeps it.
    s.save(&options);
    let extras = std::fs::read_to_string(dir.join(crate::extras::FILE)).unwrap();
    assert!(
        extras.lines().any(|l| l == "emoteKey:key.keyboard.unknown"),
        "{extras}"
    );
    assert!(!std::fs::read_to_string(&options).unwrap().contains("emote"));
    assert_eq!(
        Settings::load(&options).keys.get("key.emote"),
        "key.keyboard.unknown"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_mapping_the_game_binds_has_a_key_binds_row() {
    let s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::KeyBinds);
    let rows = key_bind_rows(&mut ui, &s);
    let (widgets, _, _, _) = ui.layout(&s, 480.0, 8000.0);
    for &(name, _, _) in keybinds::all_mappings() {
        assert_eq!(rows.iter().filter(|r| **r == name).count(), 1, "{name}");
        assert!(
            widgets.iter().any(|w| matches!(
                w.kind,
                WKind::Button {
                    act: Act::ResetKey(n),
                    ..
                } if n == name
            )),
            "{name} has no Reset"
        );
        // Searching for its name finds it, and only rows that match.
        let query = lang::get(name);
        ui.settings_search.value = query.clone();
        let found = key_bind_rows(&mut ui, &s);
        assert!(found.contains(&name), "{name}");
        assert!(found.len() < rows.len(), "{name}");
        assert!(
            found
                .iter()
                .all(|r| lang::get(r).to_lowercase().contains(&query.to_lowercase())),
            "{name}: {found:?}"
        );
        ui.settings_search.value.clear();
    }
    assert_eq!(rows.len(), keybinds::all_mappings().count());
}

#[test]
fn classic_pages_without_overflow_do_not_scroll() {
    let s = Settings::default();
    for page in [Page::Main, Page::Font, Page::Mouse] {
        let mut ui = OptionsUi::default();
        ui.show(page);
        for amount in [-1000.0, 1000.0, -0.25] {
            ui.scroll(&s, (480.0, 720.0), amount);
            assert_eq!(ui.scroll, 0.0, "{page:?}");
        }
    }
}

#[test]
fn classic_resize_positions_rows_at_the_clamped_offset() {
    let s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::KeyBinds);
    ui.scroll(&s, (480.0, 120.0), -1000.0);
    assert!(ui.scroll > 0.0);
    let (widgets, _, _, _) = ui.layout(&s, 480.0, 4000.0);
    assert_eq!(ui.scroll, 0.0);
    let (expected, _, _, _) = ui.layout(&s, 480.0, 4000.0);
    assert_eq!(
        widgets.iter().map(|w| w.y).collect::<Vec<_>>(),
        expected.iter().map(|w| w.y).collect::<Vec<_>>()
    );
}

#[test]
fn language_centering_and_filtering_use_bounded_row_positions() {
    let s = Settings {
        language: "language_29".into(),
        ..Default::default()
    };
    let mut ui = OptionsUi {
        languages: languages(),
        ..Default::default()
    };
    ui.show(Page::Language);
    let (widgets, top, bottom, content) = ui.layout(&s, 480.0, 240.0);
    assert_eq!(ui.scroll, content - (bottom - top));
    let (expected, _, _, _) = ui.layout(&s, 480.0, 240.0);
    assert_eq!(
        widgets.iter().map(|w| w.y).collect::<Vec<_>>(),
        expected.iter().map(|w| w.y).collect::<Vec<_>>()
    );
    ui.language_search.set_value("language_29");
    let (widgets, top, _, _) = ui.layout(&s, 480.0, 240.0);
    assert_eq!(ui.scroll, 0.0);
    assert_eq!(widgets.iter().find(|w| w.list).unwrap().y, top + 2.0);
}

#[test]
fn title_shortcuts_initialize_pages_and_return_directly_to_the_title() {
    let mut ui = OptionsUi::default();
    let mut s = Settings::default();
    for page in [Page::Accessibility, Page::Language] {
        ui.language_choice = Some("de_de".into());
        ui.language_search.set_value("old query");
        ui.open_at(false, page);
        assert_eq!(ui.page(), page);
        assert!(!ui.in_world);
        if page == Page::Language {
            assert_eq!(ui.language_choice, None);
            assert!(ui.language_search.value.is_empty());
            assert!(ui.center_language);
        }
        assert!(ui.back(&mut s).contains(&Event::Close));
        assert_eq!(s.language, "en_us");
        ui.open(false);
        ui.show(page);
        assert!(!ui.back(&mut s).contains(&Event::Close));
        assert_eq!(ui.page(), Page::Main);
    }
    ui.open_at(false, Page::Language);
    ui.language_choice = Some("de_de".into());
    let events = ui.back(&mut s);
    assert!(events.contains(&Event::Language("de_de".into())));
    assert!(events.contains(&Event::Close));
    assert_eq!(s.language, "de_de");
}

/// An Extras or PvP & HUD label wider than half a row takes the whole row, so every label
/// fits its button: "Keep channels for UI sounds: Off" did not.
#[test]
fn pvp_labels_fit_their_buttons() {
    // Vanilla's font: 6 px a glyph, a few narrower ones.
    let width = |t: &str| {
        t.chars()
            .map(|c| match c {
                'i' | '\'' | ':' | '.' | ',' | '!' => 2.0,
                'l' => 3.0,
                ' ' | 't' | 'I' => 4.0,
                'f' | 'k' => 5.0,
                _ => 6.0,
            })
            .sum::<f32>()
    };
    let s = Settings::default();
    let mut ui = OptionsUi::default();
    {
        ui.stack = vec![Page::Main, Page::Pvp];
        ui.wide_extras = wide_extras(&s, width);
        let rows = ui.list_rows(&s);
        let buttons: Vec<&Widget> = rows
            .iter()
            .flat_map(|r| match r {
                Row::Buttons(a, b) => std::iter::once(a).chain(b.as_ref()).collect(),
                _ => Vec::new(),
            })
            .collect();
        for b in &buttons {
            let WKind::Button { label, .. } = &b.kind else {
                continue;
            };
            let label = label.plain();
            assert!(width(&label) <= b.w - 4.0, "{label} on {}", b.w);
        }
        // Short ones keep vanilla's two to a row.
        assert!(buttons.iter().any(|b| b.w == 150.0));
    }
}

/// Every setting a player reaches, from the Options screen through every button that opens
/// a page, is in one place, and every page has one way in. Vanilla's own repeats stay as
/// vanilla has them; the graphics screen stands for Video Settings, so the vanilla video
/// options it shows count as Video Settings' (Menu Background Blur is in both of
/// vanilla's). Vanilla's Video Settings rows are all on the graphics screen.
#[test]
fn every_setting_and_page_is_reached_from_one_place() {
    use std::collections::{BTreeMap, BTreeSet};
    let mut s = Settings::default();
    // The Music page shows once its extra is on.
    s.extras.music_page = true;
    let mut ui = OptionsUi::default();
    let mut places: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut ways_in: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut put = |id: String, page: &str| {
        places.entry(id).or_default().insert(page.to_owned());
    };
    // The graphics screen, drawn by `graphics_ui`.
    let mut graphics_rows = BTreeSet::new();
    for (page, groups) in crate::graphics_ui::rows() {
        for row in groups.into_iter().flat_map(|(_, rows)| rows) {
            use crate::graphics_ui::{H, Item};
            let id = match row.item {
                // Vanilla's Graphics API button sets the same setting.
                Item::Horizon(H::GraphicsApi) => "graphics-api".to_owned(),
                Item::Horizon(H::ShaderPacks) => {
                    ways_in
                        .entry(format!("{:?}", Page::ShaderPacks))
                        .or_default()
                        .insert(format!("Graphics/{page:?}"));
                    continue;
                }
                Item::Vanilla(o) => format!("{o:?}"),
                Item::Horizon(h) => format!("graphics:{h:?}"),
                Item::Extra(e) => format!("extra:{e:?}"),
            };
            graphics_rows.insert(id.clone());
            put(id, "Graphics");
        }
    }
    // Every page from the Options screen, through its buttons.
    let mut seen = BTreeSet::new();
    let mut todo = vec![Page::Main];
    while let Some(page) = todo.pop() {
        if !seen.insert(format!("{page:?}")) {
            continue;
        }
        ui.stack = vec![page];
        let (widgets, _, _, _) = ui.layout(&s, 427.0, 4000.0);
        let name = format!("{page:?}");
        for w in &widgets {
            let id = match &w.kind {
                WKind::Slider { opt, .. } => format!("{opt:?}"),
                WKind::Button { act, .. } => match act {
                    Act::Cycle(Opt::GraphicsBackend) => "graphics-api".to_owned(),
                    Act::Cycle(opt) => format!("{opt:?}"),
                    Act::Extra(e) => format!("extra:{e:?}"),
                    Act::PvpPreset => "pvp-preset".to_owned(),
                    Act::Push(to) => {
                        ways_in
                            .entry(format!("{to:?}"))
                            .or_default()
                            .insert(name.clone());
                        // The graphics screen's pages are one screen, listed above.
                        if OptionsUi::graphics_page(*to).is_none() {
                            todo.push(*to);
                        }
                        continue;
                    }
                    _ => continue,
                },
                _ => continue,
            };
            put(id, &name);
        }
    }
    // Vanilla's own: the same option on two of its screens.
    let vanilla_repeats: BTreeMap<&str, &[&str]> = [
        ("MenuBlur", &["Accessibility", "Graphics"][..]),
        ("FovEffects", &["Accessibility", "Graphics"][..]),
        ("ShowSubtitles", &["Accessibility", "Sound"][..]),
        ("Narrator", &["Accessibility", "Chat"][..]),
        ("ChatOpacity", &["Accessibility", "Chat"][..]),
        ("TextBackgroundOpacity", &["Accessibility", "Chat"][..]),
        ("ChatLineSpacing", &["Accessibility", "Chat"][..]),
    ]
    .into();
    for (id, pages) in &places {
        let pages: Vec<&str> = pages.iter().map(String::as_str).collect();
        match vanilla_repeats.get(id.as_str()) {
            Some(expected) => assert_eq!(&pages, expected, "{id}"),
            None => assert_eq!(pages.len(), 1, "{id} is on {pages:?}"),
        }
    }
    // AccessibilityOptionsScreen links Controls, as vanilla's does.
    for (page, from) in &ways_in {
        let from: Vec<&str> = from.iter().map(String::as_str).collect();
        match page.as_str() {
            "Controls" => assert_eq!(from, ["Accessibility", "Main"]),
            _ => assert_eq!(from.len(), 1, "{page} opens from {from:?}"),
        }
    }
    assert_eq!(
        ways_in.get("ShaderPacks").map(|f| f.len()),
        Some(1),
        "{ways_in:?}"
    );
    assert!(
        !seen.contains("Video"),
        "vanilla's own layout has no way in"
    );
    // Every row of vanilla's Video Settings is on the graphics screen.
    ui.stack = vec![Page::Video];
    for row in ui.list_rows(&s) {
        let opts = match row {
            Row::Big(o) => vec![o],
            Row::Pair(a, b) => std::iter::once(a).chain(b).collect(),
            _ => Vec::new(),
        };
        for o in opts {
            let id = match o {
                Opt::GraphicsBackend => "graphics-api".to_owned(),
                o => format!("{o:?}"),
            };
            // macOS has one graphics API: the graphics screen leaves the row out.
            if id == "graphics-api" && cfg!(target_os = "macos") {
                continue;
            }
            assert!(
                graphics_rows.contains(&id),
                "{id} is not on the graphics screen"
            );
        }
    }
}

#[test]
fn horizon_settings_hang_off_options_and_reach_every_horizon_page() {
    let s = Settings::default();
    let mut ui = OptionsUi::default();
    let pushes = |ui: &mut OptionsUi| -> Vec<Page> {
        let (widgets, _, _, _) = ui.layout(&s, 427.0, 240.0);
        widgets
            .iter()
            .filter_map(|w| match &w.kind {
                WKind::Button {
                    act: Act::Push(page),
                    active: true,
                    ..
                } => Some(*page),
                _ => None,
            })
            .collect()
    };
    // Vanilla's Options buttons, Horizon's two in the Telemetry and Credits slots, then
    // Credits & Attribution (WinScreen's credits).
    let main = pushes(&mut ui);
    assert_eq!(
        &main[main.len() - 3..],
        [Page::Horizon, Page::Mods, Page::Credits]
    );
    // Video Settings... is the graphics screen, as Sodium's.
    assert!(main.contains(&Page::Graphics) && !main.contains(&Page::Video));
    ui.push(Page::Horizon);
    assert_eq!(
        pushes(&mut ui),
        [
            Page::Extras,
            Page::Pvp,
            Page::SplitSetup,
            Page::Studio,
            Page::Agent,
            Page::ModBrowser,
        ]
    );
    // No AI hides the AI Agent page.
    let mut no_ai = Settings::default();
    no_ai.extras.no_ai = true;
    let (widgets, _, _, _) = ui.layout(&no_ai, 427.0, 240.0);
    assert!(!widgets.iter().any(|w| matches!(
        &w.kind,
        WKind::Button {
            act: Act::Push(Page::Agent),
            ..
        }
    )));
    // The AI Agent page: the switch, Copy and the status the game sets.
    ui.push(Page::Agent);
    ui.agent_status = vec!["Waiting for an agent.".into()];
    let (widgets, _, _, _) = ui.layout(&s, 427.0, 240.0);
    let acts: Vec<&Act> = widgets
        .iter()
        .filter_map(|w| match &w.kind {
            WKind::Button { act, .. } => Some(act),
            _ => None,
        })
        .collect();
    assert!(acts.contains(&&Act::Extra(crate::extras::Item::AgentControl)));
    assert!(acts.contains(&&Act::CopyAgentDetails));
    ui.stack = vec![Page::Main, Page::Controls];
    assert!(pushes(&mut ui).contains(&Page::Controller));
}

#[test]
fn sound_device_button_in_sound_page_and_cycles() {
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::Sound);
    let rows = ui.list_rows(&s);
    assert!(
        rows.iter().any(|r| matches!(r, Row::Big(Opt::SoundDevice))),
        "Sound page should contain Row::Big(Opt::SoundDevice)"
    );
    assert_eq!(s.sound_device, "");
    let devices = crate::audio::output_devices();
    let count = 1 + devices.len();
    // Cycling forward 'count' times wraps around back to the initial selection ("")
    for _ in 0..count {
        Opt::SoundDevice.cycle(&mut s, false, [854, 480]);
    }
    assert_eq!(s.sound_device, "");
    // Cycling backward 'count' times also wraps around back to ""
    for _ in 0..count {
        Opt::SoundDevice.cycle(&mut s, true, [854, 480]);
    }
    assert_eq!(s.sound_device, "");
}

#[test]
fn slider_keyboard_navigation_and_selection_toggle_parity() {
    use winit::keyboard::KeyCode;
    let atlas = atlas();
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::Main);
    s.fov = 70.0;

    // 1. Tab focuses the first interactive widget: FOV slider
    ui.key(&mut s, &atlas, KeyCode::Tab, None);
    assert_eq!(ui.focus, Some(FocusId::Slider(Opt::Fov)));
    assert!(
        ui.can_change_value,
        "Tab onto slider enables can_change_value (Vanilla behavior)"
    );

    // 2. Right Arrow steps FOV up to 71
    let events = ui.key(&mut s, &atlas, KeyCode::ArrowRight, None);
    assert_eq!(s.fov, 71.0);
    assert!(events.contains(&Event::Changed));

    // 3. Left Arrow steps FOV down back to 70, then 69
    ui.key(&mut s, &atlas, KeyCode::ArrowLeft, None);
    assert_eq!(s.fov, 70.0);
    ui.key(&mut s, &atlas, KeyCode::ArrowLeft, None);
    assert_eq!(s.fov, 69.0);

    // 4. Space / Enter toggles can_change_value to false
    ui.key(&mut s, &atlas, KeyCode::Space, None);
    assert!(
        !ui.can_change_value,
        "Space toggles can_change_value to false"
    );

    // 5. With can_change_value false, Left/Right arrow keys navigate focus instead of changing value!
    ui.key(&mut s, &atlas, KeyCode::ArrowRight, None);
    assert_eq!(
        s.fov, 69.0,
        "FOV must not change when can_change_value is false"
    );
    assert_ne!(
        ui.focus,
        Some(FocusId::Slider(Opt::Fov)),
        "Focus moved to next widget horizontally"
    );

    // 6. Navigate back to FOV slider with Left Arrow
    ui.key(&mut s, &atlas, KeyCode::ArrowLeft, None);
    assert_eq!(ui.focus, Some(FocusId::Slider(Opt::Fov)));
    assert!(
        !ui.can_change_value,
        "Arrow navigation onto slider keeps can_change_value false"
    );

    // 7. Space toggles can_change_value back to true
    ui.key(&mut s, &atlas, KeyCode::Enter, None);
    assert!(ui.can_change_value);
    ui.key(&mut s, &atlas, KeyCode::ArrowRight, None);
    assert_eq!(s.fov, 70.0);

    // 8. Escape closes the options page immediately, even while a slider
    // is focused for value changes.
    let events = ui.key(&mut s, &atlas, KeyCode::Escape, None);
    assert!(!ui.can_change_value);
    assert_eq!(
        ui.page(),
        Page::Main,
        "Escape must close the options page from value edit"
    );
    assert!(events.contains(&Event::Close));
}

#[test]
fn escape_closes_options_with_search_focused() {
    use winit::keyboard::KeyCode;
    let atlas = atlas();
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::Video);
    ui.focus = Some(FocusId::Search);
    ui.settings_search.focused = true;

    let events = ui.key(&mut s, &atlas, KeyCode::Escape, None);
    assert!(events.contains(&Event::Changed));
    assert_eq!(ui.page(), Page::Main);
    assert_eq!(ui.focus, None);
    assert!(!ui.settings_search.focused);
}

#[test]
fn slider_mouse_wheel_scrolls_list_parity() {
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::Main);
    s.fov = 70.0;
    let size: (f32, f32) = (427.0, 240.0);
    let cx = (size.0 / 2.0_f32).floor();

    // Cursor over FOV slider (cx - 154.0, 29.0, 150.0, 20.0)
    let cursor = (cx - 100.0, 35.0);
    // In vanilla parity, mouse wheel over a slider does not change the slider value.
    ui.wheel(&mut s, size, cursor, [854, 480], 1.0);
    assert_eq!(s.fov, 70.0);

    // Sound page: Master Volume slider
    ui.show(Page::Sound);
    s.master_volume = 0.5;
    let (widgets, _, _, _) = ui.layout(&s, size.0, size.1);
    let master = widgets
        .iter()
        .find(|w| {
            matches!(
                w.kind,
                WKind::Slider {
                    opt: Opt::Volume(0),
                    ..
                }
            )
        })
        .unwrap();
    let cursor = (master.x + 10.0, master.y + 10.0);
    let events = ui.wheel(&mut s, size, cursor, [854, 480], 1.0);
    assert_eq!(s.master_volume, 0.5);
    assert!(!events.iter().any(|e| matches!(e, Event::Volume(0, _))));
}

#[test]
fn horizon_pages_slider_and_wheel_parity() {
    use winit::keyboard::KeyCode as K;
    let atlas = atlas();
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();

    // 1. Extras page: with the wheel extra on, the wheel on the Sort switch cycles it
    s.extras.wheel_cycles_settings = true;
    ui.show(Page::Extras);
    let size = (480.0, 600.0);
    let cursor = ui
        .extras
        .point_at(&s, size, ExtraRow::Item(crate::extras::Item::Sort));
    assert!(!s.extras.sort);
    ui.wheel(&mut s, size, cursor, [854, 480], 1.0);
    assert!(s.extras.sort);
    ui.wheel(&mut s, size, cursor, [854, 480], 1.0);
    assert!(!s.extras.sort);

    // 2. Horizon Graphics page through OptionsUi:
    // Tab onto ChunkUpdates slider, ArrowRight adjusts value, Space toggles can_change_value
    ui.show(Page::Graphics);
    ui.graphics.show(crate::graphics_ui::Page::Performance);
    s.horizon.chunk_updates = 10;
    let chunk_updates = crate::graphics_ui::Item::Horizon(crate::graphics_ui::H::ChunkUpdates);
    ui.size = size;
    ui.framebuffer = [854, 480];

    // Tab into controls
    for _ in 0..80 {
        ui.key(&mut s, &atlas, K::Tab, None);
        if ui.graphics.focused_row() == Some(chunk_updates) {
            break;
        }
    }
    assert_eq!(ui.graphics.focused_row(), Some(chunk_updates));
    assert!(ui.graphics.changes_value());

    // Right arrow steps ChunkUpdates to 11
    let events = ui.key(&mut s, &atlas, K::ArrowRight, None);
    assert_eq!(s.horizon.chunk_updates, 11);
    assert!(events.contains(&Event::SliderDrag));

    // Space toggles can_change_value to false
    ui.key(&mut s, &atlas, K::Space, None);
    assert!(!ui.graphics.changes_value());

    // ArrowRight does not change ChunkUpdates when can_change_value is false
    ui.key(&mut s, &atlas, K::ArrowRight, None);
    assert_eq!(s.horizon.chunk_updates, 11);
}

/// Vanilla's wheel scrolls the list (`AbstractScrollArea.mouseScrolled`); with the
/// extra on, a widget focused by the keyboard takes the step from anywhere on the
/// screen. A click focuses its widget for the highlight, and honouring that focus made a
/// clicked option toggle on every later wheel event while the list never scrolled.
#[test]
fn a_clicked_option_lets_the_wheel_scroll_the_list() {
    let atlas = atlas();
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::Video);
    let framebuffer = [854, 480];
    let click_size = (427.0, 400.0);
    let (widgets, click_top, click_bottom, _) = ui.layout(&s, click_size.0, click_size.1);

    // A click leaves the widget focused for its highlight, not for the wheel.
    let wd = widgets
        .iter()
        .find(|wd| {
            matches!(
                wd.kind,
                WKind::Button {
                    act: Act::Cycle(_),
                    ..
                }
            ) && (!wd.list || (wd.y >= click_top && wd.y + wd.h <= click_bottom))
        })
        .expect("a visible cycle button");
    ui.click(
        &mut s,
        &atlas,
        wd.x + 5.0,
        wd.y + 5.0,
        click_size,
        framebuffer,
        false,
        false,
    );
    assert!(
        matches!(ui.focus, Some(FocusId::Cycle(_))),
        "the click focuses the cycle button it hit"
    );
    assert!(
        !ui.focus_from_keyboard,
        "a click does not leave keyboard focus"
    );

    // The wheel over the list's empty margin scrolls it at a size where the
    // page overflows, and the clicked cycle option stays put: no ButtonClick,
    // no Changed.
    let size = (427.0, 240.0);
    let (_, top, bottom, content) = ui.layout(&s, size.0, size.1);
    assert!(
        content > bottom - top,
        "the video page scrolls at this size"
    );
    let before = s.view_bobbing;
    let start = ui.scroll;
    let cursor = (2.0, (top + bottom) / 2.0);
    let events = ui.wheel(&mut s, size, cursor, framebuffer, -1.0);
    assert_eq!(s.view_bobbing, before, "the clicked option does not toggle");
    assert!(!events.contains(&Event::ButtonClick), "no click reaches it");
    assert!(!events.contains(&Event::Changed));
    assert!(ui.scroll > start, "the list scrolls down instead");

    // With the extra on, keyboard focus keeps the documented divergence: the
    // wheel steps it wherever the cursor is, even off the widget.
    s.extras.wheel_cycles_settings = true;
    ui.focus = Some(FocusId::Cycle(Opt::ViewBobbing));
    ui.focus_from_keyboard = true;
    let events = ui.wheel(&mut s, size, cursor, framebuffer, -1.0);
    assert_ne!(s.view_bobbing, before, "a keyboard-focused option cycles");
    assert!(events.contains(&Event::ButtonClick));
    assert!(events.contains(&Event::Changed));
}

#[test]
fn the_wheel_scrolls_a_list_past_its_cycle_buttons_with_the_extra_off() {
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::Video);
    let framebuffer = [854, 480];
    let size = (427.0, 240.0);
    let (widgets, top, bottom, content) = ui.layout(&s, size.0, size.1);
    assert!(
        content > bottom - top,
        "the video page scrolls at this size"
    );
    let (opt, over, list) = widgets
        .iter()
        .find_map(|wd| match wd.kind {
            WKind::Button {
                act: Act::Cycle(opt),
                ..
            } if wd.y >= top && wd.y + wd.h <= bottom => {
                Some((opt, (wd.x + 5.0, wd.y + 5.0), wd.list))
            }
            _ => None,
        })
        .expect("a visible cycle button");

    assert!(list, "{opt:?} is a row of the video list");
    // The default, as in vanilla (#9633).
    assert!(!s.extras.wheel_cycles_settings);
    let before = format!("{s:?}");
    let start = ui.scroll;
    let events = ui.wheel(&mut s, size, over, framebuffer, -1.0);
    assert_eq!(format!("{s:?}"), before, "the hovered option stays put");
    assert!(!events.contains(&Event::Changed));
    assert!(
        ui.scroll > start,
        "the list scrolls under the cursor instead"
    );
    // Keyboard focus doesn't take the wheel either.
    ui.focus = Some(FocusId::Cycle(opt));
    ui.focus_from_keyboard = true;
    ui.wheel(&mut s, size, over, framebuffer, -1.0);
    assert_eq!(format!("{s:?}"), before);

    // On, the row under the cursor takes the step.
    s.extras.wheel_cycles_settings = true;
    let before = format!("{s:?}");
    let events = ui.wheel(&mut s, size, over, framebuffer, -1.0);
    assert_ne!(format!("{s:?}"), before, "{opt:?} cycles");
    assert!(events.contains(&Event::Changed));
}

#[test]
fn options_step_slider_matches_set_slider_for_all_stepped_options() {
    let stepped_opts = [
        Opt::Fov,
        Opt::RenderDistance,
        Opt::SimulationDistance,
        Opt::MaxFps,
        Opt::CloudRange,
        Opt::EntityDistance,
        Opt::BiomeBlend,
        Opt::Preset,
        Opt::Mipmaps,
        Opt::WeatherRadius,
        Opt::ChunkFade,
        Opt::MenuBlur,
        Opt::NotificationDisplayTime,
        Opt::PadMoveDeadzone,
        Opt::PadLookDeadzone,
        Opt::PadTriggerThreshold,
        Opt::SprintWindow,
    ];
    for opt in stepped_opts {
        let steps = opt.slider_steps().expect("slider steps");
        for i in 0..steps {
            let pos = i as f32 / steps as f32;
            let mut s1 = Settings::default();
            let mut s2 = Settings::default();
            opt.set_slider(&mut s1, pos);

            opt.step_slider(&mut s1, true, 200.0);
            let next_pos = (i + 1) as f32 / steps as f32;
            opt.set_slider(&mut s2, next_pos);
            assert_eq!(
                opt.slider_position(&s1),
                opt.slider_position(&s2),
                "{opt:?} forward from {i}"
            );
        }
        for i in 1..=steps {
            let pos = i as f32 / steps as f32;
            let mut s1 = Settings::default();
            let mut s2 = Settings::default();
            opt.set_slider(&mut s1, pos);

            opt.step_slider(&mut s1, false, 200.0);
            let prev_pos = (i - 1) as f32 / steps as f32;
            opt.set_slider(&mut s2, prev_pos);
            assert_eq!(
                opt.slider_position(&s1),
                opt.slider_position(&s2),
                "{opt:?} backward from {i}"
            );
        }
    }
}

#[test]
fn options_keyboard_space_enter_emits_button_click_for_cycle_and_default_buttons() {
    use winit::keyboard::KeyCode;
    let mut ui = OptionsUi::default();
    let mut s = Settings::default();
    let atlas = atlas();

    // 1. Focus a cycle button (e.g. Opt::ViewBobbing)
    ui.focus = Some(FocusId::Cycle(Opt::ViewBobbing));
    let events = ui.key(&mut s, &atlas, KeyCode::Space, None);
    assert!(
        events.contains(&Event::ButtonClick),
        "Space on FocusId::Cycle must emit Event::ButtonClick"
    );
    assert!(events.contains(&Event::Changed));

    let events = ui.key(&mut s, &atlas, KeyCode::Enter, None);
    assert!(
        events.contains(&Event::ButtonClick),
        "Enter on FocusId::Cycle must emit Event::ButtonClick"
    );
    assert!(events.contains(&Event::Changed));

    // 2. Focus a default/action button (e.g. Act::Done)
    ui.focus = Some(FocusId::Act(Act::Done));
    let events = ui.key(&mut s, &atlas, KeyCode::Space, None);
    assert!(
        events.contains(&Event::ButtonClick),
        "Space on FocusId::Act must emit Event::ButtonClick"
    );

    ui.focus = Some(FocusId::Act(Act::Done));
    let events = ui.key(&mut s, &atlas, KeyCode::Enter, None);
    assert!(
        events.contains(&Event::ButtonClick),
        "Enter on FocusId::Act must emit Event::ButtonClick"
    );
}

/// Browse Mods is on the Horizon settings page (the Mods footer has only Open Mods Folder
/// and Back): clicking it lists the registry's mods and opens the browser.
#[test]
fn browse_mods_opens_the_registry_browser_from_the_horizon_page() {
    let atlas = atlas();
    let size = (960.0, 540.0);
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::Horizon);
    let (widgets, ..) = ui.layout(&s, size.0, size.1);
    let browse = widgets
        .into_iter()
        .find(
            |w| matches!(&w.kind, WKind::Button { act, .. } if *act == Act::Push(Page::ModBrowser)),
        )
        .expect("Browse Mods... on the Horizon page");
    let events = ui.click(
        &mut s,
        &atlas,
        browse.x + browse.w / 2.0,
        browse.y + browse.h / 2.0,
        size,
        [854, 480],
        false,
        false,
    );
    assert_eq!(events, vec![Event::ButtonClick, Event::BrowseMods]);
    assert_eq!(ui.page(), Page::ModBrowser);
}

#[test]
fn the_music_page_button_follows_the_extra() {
    let atlas = atlas();
    let size = (960.0, 540.0);
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::Sound);
    let open = |ui: &mut OptionsUi, s: &Settings| {
        let (widgets, ..) = ui.layout(s, size.0, size.1);
        widgets.into_iter().find(
            |w| matches!(&w.kind, WKind::Button { act, .. } if *act == Act::Push(Page::Music)),
        )
    };
    // The extra is off by default: Music & Sounds is vanilla's page.
    assert!(open(&mut ui, &s).is_none());
    s.extras.music_page = true;
    assert!(open(&mut ui, &s).is_some(), "Music... once the extra is on");
    // It is the last row: scroll the list to its end before clicking it.
    ui.scroll(&s, size, -1000.0);
    let (widgets, ..) = ui.layout(&s, size.0, size.1);
    let open_button = widgets
        .into_iter()
        .find(|w| matches!(&w.kind, WKind::Button { act, .. } if *act == Act::Push(Page::Music)))
        .expect("Music... on the Music & Sounds page");
    let events = ui.click(
        &mut s,
        &atlas,
        open_button.x + open_button.w / 2.0,
        open_button.y + open_button.h / 2.0,
        size,
        [854, 480],
        false,
        false,
    );
    assert_eq!(ui.page(), Page::Music);
    assert!(events.contains(&Event::ListMusicTracks));
}

#[test]
fn the_music_page_plays_a_track_now_and_turns_tracks_off() {
    let atlas = atlas();
    let size = (960.0, 540.0);
    let mut s = Settings::default();
    let mut ui = OptionsUi {
        music_groups: vec![MusicGroup {
            label: "horizon.pages.music.overworld".into(),
            tracks: vec![
                MusicTrack {
                    event: "minecraft:music.game".into(),
                    id: "minecraft:music/game/calm1".into(),
                },
                MusicTrack {
                    event: "minecraft:music.overworld.forest".into(),
                    id: "minecraft:music/game/bromeliad".into(),
                },
            ],
        }],
        ..OptionsUi::default()
    };
    ui.show(Page::Music);
    let at = |ui: &mut OptionsUi, s: &Settings, want: &Act| {
        let (widgets, ..) = ui.layout(s, size.0, size.1);
        let w = widgets
            .into_iter()
            .find(|w| matches!(&w.kind, WKind::Button { act, .. } if act == want))
            .expect("button");
        (w.x + w.w / 2.0, w.y + w.h / 2.0)
    };
    // The left of a row plays that track now, from its own pool (a biome
    // pool's track plays from that pool).
    let calm1 = "minecraft:music/game/calm1";
    let (x, y) = at(
        &mut ui,
        &s,
        &Act::PlayMusicTrack("minecraft:music.game".into(), calm1.into()),
    );
    let events = ui.click(&mut s, &atlas, x, y, size, [854, 480], false, false);
    assert!(events.contains(&Event::PlayMusicTrack(
        "minecraft:music.game".into(),
        calm1.into()
    )));
    let bromeliad = "minecraft:music/game/bromeliad";
    let (x, y) = at(
        &mut ui,
        &s,
        &Act::PlayMusicTrack("minecraft:music.overworld.forest".into(), bromeliad.into()),
    );
    let events = ui.click(&mut s, &atlas, x, y, size, [854, 480], false, false);
    assert!(events.contains(&Event::PlayMusicTrack(
        "minecraft:music.overworld.forest".into(),
        bromeliad.into()
    )));
    // The right of a row turns it off, and the game saves that on `Changed`.
    let (x, y) = at(&mut ui, &s, &Act::MusicTrackOn(calm1.into(), true));
    let events = ui.click(&mut s, &atlas, x, y, size, [854, 480], false, false);
    assert!(events.contains(&Event::Changed));
    assert_eq!(s.extras.music_off, [calm1.to_owned()]);
    // And back on.
    let (x, y) = at(&mut ui, &s, &Act::MusicTrackOn(calm1.into(), false));
    let events = ui.click(&mut s, &atlas, x, y, size, [854, 480], false, false);
    assert!(events.contains(&Event::Changed));
    assert!(s.extras.music_off.is_empty());
}

#[test]
fn options_cycle_button_cycles_forward_and_shift_click_cycles_backward_with_wrapping() {
    let atlas = atlas();
    let mut s = Settings::default();
    let mut ui = OptionsUi::default();
    ui.show(Page::Chat);
    let size = (480.0, 480.0);
    let (widgets, _, _, _) = ui.layout(&s, size.0, size.1);
    let button = widgets
        .iter()
        .find(|wd| {
            matches!(
                wd.kind,
                WKind::Button {
                    active: true,
                    act: Act::Cycle(Opt::ChatVisibility),
                    ..
                }
            )
        })
        .expect("ChatVisibility must be a cycle button");
    let (x, y) = (button.x + 1.0, button.y + 1.0);

    // Initial chat_visibility is 0
    assert_eq!(s.chat_visibility, 0);

    // 1. Feature without shift (normal click): cycles forward (+1)
    assert_eq!(
        ui.click(&mut s, &atlas, x, y, size, [854, 480], false, false),
        vec![Event::ButtonClick, Event::Changed]
    );
    assert_eq!(s.chat_visibility, 1);

    assert_eq!(
        ui.click(&mut s, &atlas, x, y, size, [854, 480], false, false),
        vec![Event::ButtonClick, Event::Changed]
    );
    assert_eq!(s.chat_visibility, 2);

    // 3. Wrap-around forward (at max value, normal click wraps to min / 0)
    assert_eq!(
        ui.click(&mut s, &atlas, x, y, size, [854, 480], false, false),
        vec![Event::ButtonClick, Event::Changed]
    );
    assert_eq!(s.chat_visibility, 0);

    // 4. Wrap-around backward (at min / 0, shift-click wraps to max)
    ui.shift = true;
    assert_eq!(
        ui.click(&mut s, &atlas, x, y, size, [854, 480], false, false),
        vec![Event::ButtonClick, Event::Changed]
    );
    assert_eq!(s.chat_visibility, 2);

    // 2. Feature with shift: cycles backward (-1)
    assert_eq!(
        ui.click(&mut s, &atlas, x, y, size, [854, 480], false, false),
        vec![Event::ButtonClick, Event::Changed]
    );
    assert_eq!(s.chat_visibility, 1);

    assert_eq!(
        ui.click(&mut s, &atlas, x, y, size, [854, 480], false, false),
        vec![Event::ButtonClick, Event::Changed]
    );
    assert_eq!(s.chat_visibility, 0);
    ui.shift = false;

    // Also test Opt::Particles on Page::Video
    ui.show(Page::Video);
    let (widgets, _, _, _) = ui.layout(&s, size.0, size.1);
    let button = widgets
        .iter()
        .find(|wd| {
            matches!(
                wd.kind,
                WKind::Button {
                    active: true,
                    act: Act::Cycle(Opt::Particles),
                    ..
                }
            )
        })
        .expect("Particles must be a cycle button");
    let (px, py) = (button.x + 1.0, button.y + 1.0);

    // Initial particles is 0
    assert_eq!(s.particles, 0);

    // Forward
    ui.click(&mut s, &atlas, px, py, size, [854, 480], false, false);
    assert_eq!(s.particles, 1);
    ui.click(&mut s, &atlas, px, py, size, [854, 480], false, false);
    assert_eq!(s.particles, 2);
    // Wrap forward
    ui.click(&mut s, &atlas, px, py, size, [854, 480], false, false);
    assert_eq!(s.particles, 0);

    // Wrap backward with shift
    ui.shift = true;
    ui.click(&mut s, &atlas, px, py, size, [854, 480], false, false);
    assert_eq!(s.particles, 2);
    // Backward with shift
    ui.click(&mut s, &atlas, px, py, size, [854, 480], false, false);
    assert_eq!(s.particles, 1);
    ui.click(&mut s, &atlas, px, py, size, [854, 480], false, false);
    assert_eq!(s.particles, 0);
    ui.shift = false;
}
