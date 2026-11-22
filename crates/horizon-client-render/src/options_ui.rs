//! The options screens: OptionsScreen, Video Settings, Music & Sounds,
//! Controls, Key Binds, Mouse Settings, Chat Settings, Accessibility,
//! Language and Resource Packs, laid out like vanilla's HeaderAndFooter /
//! OptionsList screens and drawn with the shared widgets.
mod draw;
mod pack_drop;

use std::time::Instant;

pub use crate::settings::Opt;

#[path = "options_ui_widgets.rs"]
mod widgets;
use crate::{
    gamepad::{self, GlyphStyle},
    hdr_calibration::Picture,
    hud::{HudAtlas, HudBatch, rgba},
    keybinds::{self, CATEGORIES, KeyBinds},
    lang::{self, LanguageInfo},
    settings::{SOUND_SOURCES, Settings},
    settings_shell as shell,
    text::{self, Style, Text},
    ui,
};
pub(crate) use widgets::{button, inactive};

#[path = "options_ui_values.rs"]
mod values;
pub(crate) use values::{chat_delay_label, chat_px, generic, on_off, percent};

mod layout;
mod shell_pages;

use self::layout::OptionsLayout;
#[path = "options_ui_wheel.rs"]
mod wheel;
pub(crate) use wheel::slider_unsnapped;

fn tr(key: &str) -> Text {
    Text::translate(key, &[])
}

/// False only in a game built without the `horizon-extras` feature (docs/forking.md), which says
/// so once at startup: the Options screen then has vanilla's Telemetry Data slot instead of
/// Horizon Settings and Mods, and Credits & Attribution names the official build.
pub static HORIZON_EXTRAS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
fn horizon_extras() -> bool {
    HORIZON_EXTRAS.load(std::sync::atomic::Ordering::Relaxed)
}
/// `text` cut into lines of at most `width` characters, at spaces where it has them.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let len = line.chars().count();
        if len > 0 && len + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
        while line.chars().count() > width {
            let rest: String = line.chars().skip(width).collect();
            line = line.chars().take(width).collect();
            lines.push(std::mem::replace(&mut line, rest));
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}
/// Setting `j` of the native mod in row `i` of Options > Mods.
fn mod_setting(s: &Settings, i: u16, j: u16) -> Option<&crate::mod_list::NativeSetting> {
    s.mods
        .installed
        .get(usize::from(i))?
        .native
        .as_ref()?
        .settings
        .get(usize::from(j))
}
/// A native mod's setting as its row says it: the label, then the value.
fn mod_setting_label(setting: &crate::mod_list::NativeSetting) -> Text {
    use crate::mod_list::NativeSettingValue as V;
    let value = match &setting.value {
        V::Toggle(on) => on_off(*on),
        V::Choice { options, index } => options
            .get(*index)
            .map_or_else(|| Text::literal(""), |o| tr(o)),
        V::Range { value, .. } => {
            let shown = format!("{value:.2}");
            Text::literal(shown.trim_end_matches('0').trim_end_matches('.'))
        }
        V::Color(rgb) => Text::literal(format!("#{rgb:06X}")).with_color(*rgb),
    };
    Text::option_name_value(tr(&setting.label), value)
}
/// A music track's name as its row shows it: the file, without its folder
/// (`minecraft:music/game/calm1` -> `calm1`; the group names the situation).
fn track_name(id: &str) -> &str {
    id.rsplit('/').next().unwrap_or(id)
}
pub(crate) enum Kind {
    /// Slider position 0..1.
    Slider(f32),
    Cycle,
}
/// `PlayerModelPart` keys, in vanilla's bit order (`modelPart_*` in options.txt).
const MODEL_PART_KEYS: [&str; 7] = [
    "modelPart_cape",
    "modelPart_jacket",
    "modelPart_left_sleeve",
    "modelPart_right_sleeve",
    "modelPart_left_pants_leg",
    "modelPart_right_pants_leg",
    "modelPart_hat",
];
/// The Accessibility Settings page:
/// `AccessibilityOptionsScreen.options`, with the Controls link inserted separately.
pub(crate) const ACCESSIBILITY_OPTIONS: &[Opt] = &[
    Opt::Narrator,
    Opt::ShowSubtitles,
    Opt::HighContrast,
    Opt::MenuBlur,
    Opt::TextBackgroundOpacity,
    Opt::TextBackground,
    Opt::ChatOpacity,
    Opt::ChatLineSpacing,
    Opt::ChatDelay,
    Opt::NotificationDisplayTime,
    Opt::ViewBobbing,
    Opt::DistortionEffects,
    Opt::FovEffects,
    Opt::DarknessPulsing,
    Opt::DamageTilt,
    Opt::GlintSpeed,
    Opt::GlintStrength,
    Opt::HideLightningFlashes,
    Opt::MonochromeLogo,
    Opt::PanoramaSpeed,
    Opt::HideSplashTexts,
    Opt::NarratorHotkey,
    Opt::RotateWithMinecarts,
    Opt::HighContrastBlockOutlines,
];

/// `ControlsScreen.options`: the controls added by Java Edition 26.3, in
/// their vanilla order. The macOS-only Control-click switch is omitted on
/// other platforms just as `InputQuirks.EMULATE_RIGHT_CLICK_WITH_CTRL_KEY`
/// does in Java.
pub(crate) fn vanilla_control_options() -> Vec<Opt> {
    let mut options = vec![
        Opt::ToggleCrouch,
        Opt::ToggleSprint,
        Opt::ToggleAttack,
        Opt::ToggleUse,
        Opt::AutoJump,
        Opt::SprintWindow,
        Opt::QuitShortcuts,
    ];
    if cfg!(target_os = "macos") {
        options.push(Opt::CtrlClickEmulatesRightClick);
    }
    options.push(Opt::OperatorItemsTab);
    options
}

/// `MouseSettingsScreen.options` in Java Edition 26.3, in vanilla order.
pub(crate) const MOUSE_OPTIONS: &[Opt] = &[
    Opt::Sensitivity,
    Opt::WheelSensitivity,
    Opt::DiscreteScroll,
    Opt::InvertX,
    Opt::InvertY,
    Opt::AllowCursorChanges,
];

impl Opt {
    /// Keep controls visible while their underlying systems are still missing.
    pub(crate) fn available(self) -> bool {
        !matches!(
            self,
            // Stored, but there are no blocked players or signed chat yet.
            Opt::HideMatchedNames
                | Opt::OnlySecureChat
                // Saved, but no Horizon system reads them yet: no Realms, no
                // friends list, no directional audio.
                | Opt::RealmsNotifications
                | Opt::InGameNotification
                | Opt::SharePresence
                | Opt::DirectionalAudio
                // Saved, but nothing in rendering reads it yet.
                | Opt::ChunkBuilder
        )
    }
    fn file_key(self) -> Option<&'static str> {
        Some(match self {
            Opt::Narrator => "narrator",
            Opt::TextBackground => "backgroundForChatOnly",
            Opt::ChatDelay => "chatDelay",
            Opt::HideMatchedNames => "hideMatchedNames",
            Opt::OnlySecureChat => "onlyShowSecureChat",
            Opt::SaveChatDrafts => "saveChatDrafts",
            Opt::DistortionEffects => "screenEffectScale",
            Opt::FovEffects => "fovEffectScale",
            Opt::DarknessPulsing => "darknessEffectScale",
            Opt::DamageTilt => "damageTiltStrength",
            Opt::GlintSpeed => "glintSpeed",
            Opt::GlintStrength => "glintStrength",
            Opt::HideLightningFlashes => "hideLightningFlashes",
            Opt::MonochromeLogo => "darkMojangStudiosBackground",
            Opt::PanoramaSpeed => "panoramaScrollSpeed",
            Opt::HideSplashTexts => "hideSplashTexts",
            Opt::NarratorHotkey => "narratorHotkey",
            Opt::RotateWithMinecarts => "rotateWithMinecart",
            Opt::HighContrastBlockOutlines => "highContrastBlockOutline",
            Opt::MusicFrequency => "musicFrequency",
            Opt::SoundDevice => "soundDevice",
            Opt::OperatorItemsTab => "operatorItemsTab",
            Opt::QuitShortcuts => "quitShortcuts",
            Opt::CtrlClickEmulatesRightClick => "ctrlClickEmulatesRightClick",
            Opt::ModelPart(i) => MODEL_PART_KEYS[i],
            Opt::AllowServerListing => "allowServerListing",
            Opt::RealmsNotifications => "realmsNotifications",
            Opt::InGameNotification => "inGameNotification",
            Opt::SharePresence => "sharePresence",
            Opt::DirectionalAudio => "directionalAudio",
            Opt::MusicToast => "musicToast",
            _ => return None,
        })
    }

    pub(crate) fn caption(self) -> &'static str {
        match self {
            Opt::Fov => "options.fov",
            Opt::RenderDistance => "options.renderDistance",
            Opt::SimulationDistance => "options.simulationDistance",
            Opt::Sensitivity => "options.sensitivity",
            Opt::WheelSensitivity => "options.mouseWheelSensitivity",
            Opt::InvertX => "options.invertMouseX",
            Opt::InvertY => "options.invertMouseY",
            Opt::DiscreteScroll => "options.discrete_mouse_scroll",
            Opt::AllowCursorChanges => "options.allowCursorChanges",
            Opt::Volume(_) => "soundCategory.master",
            Opt::ModSetting(..) => "horizon.mods.settings",
            Opt::SoundDevice => "options.audioDevice",
            Opt::GuiScale => "options.guiScale",
            Opt::ViewBobbing => "options.viewBobbing",
            Opt::Gamma => "options.gamma",
            Opt::MaxFps => "options.framerateLimit",
            Opt::Vsync => "options.vsync",
            Opt::Fullscreen => "options.fullscreen",
            Opt::ExclusiveFullscreen => "options.exclusiveFullscreen",
            Opt::ExclusiveMode => "options.fullscreen.exclusive.mode",
            Opt::GraphicsBackend => "options.graphicsApi",
            Opt::TextureFiltering => "options.textureFiltering",
            Opt::MaxAnisotropy => "options.maxAnisotropy",
            Opt::SmoothLighting => "options.ao",
            Opt::Clouds => "options.renderClouds",
            Opt::CloudRange => "options.renderCloudsDistance",
            Opt::Particles => "options.particles",
            Opt::EntityDistance => "options.entityDistanceScaling",
            Opt::EntityShadows => "options.entityShadows",
            Opt::BiomeBlend => "options.biomeBlendRadius",
            Opt::Preset => "options.graphics.preset",
            Opt::Mipmaps => "options.mipmapLevels",
            Opt::WeatherRadius => "options.weatherRadius",
            Opt::ChunkFade => "options.chunkFade",
            Opt::Vignette => "options.vignette",
            Opt::Autosave => "options.autosaveIndicator",
            Opt::CutoutLeaves => "options.cutoutLeaves",
            Opt::ImprovedTransparency => "options.improvedTransparency",
            Opt::AttackIndicator => "options.attackIndicator",
            Opt::InactivityFps => "options.inactivityFpsLimit",
            Opt::ChunkBuilder => "options.prioritizeChunkUpdates",
            Opt::MenuBlur => "options.accessibility.menu_background_blurriness",
            Opt::ToggleCrouch => "key.sneak",
            Opt::ToggleSprint => "key.sprint",
            Opt::ToggleAttack => "key.attack",
            Opt::ToggleUse => "key.use",
            Opt::AutoJump => "options.autoJump",
            Opt::SprintWindow => "options.sprintWindow",
            Opt::QuitShortcuts => "options.quitShortcuts",
            Opt::CtrlClickEmulatesRightClick => "options.ctrlClickEmulatesRightClick",
            Opt::OperatorItemsTab => "options.operatorItemsTab",
            Opt::ChatVisibility => "options.chat.visibility",
            Opt::ChatColors => "options.chat.color",
            Opt::ChatLinks => "options.chat.links",
            Opt::ChatLinksPrompt => "options.chat.links.prompt",
            Opt::ChatOpacity => "options.chat.opacity",
            Opt::TextBackgroundOpacity => "options.accessibility.text_background_opacity",
            Opt::ChatScale => "options.chat.scale",
            Opt::ChatWidth => "options.chat.width",
            Opt::ChatHeightFocused => "options.chat.height.focused",
            Opt::ChatHeightUnfocused => "options.chat.height.unfocused",
            Opt::ChatLineSpacing => "options.chat.line_spacing",
            Opt::AutoSuggestions => "options.autoSuggestCommands",
            Opt::HideMatchedNames => "options.hideMatchedNames",
            Opt::OnlySecureChat => "options.onlyShowSecureChat",
            Opt::SaveChatDrafts => "options.chat.drafts",
            Opt::ReducedDebugInfo => "options.reducedDebugInfo",
            Opt::HighContrast => "options.accessibility.high_contrast",
            Opt::ShowSubtitles => "options.showSubtitles",
            Opt::MusicFrequency => "options.music_frequency",
            Opt::NotificationDisplayTime => "options.notifications.display_time",
            Opt::Narrator => "options.narrator",
            Opt::TextBackground => "options.accessibility.text_background",
            Opt::ChatDelay => "horizon.options.chat_delay",
            Opt::DistortionEffects => "options.screenEffectScale",
            Opt::FovEffects => "options.fovEffectScale",
            Opt::DarknessPulsing => "options.darknessEffectScale",
            Opt::DamageTilt => "options.damageTiltStrength",
            Opt::GlintSpeed => "options.glintSpeed",
            Opt::GlintStrength => "options.glintStrength",
            Opt::HideLightningFlashes => "options.hideLightningFlashes",
            Opt::MonochromeLogo => "options.darkMojangStudiosBackgroundColor",
            Opt::PanoramaSpeed => "options.accessibility.panorama_speed",
            Opt::HideSplashTexts => "options.hideSplashTexts",
            Opt::NarratorHotkey => "options.accessibility.narrator_hotkey",
            Opt::RotateWithMinecarts => "options.rotateWithMinecart",
            Opt::HighContrastBlockOutlines => "options.accessibility.high_contrast_block_outline",
            Opt::ForceUnicode => "options.forceUnicodeFont",
            Opt::JapaneseGlyphs => "options.japaneseGlyphVariants",
            // Horizon's own options: vanilla has no keys for them, so they read
            // the namespace the client bundles (assets/horizon/lang).
            Opt::PadEnabled => "horizon.options.controller",
            Opt::PadVibration => "horizon.options.vibration",
            Opt::PadPlayInBackground => "horizon.options.play_in_background",
            Opt::PadAdaptiveTriggers => "horizon.options.adaptive_triggers",
            Opt::PadLightbar => "horizon.options.lightbar",
            Opt::PadLookSensitivity => "horizon.options.look_sensitivity",
            Opt::PadPointerSpeed => "horizon.options.pointer_speed",
            Opt::PadScrollSpeed => "horizon.options.scroll_speed",
            Opt::PadRepeatDelay => "horizon.options.repeat_delay",
            Opt::PadRepeatInterval => "horizon.options.repeat_interval",
            Opt::PadInvertX => "horizon.options.invert_x",
            Opt::PadInvertY => "horizon.options.invert_y",
            Opt::PadCraftOnServers => "horizon.options.craft_on_servers",
            Opt::PadToggleSneak => "horizon.options.sneak_button",
            Opt::PadMoveDeadzone => "horizon.options.move_deadzone",
            Opt::PadLookDeadzone => "horizon.options.look_deadzone",
            Opt::PadTriggerThreshold => "horizon.options.trigger_threshold",
            Opt::PadGlyphs => "horizon.options.button_glyphs",
            Opt::PadNintendoButtons => "horizon.options.nintendo_buttons",
            Opt::PadKeyboard => "horizon.options.on_screen_keyboard",
            Opt::PadGameplayGuide => "horizon.options.gameplay_guide",
            Opt::PadRadialMode => "horizon.options.radial_mode",
            Opt::PadRadialStick => "horizon.options.radial_stick",
            Opt::ModelPart(i) => [
                "options.modelPart.cape",
                "options.modelPart.jacket",
                "options.modelPart.left_sleeve",
                "options.modelPart.right_sleeve",
                "options.modelPart.left_pants_leg",
                "options.modelPart.right_pants_leg",
                "options.modelPart.hat",
            ][i],
            Opt::SkinLayers3d => "horizon.options.three_d_skin_layers",
            Opt::MainHand => "options.mainHand",
            Opt::AllowServerListing => "options.allowServerListing",
            Opt::RealmsNotifications => "options.realmsNotifications.button",
            Opt::InGameNotification => "options.inGameNotification",
            Opt::SharePresence => "options.sharePresence",
            Opt::DirectionalAudio => "options.directionalAudio",
            Opt::MusicToast => "options.musicToast",
            Opt::Page(page, row) => pages::slider_caption(page, row),
        }
    }
    pub(crate) fn kind(self, s: &Settings) -> Kind {
        let unit = |v: f32, lo: f32, hi: f32| Kind::Slider(((v - lo) / (hi - lo)).clamp(0.0, 1.0));
        match self {
            Opt::ModSetting(i, j) => {
                Kind::Slider(mod_setting(s, i, j).map_or(0.0, |m| m.value.fraction()))
            }
            Opt::ChatDelay => values::kind(s.file.double("chatDelay")),
            Opt::FovEffects | Opt::DarknessPulsing => {
                Kind::Slider(s.file.double(self.file_key().unwrap()).sqrt() as f32)
            }
            Opt::DistortionEffects
            | Opt::DamageTilt
            | Opt::GlintSpeed
            | Opt::GlintStrength
            | Opt::PanoramaSpeed => Kind::Slider(s.file.double(self.file_key().unwrap()) as f32),
            Opt::Fov => unit(s.fov, 30.0, 110.0),
            Opt::RenderDistance => unit(
                s.render_distance as f32,
                2.0,
                crate::options_file::MAX_RENDER_DISTANCE as f32,
            ),
            Opt::SimulationDistance => unit(s.simulation_distance as f32, 5.0, 32.0),
            Opt::Sensitivity => Kind::Slider(s.sensitivity),
            Opt::WheelSensitivity => wheel::kind(s.mouse_wheel_sensitivity),
            Opt::Volume(i) => Kind::Slider(if i == 0 {
                s.master_volume
            } else {
                s.volumes[i]
            }),
            Opt::Gamma => Kind::Slider(s.gamma),
            Opt::MaxFps => unit(s.max_fps as f32, 10.0, 260.0),
            Opt::CloudRange => unit(s.cloud_range as f32, 2.0, 128.0),
            Opt::EntityDistance => unit(s.entity_distance, 0.5, 5.0),
            Opt::BiomeBlend => unit(s.biome_blend as f32, 0.0, 7.0),
            Opt::Preset => unit(preset_index(&s.graphics_preset) as f32, 0.0, 3.0),
            Opt::Mipmaps => unit(s.mipmap_levels as f32, 0.0, 4.0),
            Opt::MaxAnisotropy => unit(s.file.int("maxAnisotropyBit") as f32, 1.0, 3.0),
            Opt::ExclusiveMode => {
                let last = s.video_modes.len() as f32 - 1.0;
                if last < 0.0 {
                    Kind::Slider(0.0)
                } else {
                    unit(s.video_mode_index() as f32, -1.0, last)
                }
            }
            Opt::WeatherRadius => unit(s.weather_radius as f32, 3.0, 10.0),
            Opt::ChunkFade => unit(s.chunk_fade, 0.0, 2.0),
            Opt::MenuBlur => unit(s.menu_blur as f32, 0.0, 10.0),
            Opt::NotificationDisplayTime => unit(s.notification_display_time, 0.5, 10.0),
            Opt::ChatOpacity => Kind::Slider(s.chat_opacity),
            Opt::TextBackgroundOpacity => Kind::Slider(s.text_background_opacity),
            Opt::ChatScale => Kind::Slider(s.chat_scale),
            Opt::ChatWidth => Kind::Slider(s.chat_width),
            Opt::ChatHeightFocused => Kind::Slider(s.chat_height_focused),
            Opt::ChatHeightUnfocused => Kind::Slider(s.chat_height_unfocused),
            Opt::ChatLineSpacing => Kind::Slider(s.chat_line_spacing),
            Opt::PadLookSensitivity => Kind::Slider(s.controller.look_sensitivity),
            Opt::PadPointerSpeed => unit(s.controller.pointer_speed, 0.25, 3.0),
            Opt::PadScrollSpeed => unit(s.controller.scroll_speed, 1.0, 36.0),
            Opt::PadRepeatDelay => unit(s.controller.repeat_delay, 0.1, 1.0),
            Opt::PadRepeatInterval => unit(s.controller.repeat_interval, 0.03, 0.3),
            Opt::PadMoveDeadzone => unit(s.controller.move_deadzone, 0.0, 0.5),
            Opt::PadLookDeadzone => unit(s.controller.look_deadzone, 0.0, 0.5),
            Opt::PadTriggerThreshold => unit(s.controller.trigger_threshold, 0.1, 0.9),
            Opt::SprintWindow => unit(s.sprint_window as f32, 0.0, 10.0),
            Opt::Page(page, row) => pages::slider_kind(page, row),
            _ => Kind::Cycle,
        }
    }
    /// OptionInstance's static or value-dependent tooltip translation.
    fn tooltip_key(self, s: &Settings) -> String {
        if let Opt::ModSetting(i, j) = self {
            return mod_setting(s, i, j)
                .and_then(|m| m.tooltip.clone())
                .unwrap_or_default();
        }
        let key = match self {
            Opt::InactivityFps => {
                if s.inactivity_fps_limit == "minimized" {
                    "options.inactivityFpsLimit.minimized"
                } else {
                    "options.inactivityFpsLimit.afk"
                }
            }
            Opt::ChunkBuilder => [
                "options.prioritizeChunkUpdates.none",
                "options.prioritizeChunkUpdates.byPlayer",
                "options.prioritizeChunkUpdates.nearby",
            ][usize::from(s.prioritize_chunk_updates.min(2))],
            // Options.exclusiveFullscreen: a tooltip per value.
            Opt::ExclusiveFullscreen => {
                if s.file.bool("exclusiveFullscreen") {
                    "options.exclusiveFullscreen.on"
                } else {
                    "options.exclusiveFullscreen.off"
                }
            }
            Opt::TextureFiltering => [
                "options.textureFiltering.none",
                "options.textureFiltering.rgss",
                "options.textureFiltering.anisotropic",
            ][s.file.int("textureFiltering").clamp(0, 2) as usize],
            // InputQuirks.REPLACE_CTRL_KEY_WITH_CMD_KEY: Cmd+B on macOS.
            Opt::NarratorHotkey if cfg!(target_os = "macos") => {
                "options.accessibility.narrator_hotkey.mac"
            }
            _ => self.caption(),
        };
        format!("{key}.tooltip")
    }
    /// A slider's position (0..1), or `None` for a cycle button.
    pub fn slider_position(self, s: &Settings) -> Option<f32> {
        match self.kind(s) {
            Kind::Slider(v) => Some(v),
            Kind::Cycle => None,
        }
    }
    /// The discrete step count for stepped sliders, or `None` if continuous.
    pub fn slider_steps(self) -> Option<u32> {
        match self {
            Opt::ChatDelay => Some(60),
            Opt::Fov => Some(80),
            Opt::RenderDistance => Some((crate::options_file::MAX_RENDER_DISTANCE - 2) as u32),
            Opt::SimulationDistance => Some(27),
            Opt::MaxFps => Some(25),
            Opt::CloudRange => Some(126),
            Opt::EntityDistance => Some(18),
            Opt::BiomeBlend => Some(7),
            Opt::Preset => Some(3),
            Opt::Mipmaps => Some(4),
            Opt::WeatherRadius => Some(7),
            Opt::ChunkFade => Some(40),
            Opt::MenuBlur => Some(10),
            Opt::NotificationDisplayTime => Some(95),
            Opt::PadMoveDeadzone | Opt::PadLookDeadzone => Some(50),
            Opt::PadTriggerThreshold => Some(80),
            Opt::SprintWindow => Some(10),
            _ => None,
        }
    }
    /// Step a slider forward or backward, derived from its own slider definition and step count.
    pub fn step_slider(self, s: &mut Settings, forward: bool, width: f32) {
        if !self.available() {
            return;
        }
        let Some(pos) = self.slider_position(s) else {
            return;
        };
        // A mod's range steps by its registered step.
        let steps = match self {
            Opt::ModSetting(i, j) => match mod_setting(s, i, j).map(|m| &m.value) {
                Some(crate::mod_list::NativeSettingValue::Range { min, max, step, .. }) => {
                    Some(((max - min) / step).round().max(1.0) as u32)
                }
                _ => None,
            },
            _ => self.slider_steps(),
        };
        let next_pos = if let Some(steps) = steps {
            let steps = steps as f32;
            let current = (pos * steps).round();
            let next = if forward {
                current + 1.0
            } else {
                current - 1.0
            };
            (next / steps).clamp(0.0, 1.0)
        } else {
            let delta = 1.0 / (width - 8.0).max(1.0);
            if forward {
                (pos + delta).min(1.0)
            } else {
                (pos - delta).max(0.0)
            }
        };
        self.set_slider(s, next_pos);
    }
    /// The widget message (OptionInstance.toString / CycleButton full name).
    pub fn label(self, s: &Settings) -> Text {
        if let Opt::ModSetting(i, j) = self {
            return mod_setting(s, i, j).map_or_else(|| Text::literal(""), mod_setting_label);
        }
        let c = self.caption();
        match self {
            Opt::ChatDelay => chat_delay_label(s),
            Opt::FovEffects
            | Opt::DarknessPulsing
            | Opt::DistortionEffects
            | Opt::DamageTilt
            | Opt::GlintSpeed
            | Opt::GlintStrength => {
                let value = s.file.double(self.file_key().unwrap()) as f32;
                if value == 0.0 {
                    generic(c, tr("options.off"))
                } else {
                    percent(c, value)
                }
            }
            Opt::PanoramaSpeed => percent(c, s.file.double("panoramaScrollSpeed") as f32),
            Opt::SprintWindow => {
                let value = if s.sprint_window == 0 {
                    tr("options.off")
                } else {
                    Text::tr("options.value", &[&s.sprint_window])
                };
                generic(c, value)
            }
            Opt::Fov => match s.fov as i32 {
                70 => generic(c, tr("options.fov.min")),
                110 => generic(c, tr("options.fov.max")),
                v => generic(c, Text::literal(v.to_string())),
            },
            Opt::RenderDistance => generic(c, Text::tr("options.chunks", &[&s.render_distance])),
            Opt::SimulationDistance => {
                generic(c, Text::tr("options.chunks", &[&s.simulation_distance]))
            }
            Opt::Sensitivity => {
                if s.sensitivity == 0.0 {
                    generic(c, tr("options.sensitivity.min"))
                } else if s.sensitivity == 1.0 {
                    generic(c, tr("options.sensitivity.max"))
                } else {
                    percent(c, 2.0 * s.sensitivity)
                }
            }
            Opt::WheelSensitivity => generic(
                c,
                Text::literal(format!("{:.2}", s.mouse_wheel_sensitivity)),
            ),
            Opt::Volume(i) => {
                let v = if i == 0 {
                    s.master_volume
                } else {
                    s.volumes[i]
                };
                let caption = format!("soundCategory.{}", SOUND_SOURCES[i]);
                if v == 0.0 {
                    generic(&caption, tr("options.off"))
                } else {
                    percent(&caption, v)
                }
            }
            Opt::GuiScale => {
                let value = if s.gui_scale == 0 {
                    tr("options.guiScale.auto")
                } else {
                    Text::literal(s.gui_scale.to_string())
                };
                generic(c, value)
            }
            Opt::Gamma => match (f64::from(s.gamma) * 100.0) as i32 {
                0 => generic(c, tr("options.gamma.min")),
                50 => generic(c, tr("options.gamma.default")),
                100 => generic(c, tr("options.gamma.max")),
                v => generic(c, Text::literal(v.to_string())),
            },
            Opt::MaxFps => {
                if s.max_fps >= 260 {
                    generic(c, tr("options.framerateLimit.max"))
                } else {
                    generic(c, Text::tr("options.framerate", &[&s.max_fps]))
                }
            }
            Opt::CloudRange => generic(c, Text::tr("options.chunks", &[&s.cloud_range])),
            Opt::EntityDistance => percent(c, s.entity_distance),
            Opt::BiomeBlend => generic(
                c,
                tr(&format!(
                    "options.biomeBlendRadius.{}",
                    s.biome_blend * 2 + 1
                )),
            ),
            Opt::Preset => generic(c, tr(&format!("options.graphics.{}", s.graphics_preset))),
            Opt::MaxAnisotropy => match s.file.int("maxAnisotropyBit") {
                0 => generic(c, tr("options.off")),
                bit => generic(c, Text::tr("options.multiplier", &[&(1 << bit)])),
            },
            Opt::ExclusiveMode => match s.video_modes.get(s.video_mode_index().max(0) as usize) {
                None if s.video_modes.is_empty() => tr("options.fullscreen.unavailable"),
                Some(mode) if s.video_mode_index() >= 0 => generic(
                    c,
                    Text::tr(
                        "options.fullscreen.entry",
                        &[
                            &mode.width,
                            &mode.height,
                            &mode.refresh_rate_label(),
                            &mode.bits(),
                        ],
                    ),
                ),
                _ => generic(c, tr("options.fullscreen.current")),
            },
            Opt::Mipmaps => {
                if s.mipmap_levels == 0 {
                    generic(c, tr("options.off"))
                } else {
                    generic(c, Text::literal(s.mipmap_levels.to_string()))
                }
            }
            Opt::WeatherRadius => generic(c, Text::tr("options.blocks", &[&s.weather_radius])),
            Opt::ChunkFade => {
                if s.chunk_fade <= 0.0 {
                    tr("options.chunkFade.none")
                } else {
                    Text::tr(
                        "options.chunkFade.seconds",
                        &[&format!("{:.2}", s.chunk_fade)],
                    )
                }
            }
            Opt::MenuBlur => {
                if s.menu_blur == 0 {
                    generic(c, tr("options.off"))
                } else {
                    generic(c, Text::literal(s.menu_blur.to_string()))
                }
            }
            Opt::ChatOpacity => percent(c, s.chat_opacity * 0.9 + 0.1),
            Opt::TextBackgroundOpacity => percent(c, s.text_background_opacity),
            Opt::ChatScale => {
                if s.chat_scale == 0.0 {
                    generic(c, tr("options.off"))
                } else {
                    percent(c, s.chat_scale)
                }
            }
            Opt::ChatWidth => chat_px(c, (s.chat_width * 280.0 + 40.0).floor()),
            Opt::ChatHeightFocused => chat_px(c, (s.chat_height_focused * 160.0 + 20.0).floor()),
            Opt::ChatHeightUnfocused => {
                chat_px(c, (s.chat_height_unfocused * 160.0 + 20.0).floor())
            }
            Opt::ChatLineSpacing => percent(c, s.chat_line_spacing),
            Opt::NotificationDisplayTime => generic(
                c,
                Text::tr(
                    "options.multiplier",
                    &[&format!("{:.1}", s.notification_display_time)],
                ),
            ),
            Opt::PadLookSensitivity => percent(c, 2.0 * s.controller.look_sensitivity),
            Opt::PadPointerSpeed => percent(c, s.controller.pointer_speed),
            Opt::PadScrollSpeed => percent(c, s.controller.scroll_speed / 12.0),
            Opt::PadRepeatDelay | Opt::PadRepeatInterval => generic(
                c,
                Text::tr(
                    "horizon.options.milliseconds",
                    &[&format!(
                        "{:.0}",
                        1000.0
                            * if self == Opt::PadRepeatDelay {
                                s.controller.repeat_delay
                            } else {
                                s.controller.repeat_interval
                            }
                    )],
                ),
            ),
            Opt::PadMoveDeadzone | Opt::PadLookDeadzone | Opt::PadTriggerThreshold => {
                let v = match self {
                    Opt::PadMoveDeadzone => s.controller.move_deadzone,
                    Opt::PadLookDeadzone => s.controller.look_deadzone,
                    _ => s.controller.trigger_threshold,
                };
                Text::translate(
                    "options.percent_value",
                    &[
                        tr(c),
                        Text::literal(((v * 100.0).round() as i32).to_string()),
                    ],
                )
            }
            _ => generic(c, self.value_text(s)),
        }
    }
    fn value_text(self, s: &Settings) -> Text {
        match self {
            Opt::TextBackground => tr(if s.file.bool("backgroundForChatOnly") {
                "options.accessibility.text_background.chat"
            } else {
                "options.accessibility.text_background.everywhere"
            }),
            // Options.narrator's caption: the status while the narrator is
            // there, "Not Available" otherwise.
            Opt::Narrator => tr(if horizon_narrator::is_active() {
                s.narrator().key()
            } else {
                "options.narrator.notavailable"
            }),
            Opt::ChatDelay => chat_delay_label(s),
            Opt::HideLightningFlashes => on_off(s.file.bool("hideLightningFlashes")),
            Opt::MonochromeLogo => on_off(s.file.bool("darkMojangStudiosBackground")),
            Opt::HideSplashTexts => on_off(s.file.bool("hideSplashTexts")),
            Opt::HideMatchedNames
            | Opt::OnlySecureChat
            | Opt::SaveChatDrafts
            | Opt::ModelPart(_)
            | Opt::AllowServerListing
            | Opt::RealmsNotifications
            | Opt::InGameNotification
            | Opt::DirectionalAudio => on_off(s.file.bool(self.file_key().unwrap())),
            Opt::MainHand => tr(if s.main_hand_left {
                "options.mainHand.left"
            } else {
                "options.mainHand.right"
            }),
            Opt::SharePresence => tr([
                "options.sharePresence.none",
                "options.sharePresence.limited",
                "options.sharePresence.all",
            ][s.file.int("sharePresence").clamp(0, 2) as usize]),
            Opt::MusicToast => tr([
                "options.musicToast.never",
                "options.musicToast.pauseMenu",
                "options.musicToast.pauseMenuAndToast",
            ][s.file.int("musicToast").clamp(0, 2) as usize]),
            Opt::NarratorHotkey => on_off(s.file.bool("narratorHotkey")),
            Opt::RotateWithMinecarts => on_off(s.file.bool("rotateWithMinecart")),
            Opt::HighContrastBlockOutlines => on_off(s.file.bool("highContrastBlockOutline")),
            Opt::InvertX => on_off(s.invert_x),
            Opt::InvertY => on_off(s.invert_y),
            Opt::DiscreteScroll => on_off(s.discrete_scroll),
            Opt::AllowCursorChanges => on_off(s.allow_cursor_changes),
            Opt::ViewBobbing => on_off(s.view_bobbing),
            Opt::Vsync => on_off(s.vsync),
            Opt::Fullscreen => on_off(s.fullscreen),
            Opt::ExclusiveFullscreen => on_off(s.file.bool("exclusiveFullscreen")),
            Opt::TextureFiltering => tr([
                "options.textureFiltering.none",
                "options.textureFiltering.rgss",
                "options.textureFiltering.anisotropic",
            ][s.file.int("textureFiltering").clamp(0, 2) as usize]),
            Opt::GraphicsBackend => tr(match s.horizon.api {
                crate::graphics::GraphicsApi::OpenGl => "options.graphicsApi.opengl",
                crate::graphics::GraphicsApi::Vulkan => "options.graphicsApi.vulkan",
                _ => "options.graphicsApi.default",
            }),
            Opt::SmoothLighting => on_off(s.smooth_lighting),
            Opt::Clouds => tr(match s.clouds.as_str() {
                "false" => "options.off",
                "fast" => "options.clouds.fast",
                _ => "options.clouds.fancy",
            }),
            Opt::Particles => tr([
                "options.particles.all",
                "options.particles.decreased",
                "options.particles.minimal",
            ][usize::from(s.particles.min(2))]),
            Opt::EntityShadows => on_off(s.entity_shadows),
            Opt::Vignette => on_off(s.vignette),
            Opt::Autosave => on_off(s.autosave_indicator),
            Opt::CutoutLeaves => on_off(s.cutout_leaves),
            Opt::ImprovedTransparency => on_off(s.improved_transparency),
            Opt::AttackIndicator => tr([
                "options.off",
                "options.attack.crosshair",
                "options.attack.hotbar",
            ][usize::from(s.attack_indicator.min(2))]),
            Opt::InactivityFps => tr(if s.inactivity_fps_limit == "minimized" {
                "options.inactivityFpsLimit.minimized"
            } else {
                "options.inactivityFpsLimit.afk"
            }),
            Opt::ChunkBuilder => tr([
                "options.prioritizeChunkUpdates.none",
                "options.prioritizeChunkUpdates.byPlayer",
                "options.prioritizeChunkUpdates.nearby",
            ][usize::from(s.prioritize_chunk_updates.min(2))]),
            Opt::ToggleCrouch => tr(if s.toggle_crouch {
                "options.key.toggle"
            } else {
                "options.key.hold"
            }),
            Opt::ToggleSprint => tr(if s.toggle_sprint {
                "options.key.toggle"
            } else {
                "options.key.hold"
            }),
            Opt::SkinLayers3d => tr(if s.extras.three_d_skin_layers {
                "options.on"
            } else {
                "options.off"
            }),
            Opt::ToggleAttack => tr(if s.toggle_attack {
                "options.key.toggle"
            } else {
                "options.key.hold"
            }),
            Opt::ToggleUse => tr(if s.toggle_use {
                "options.key.toggle"
            } else {
                "options.key.hold"
            }),
            Opt::AutoJump => on_off(s.auto_jump),
            Opt::QuitShortcuts => on_off(s.quit_shortcuts),
            Opt::CtrlClickEmulatesRightClick => on_off(s.ctrl_click_emulates_right_click),
            Opt::OperatorItemsTab => on_off(s.operator_items_tab),
            Opt::ChatVisibility => tr([
                "options.chat.visibility.full",
                "options.chat.visibility.system",
                "options.chat.visibility.hidden",
            ][usize::from(s.chat_visibility.min(2))]),
            Opt::ChatColors => on_off(s.chat_colors),
            Opt::ChatLinks => on_off(s.chat_links),
            Opt::ChatLinksPrompt => on_off(s.chat_links_prompt),
            Opt::AutoSuggestions => on_off(s.auto_suggestions),
            Opt::ReducedDebugInfo => on_off(s.reduced_debug_info),
            Opt::HighContrast => on_off(s.high_contrast),
            Opt::ShowSubtitles => on_off(s.show_subtitles),
            Opt::SoundDevice => {
                if s.sound_device.is_empty() {
                    tr("options.audioDevice.default")
                } else {
                    Text::literal(&s.sound_device)
                }
            }
            Opt::MusicFrequency => tr([
                "options.music_frequency.default",
                "options.music_frequency.frequent",
                "options.music_frequency.constant",
            ][s.file.int("musicFrequency").clamp(0, 2) as usize]),
            Opt::ForceUnicode => on_off(s.force_unicode),
            Opt::JapaneseGlyphs => on_off(s.japanese_glyphs),
            Opt::PadEnabled => on_off(s.controller.enabled),
            Opt::PadVibration => on_off(s.controller.vibration),
            Opt::PadPlayInBackground => on_off(s.controller.play_in_background),
            Opt::PadAdaptiveTriggers => on_off(s.controller.adaptive_triggers),
            Opt::PadGameplayGuide => tr(match s.controller.guide_detail {
                gamepad::GuideDetail::Full => "horizon.options.gameplay_guide.full",
                gamepad::GuideDetail::Basic => "horizon.options.gameplay_guide.basic",
                gamepad::GuideDetail::Off => "horizon.options.gameplay_guide.off",
            }),
            Opt::PadInvertX => on_off(s.controller.invert_x),
            Opt::PadInvertY => on_off(s.controller.invert_y),
            Opt::PadCraftOnServers => on_off(s.controller.craft_on_servers),
            Opt::PadToggleSneak => tr(if s.controller.toggle_sneak {
                "options.key.toggle"
            } else {
                "options.key.hold"
            }),
            Opt::PadLightbar => tr(match s.controller.lightbar {
                gamepad::Lightbar::Health => "horizon.options.lightbar.health",
                gamepad::Lightbar::Dimension => "horizon.options.lightbar.dimension",
                gamepad::Lightbar::Off => "options.off",
            }),
            Opt::PadGlyphs => tr(match s.controller.glyphs {
                GlyphStyle::Auto => "horizon.options.glyphs.auto",
                GlyphStyle::Xbox => "horizon.options.glyphs.xbox",
                GlyphStyle::PlayStation => "horizon.options.glyphs.playstation",
                GlyphStyle::Nintendo => "horizon.options.glyphs.nintendo",
            }),
            Opt::PadNintendoButtons => tr(if s.controller.nintendo_letters {
                "horizon.options.nintendo_buttons.letter"
            } else {
                "horizon.options.nintendo_buttons.position"
            }),
            Opt::PadKeyboard => tr(match s.controller.keyboard {
                gamepad::TextKeyboard::Auto => "horizon.options.keyboard.auto",
                gamepad::TextKeyboard::Horizon => "horizon.options.keyboard.horizon",
                gamepad::TextKeyboard::Steam => "horizon.options.keyboard.steam",
            }),
            Opt::PadRadialMode | Opt::PadRadialStick => self.radial_text(s),
            Opt::Page(page, row) => pages::slider_text(page, row),
            _ => Text::empty(),
        }
    }
    /// Set a slider from its 0..1 position (OptionInstance value sets).
    pub fn set_slider(self, s: &mut Settings, v: f32) {
        if let Opt::ModSetting(i, j) = self {
            let setting = s
                .mods
                .installed
                .get_mut(usize::from(i))
                .and_then(|m| m.native.as_mut())
                .and_then(|page| page.settings.get_mut(usize::from(j)));
            if let Some(setting) = setting {
                setting.value.slide(v);
            }
            return;
        }
        if !self.available() {
            return;
        }
        let v = v.clamp(0.0, 1.0);
        let step = |lo: f32, hi: f32| (lo + v * (hi - lo)).round();
        match self {
            Opt::ChatDelay => values::set(s, v),
            Opt::FovEffects | Opt::DarknessPulsing => s.file.set(
                self.file_key().unwrap(),
                crate::options_file::V::Double(f64::from(v).powi(2)),
            ),
            Opt::DistortionEffects
            | Opt::DamageTilt
            | Opt::PanoramaSpeed
            | Opt::GlintSpeed
            | Opt::GlintStrength => s.file.set(
                self.file_key().unwrap(),
                crate::options_file::V::Double(f64::from(v)),
            ),
            Opt::Fov => s.fov = step(30.0, 110.0),
            Opt::RenderDistance => {
                s.render_distance =
                    step(2.0, crate::options_file::MAX_RENDER_DISTANCE as f32) as i32
            }
            Opt::SimulationDistance => s.simulation_distance = step(5.0, 32.0) as i32,
            Opt::Sensitivity => s.sensitivity = v,
            Opt::WheelSensitivity => s.mouse_wheel_sensitivity = wheel::from_slider(v),
            Opt::Volume(0) => s.master_volume = v,
            Opt::Volume(i) => s.volumes[i] = v,
            Opt::Gamma => s.gamma = v,
            Opt::MaxFps => s.max_fps = (step(1.0, 26.0) as u32) * 10,
            Opt::CloudRange => s.cloud_range = step(2.0, 128.0) as i32,
            Opt::EntityDistance => s.entity_distance = step(2.0, 20.0) / 4.0,
            Opt::BiomeBlend => s.biome_blend = step(0.0, 7.0) as i32,
            Opt::Preset => s.graphics_preset = PRESETS[preset_from_slider(v)].into(),
            Opt::Mipmaps => s.mipmap_levels = step(0.0, 4.0) as i32,
            Opt::MaxAnisotropy => s.file.set(
                "maxAnisotropyBit",
                crate::options_file::V::Int(step(1.0, 3.0) as i32),
            ),
            Opt::ExclusiveMode => {
                // Window.setPreferredFullscreenVideoMode: -1 is the desktop's own mode.
                let Some(last) = s.video_modes.len().checked_sub(1) else {
                    return;
                };
                let index = step(-1.0, last as f32) as i32;
                s.file.fullscreen_resolution = usize::try_from(index)
                    .ok()
                    .map(|i| s.video_modes[i].write());
            }
            Opt::WeatherRadius => s.weather_radius = step(3.0, 10.0) as i32,
            Opt::ChunkFade => s.chunk_fade = step(0.0, 40.0) / 20.0,
            Opt::MenuBlur => s.menu_blur = step(0.0, 10.0) as i32,
            Opt::NotificationDisplayTime => s.notification_display_time = step(5.0, 100.0) / 10.0,
            Opt::ChatOpacity => s.chat_opacity = v,
            Opt::TextBackgroundOpacity => s.text_background_opacity = v,
            Opt::ChatScale => s.chat_scale = v,
            Opt::ChatWidth => s.chat_width = v,
            Opt::ChatHeightFocused => s.chat_height_focused = v,
            Opt::ChatHeightUnfocused => s.chat_height_unfocused = v,
            Opt::ChatLineSpacing => s.chat_line_spacing = v,
            Opt::PadLookSensitivity => s.controller.look_sensitivity = v,
            Opt::PadPointerSpeed => s.controller.pointer_speed = step(25.0, 300.0) / 100.0,
            Opt::PadScrollSpeed => s.controller.scroll_speed = step(1.0, 36.0),
            Opt::PadRepeatDelay => s.controller.repeat_delay = step(100.0, 1000.0) / 1000.0,
            Opt::PadRepeatInterval => s.controller.repeat_interval = step(30.0, 300.0) / 1000.0,
            Opt::PadMoveDeadzone => s.controller.move_deadzone = step(0.0, 50.0) / 100.0,
            Opt::PadLookDeadzone => s.controller.look_deadzone = step(0.0, 50.0) / 100.0,
            Opt::PadTriggerThreshold => s.controller.trigger_threshold = step(10.0, 90.0) / 100.0,
            Opt::SprintWindow => s.sprint_window = step(0.0, 10.0) as i32,
            Opt::Page(page, row) => pages::slide(page, row, v),
            _ => {}
        }
        if self.affects_preset() {
            s.graphics_preset = "custom".into();
        }
    }
    fn affects_preset(self) -> bool {
        matches!(
            self,
            Opt::RenderDistance
                | Opt::SimulationDistance
                | Opt::SmoothLighting
                | Opt::Clouds
                | Opt::CloudRange
                | Opt::Particles
                | Opt::EntityDistance
                | Opt::EntityShadows
                | Opt::BiomeBlend
                | Opt::Mipmaps
                | Opt::WeatherRadius
                | Opt::CutoutLeaves
                | Opt::ImprovedTransparency
                | Opt::MenuBlur
                | Opt::ChunkBuilder
                | Opt::TextureFiltering
                | Opt::MaxAnisotropy
        )
    }
    /// Advance a cycle button (CycleButton.cycleValue).
    pub fn cycle(self, s: &mut Settings, back: bool, framebuffer: [i32; 2]) {
        if !self.available() {
            return;
        }
        let next = |n: u8, count: u8| {
            if back {
                (n + count - 1) % count
            } else {
                (n + 1) % count
            }
        };
        match self {
            Opt::GuiScale => {
                let max = crate::window::calculate_scale(framebuffer, 0, s.force_unicode) as u32;
                let value = s.gui_scale.min(max);
                s.gui_scale = if back {
                    if value == 0 { max } else { value - 1 }
                } else if value == max {
                    0
                } else {
                    value + 1
                };
            }
            Opt::TextBackground
            | Opt::HideLightningFlashes
            | Opt::HideSplashTexts
            | Opt::HighContrastBlockOutlines
            | Opt::MonochromeLogo
            | Opt::HideMatchedNames
            | Opt::OnlySecureChat
            | Opt::SaveChatDrafts
            | Opt::ModelPart(_)
            | Opt::AllowServerListing
            | Opt::RealmsNotifications
            | Opt::InGameNotification
            | Opt::DirectionalAudio
            | Opt::RotateWithMinecarts
            | Opt::NarratorHotkey => {
                let key = self.file_key().unwrap();
                s.file
                    .set(key, crate::options_file::V::Bool(!s.file.bool(key)));
            }
            // OptionInstance.Enum over NarratorStatus.values().
            Opt::Narrator => {
                let n = next(s.narrator().id() as u8, 4);
                s.set_narrator(horizon_narrator::Status::by_id(i32::from(n)));
            }
            Opt::InvertX => s.invert_x = !s.invert_x,
            Opt::InvertY => s.invert_y = !s.invert_y,
            Opt::DiscreteScroll => s.discrete_scroll = !s.discrete_scroll,
            Opt::AllowCursorChanges => s.allow_cursor_changes = !s.allow_cursor_changes,
            Opt::ViewBobbing => s.view_bobbing = !s.view_bobbing,
            Opt::Vsync => s.vsync = !s.vsync,
            Opt::Fullscreen => s.fullscreen = !s.fullscreen,
            Opt::ExclusiveFullscreen => {
                let on = s.file.bool("exclusiveFullscreen");
                s.file
                    .set("exclusiveFullscreen", crate::options_file::V::Bool(!on));
            }
            Opt::TextureFiltering => {
                let n = next(s.file.int("textureFiltering").clamp(0, 2) as u8, 3);
                s.file.set(
                    "textureFiltering",
                    crate::options_file::V::Int(i32::from(n)),
                );
            }
            Opt::GraphicsBackend => {
                // PreferredGraphicsApi: Default, OpenGL, Vulkan; a platform lacking one skips it.
                use crate::graphics::GraphicsApi::{Auto, OpenGl, Vulkan};
                let offered: Vec<_> = [Auto, OpenGl, Vulkan]
                    .into_iter()
                    .filter(|a| crate::graphics::GraphicsApi::choices().contains(a))
                    .collect();
                let at = offered
                    .iter()
                    .position(|a| *a == s.horizon.api)
                    .unwrap_or(0);
                let count = offered.len();
                let api = offered[if back {
                    (at + count - 1) % count
                } else {
                    (at + 1) % count
                }];
                s.horizon.api = api;
                s.horizon.watchdog_api = None;
                s.horizon.watchdog_driver = None;
                s.file.set_name(
                    "preferredGraphicsBackend",
                    match api {
                        OpenGl => "opengl",
                        Vulkan => "vulkan",
                        _ => "default",
                    },
                );
            }
            Opt::SmoothLighting => s.smooth_lighting = !s.smooth_lighting,
            Opt::Clouds => {
                let order = ["false", "fast", "true"];
                let i = order.iter().position(|c| *c == s.clouds).unwrap_or(2) as u8;
                s.clouds = order[usize::from(next(i, 3))].into();
            }
            Opt::Particles => s.particles = next(s.particles, 3),
            Opt::EntityShadows => s.entity_shadows = !s.entity_shadows,
            Opt::Vignette => s.vignette = !s.vignette,
            Opt::Autosave => s.autosave_indicator = !s.autosave_indicator,
            Opt::CutoutLeaves => s.cutout_leaves = !s.cutout_leaves,
            Opt::ImprovedTransparency => s.improved_transparency = !s.improved_transparency,
            Opt::AttackIndicator => s.attack_indicator = next(s.attack_indicator, 3),
            Opt::InactivityFps => {
                s.inactivity_fps_limit = if s.inactivity_fps_limit == "afk" {
                    "minimized".into()
                } else {
                    "afk".into()
                }
            }
            Opt::ChunkBuilder => s.prioritize_chunk_updates = next(s.prioritize_chunk_updates, 3),
            Opt::ToggleCrouch => s.toggle_crouch = !s.toggle_crouch,
            Opt::ToggleSprint => s.toggle_sprint = !s.toggle_sprint,
            Opt::SkinLayers3d => s.extras.three_d_skin_layers = !s.extras.three_d_skin_layers,
            Opt::ToggleAttack => s.toggle_attack = !s.toggle_attack,
            Opt::ToggleUse => s.toggle_use = !s.toggle_use,
            Opt::AutoJump => s.auto_jump = !s.auto_jump,
            Opt::QuitShortcuts => s.quit_shortcuts = !s.quit_shortcuts,
            Opt::CtrlClickEmulatesRightClick => {
                s.ctrl_click_emulates_right_click = !s.ctrl_click_emulates_right_click
            }
            Opt::OperatorItemsTab => s.operator_items_tab = !s.operator_items_tab,
            Opt::ChatVisibility => s.chat_visibility = next(s.chat_visibility, 3),
            Opt::ChatColors => s.chat_colors = !s.chat_colors,
            Opt::ChatLinks => s.chat_links = !s.chat_links,
            Opt::ChatLinksPrompt => s.chat_links_prompt = !s.chat_links_prompt,
            Opt::AutoSuggestions => s.auto_suggestions = !s.auto_suggestions,
            Opt::ReducedDebugInfo => s.reduced_debug_info = !s.reduced_debug_info,
            Opt::HighContrast => s.high_contrast = !s.high_contrast,
            Opt::ShowSubtitles => s.show_subtitles = !s.show_subtitles,
            Opt::SoundDevice => {
                let devices = crate::audio::output_devices();
                let mut all = vec![String::new()];
                for d in devices {
                    if !all.contains(&d) {
                        all.push(d);
                    }
                }
                let current_idx = all.iter().position(|d| d == &s.sound_device).unwrap_or(0);
                let count = all.len();
                let next_idx = if back {
                    (current_idx + count - 1) % count
                } else {
                    (current_idx + 1) % count
                };
                s.sound_device = all[next_idx].clone();
            }
            Opt::MusicFrequency => {
                let n = next(s.file.int("musicFrequency").clamp(0, 2) as u8, 3);
                s.file
                    .set("musicFrequency", crate::options_file::V::Int(i32::from(n)));
            }
            Opt::MainHand => s.main_hand_left = !s.main_hand_left,
            Opt::SharePresence => {
                let n = next(s.file.int("sharePresence").clamp(0, 2) as u8, 3);
                s.file
                    .set("sharePresence", crate::options_file::V::Int(i32::from(n)));
            }
            Opt::MusicToast => {
                let n = next(s.file.int("musicToast").clamp(0, 2) as u8, 3);
                s.file
                    .set("musicToast", crate::options_file::V::Int(i32::from(n)));
            }
            Opt::ForceUnicode => s.force_unicode = !s.force_unicode,
            Opt::JapaneseGlyphs => s.japanese_glyphs = !s.japanese_glyphs,
            Opt::PadEnabled => s.controller.enabled = !s.controller.enabled,
            Opt::PadVibration => s.controller.vibration = !s.controller.vibration,
            Opt::PadPlayInBackground => {
                s.controller.play_in_background = !s.controller.play_in_background
            }
            Opt::PadAdaptiveTriggers => {
                s.controller.adaptive_triggers = !s.controller.adaptive_triggers
            }
            Opt::PadInvertX => s.controller.invert_x = !s.controller.invert_x,
            Opt::PadInvertY => s.controller.invert_y = !s.controller.invert_y,
            Opt::PadCraftOnServers => {
                s.controller.craft_on_servers = !s.controller.craft_on_servers
            }
            Opt::PadToggleSneak => s.controller.toggle_sneak = !s.controller.toggle_sneak,
            Opt::PadNintendoButtons => {
                s.controller.nintendo_letters = !s.controller.nintendo_letters
            }
            Opt::PadLightbar => {
                let all = gamepad::Lightbar::ALL;
                let i = all
                    .iter()
                    .position(|l| *l == s.controller.lightbar)
                    .unwrap_or(0) as u8;
                s.controller.lightbar = all[usize::from(next(i, 3))];
            }
            Opt::PadGlyphs => {
                let i = GlyphStyle::ALL
                    .iter()
                    .position(|g| *g == s.controller.glyphs)
                    .unwrap_or(0) as u8;
                s.controller.glyphs = GlyphStyle::ALL[usize::from(next(i, 4))];
            }
            Opt::PadGameplayGuide => {
                let all = gamepad::GuideDetail::ALL;
                let i = all
                    .iter()
                    .position(|d| *d == s.controller.guide_detail)
                    .unwrap_or(0) as u8;
                s.controller.guide_detail = all[usize::from(next(i, 3))];
            }
            Opt::PadKeyboard => {
                let all = gamepad::TextKeyboard::ALL;
                let i = all
                    .iter()
                    .position(|k| *k == s.controller.keyboard)
                    .unwrap_or(0) as u8;
                s.controller.keyboard = all[usize::from(next(i, 3))];
            }
            Opt::PadRadialMode | Opt::PadRadialStick => self.radial_step(s, back),
            _ => {}
        }
        if self.affects_preset() {
            s.graphics_preset = "custom".into();
        }
    }
}
const PRESETS: [&str; 4] = ["fast", "fancy", "fabulous", "custom"];
fn preset_index(p: &str) -> usize {
    PRESETS.iter().position(|x| *x == p).unwrap_or(1)
}
/// A slider moved to `v` (OptionInstance.set): the graphics preset applies
/// its options when its value changes (`Options.applyGraphicsPreset`).
pub fn set_slider(opt: Opt, s: &mut Settings, v: f32) {
    let preset = preset_index(&s.graphics_preset);
    opt.set_slider(s, v);
    if opt == Opt::Preset && preset_index(&s.graphics_preset) != preset {
        apply_preset(s);
    }
}
/// A slider pressed or dragged to `v`: apply it live, and tell the game when
/// a category volume changed (`Options.createSoundSliderOptionInstance`'s
/// callback, which `OptionInstance.set` runs only for a new value).
fn drag_slider(opt: Opt, s: &mut Settings, v: f32, events: &mut Vec<Event>) {
    let volume = |s: &Settings| match opt {
        Opt::Volume(0) => Some(s.master_volume),
        Opt::Volume(i) => Some(s.volumes[i]),
        _ => None,
    };
    let before = volume(s);
    set_slider(opt, s, v);
    events.push(Event::SliderDrag);
    if let Opt::ModSetting(i, j) = opt {
        events.push(Event::NativeSetting(usize::from(i), usize::from(j)));
    }
    if let (Opt::Volume(source), Some(after)) = (opt, volume(s))
        && before != Some(after)
    {
        events.push(Event::Volume(source, after));
    }
}
/// GraphicsPreset.apply: the options Fast, Fancy and Fabulous set.
pub fn apply_preset(s: &mut Settings) {
    let fabulous = s.graphics_preset == "fabulous";
    match s.graphics_preset.as_str() {
        "fast" => {
            s.biome_blend = 1;
            s.render_distance = 8;
            s.prioritize_chunk_updates = 0;
            s.simulation_distance = 6;
            s.smooth_lighting = false;
            s.clouds = "fast".into();
            s.particles = 1;
            s.mipmap_levels = 2;
            s.entity_shadows = false;
            s.entity_distance = 0.75;
            s.menu_blur = 2;
            s.cloud_range = 32;
            s.cutout_leaves = false;
            s.improved_transparency = false;
            s.weather_radius = 5;
            set_filtering(s, 1, 0);
        }
        "fancy" | "fabulous" => {
            s.biome_blend = 2;
            s.render_distance = if fabulous { 32 } else { 16 };
            s.prioritize_chunk_updates = 1;
            s.simulation_distance = 12;
            s.smooth_lighting = true;
            s.clouds = "true".into();
            s.particles = 0;
            s.mipmap_levels = 4;
            s.entity_shadows = true;
            s.entity_distance = if fabulous { 1.25 } else { 1.0 };
            s.menu_blur = 5;
            s.cloud_range = if fabulous { 128 } else { 64 };
            s.cutout_leaves = true;
            s.improved_transparency = fabulous;
            s.weather_radius = 10;
            // Fabulous takes anisotropic filtering (RGSS on a device with known issues, which
            // wgpu does not report).
            set_filtering(
                s,
                if fabulous { 2 } else { 1 },
                if fabulous { 2 } else { 1 },
            );
        }
        _ => {}
    }
}
/// The preset's `maxAnisotropyBit` and `textureFiltering` (0 none, 1 RGSS, 2 anisotropic).
fn set_filtering(s: &mut Settings, anisotropy_bit: i32, filtering: i32) {
    use crate::options_file::V;
    s.file.set("maxAnisotropyBit", V::Int(anisotropy_bit));
    s.file.set("textureFiltering", V::Int(filtering));
}

/// `Options.preferredGraphicsBackend`'s tooltip: the restart note once the choice differs from
/// the startup one, the description, and Vulkan's warning.
fn graphics_api_tooltip(s: &Settings) -> Text {
    let line = |key: &str| lang::get_or(key, key);
    let mut lines = Vec::new();
    if s.horizon.api != s.startup_api {
        lines.push(line("options.needsRestart"));
        lines.push(String::new());
    }
    lines.push(line("options.graphicsApi.tooltip"));
    if s.horizon.api == crate::graphics::GraphicsApi::Vulkan {
        lines.push(String::new());
        lines.push(line("options.graphicsApi.tooltip.vulkan"));
    }
    Text::literal(lines.join("\n"))
}

/// `selected_key` while a key a native mod registered waits (`selected_mod_key`).
const MOD_KEY: &str = "horizon.modKey";

/// The id of mod `i`'s registered key `k` in `mods.installed`, with the mod's id.
fn mod_mapping(s: &Settings, i: usize, k: usize) -> Option<(String, String)> {
    let m = s.mods.installed.get(i)?;
    let mapping = m.native.as_ref()?.mappings.get(k)?;
    Some((m.id.clone(), mapping.id.clone()))
}

fn key_conflicts(
    s: &Settings,
    owner: &str,
    id: &str,
    key: &str,
    modifiers: u32,
    context: crate::mod_list::KeyConflictContext,
    at_default: bool,
) -> Vec<String> {
    use crate::mod_list::{KeyConflictContext, bindings_conflict};
    let binding = (key, modifiers, context, at_default);
    let mut conflicts = Vec::new();
    for &(name, _, _) in keybinds::all_mappings() {
        if !(owner.is_empty() && name == id)
            && bindings_conflict(
                binding,
                (
                    s.keys.get(name),
                    0,
                    KeyConflictContext::Universal,
                    s.keys.is_default(name),
                ),
            )
        {
            conflicts.push(lang::get(name));
        }
    }
    for installed in &s.mods.installed {
        for mapping in installed.native.iter().flat_map(|page| &page.mappings) {
            if installed.id == owner && mapping.id == id {
                continue;
            }
            let (other_key, other_modifiers) = s.mods.binding(
                &installed.id,
                &mapping.id,
                (&mapping.key, mapping.modifiers),
            );
            if bindings_conflict(
                binding,
                (
                    &other_key,
                    other_modifiers,
                    mapping.context,
                    other_key == mapping.key && other_modifiers == mapping.modifiers,
                ),
            ) {
                conflicts.push(lang::get(&mapping.label));
            }
        }
    }
    conflicts
}

/// A native mod's title or key category on Controls: bold and underlined, centred on the row at `y`.
fn section_title(text: Text, y: f32, w: f32) -> Widget {
    Widget {
        x: 0.0,
        y: y + 7.0,
        w,
        h: 9.0,
        kind: WKind::Text {
            text: text.with_style(&Style {
                bold: Some(true),
                underlined: Some(true),
                ..Style::default()
            }),
            color: u32::MAX,
            centered: true,
        },
        list: false,
        row: 0,
    }
}

/// A registered key's binding as Controls shows it: its modifiers' key names, then its key's.
fn binding_text(key: &str, modifiers: u32) -> Text {
    let mut text = Text::empty();
    for (bit, name) in [
        (crate::mod_list::SHIFT, "key.keyboard.left.shift"),
        (crate::mod_list::CONTROL, "key.keyboard.left.control"),
        (crate::mod_list::ALT, "key.keyboard.left.alt"),
    ] {
        if modifiers & bit != 0 {
            text = text.append(keybinds::display_name(name)).append(" + ");
        }
    }
    text.append(keybinds::display_name(key))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Main,
    Video,
    Sound,
    Controls,
    KeyBinds,
    Mouse,
    Chat,
    Accessibility,
    Language,
    Font,
    Packs,
    Mods,
    /// Browse mods: the registry's content mods, searched (`mod_browser`).
    ModBrowser,
    /// One registry project's page: `OptionsUi::mod_project`.
    ModProject,
    /// A native mod's settings page: its index in `mods.installed`.
    NativeMod(usize),
    /// A page a crate registered (`horizon_hooks::pages`): its index in `OptionsUi::pages`.
    Added(usize),
    /// Horizon's shader packs, either kind (`shader_pack_list`).
    ShaderPacks,
    /// The picked pack's options: the shader screen (`shader_ui`) at the pack's main screen.
    ShaderOptions,
    /// The selected resource packs' Polytone options (`pack_options_ui`), from the pack screen.
    PackOptions,
    /// Horizon's graphics settings (`graphics_ui`).
    Graphics,
    GraphicsAdvanced,
    GraphicsHdr,
    /// HDR calibration, full screen (`calibration`).
    HdrCalibration,
    /// Horizon's controller settings and button binds.
    Controller,
    PadBinds,
    /// The radial menu's pages (`radial_editor`), on the settings shell, and a page's name.
    ControllerRadial,
    ControllerRadialName,
    /// The list of every action for one of its slots.
    ControllerRadialPick,
    Extras,
    /// PvP & HUD (`pvp`): the extras a PvP player sets, on their own page.
    Pvp,
    /// CreditsAndAttributionScreen.
    Credits,
    /// Horizon Settings: every page Horizon adds, as a mod's settings
    /// screen hangs off vanilla's Options.
    Horizon,
    /// Replay Studio: saved recordings browser and 3D video editor launcher.
    Studio,
    /// AI Agent: agent control on or off, whether an agent is connected, the connection details.
    Agent,
    /// SkinCustomizationScreen: the skin layers and Main Hand.
    Skin,
    /// OnlineOptionsScreen: server listing, Realms and friends notifications.
    Online,
    /// Splitscreen: the profile another player joins as (`split_profiles`).
    SplitProfiles,
    /// Splitscreen: the monitor map and draft window arrangement.
    SplitSetup,
    /// The built-in music tracks, by situation: play one, turn any off
    /// (vanilla's `minecraft:audio/background_music` pools).
    Music,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Act {
    None,
    Push(Page),
    Done,
    Cycle(Opt),
    Extra(crate::extras::Item),
    Key(&'static str),
    ResetKey(&'static str),
    ResetKeys,
    /// PvP & HUD's PvP preset (`pvp::apply_preset`).
    PvpPreset,
    /// PvP & HUD's Edit HUD button.
    HudEditor,
    /// A native mod's key (mod, key) waits for a key; a button of its page.
    /// A key a native mod registered (mod, mapping) waits for a key, or is reset.
    ModKey(usize, usize),
    ResetModKey(usize, usize),
    /// Setting `.1` of native mod `.0`'s page clicked: a toggle, choice or colour steps on.
    NativeSetting(usize, usize),
    PageRow(usize, usize),
    /// Browse mods: the page of listing `.0` of `mod_browser`.
    ModProject(usize),
    /// The open project page's Install or Update.
    InstallMod,
    /// The open project page's page on the website.
    OpenModPage,
    /// Accessibility's "Accessibility Help" link
    /// (`AccessibilityOptionsScreen.addFooter`'s openLinkButton).
    AccessibilityHelp,
    /// Credits & Attribution's Credits button (WinScreen without the poem).
    Credits,
    CopyAgentDetails,
    /// AI Agent: start or stop the repro recording.
    ToggleRecording,
    /// AI Agent: write the repro bundle now.
    ExportRepro,
    /// The Music page: play this track of this event now.
    PlayMusicTrack(String, String),
    /// The Music page: turn this track off, or back on.
    MusicTrackOn(String, bool),
}
#[derive(Clone, Debug, PartialEq)]
enum WKind {
    Button {
        label: Text,
        active: bool,
        act: Act,
    },
    Slider {
        label: Text,
        value: f32,
        opt: Opt,
    },
    Text {
        text: Text,
        color: u32,
        centered: bool,
    },
    LanguageRow {
        label: Text,
        selected: bool,
        code: String,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Widget {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    kind: WKind,
    /// Scrolls with the list (clipped to the list area).
    list: bool,
    /// The list row it sits in (`ContainerObjectSelectionList`'s entry).
    row: usize,
}

/// The keyboard-focused UI target on an options page.
#[derive(Clone, Debug, PartialEq)]
enum FocusId {
    Search,
    Slider(Opt),
    Cycle(Opt),
    Act(Act),
    Lang(String),
    Index(usize),
}

fn focus_id(i: usize, wd: &Widget) -> FocusId {
    match &wd.kind {
        WKind::Slider { opt, .. } => FocusId::Slider(*opt),
        WKind::Button {
            act: Act::Cycle(opt),
            ..
        } => FocusId::Cycle(*opt),
        WKind::Button { act, .. } if *act != Act::None => FocusId::Act(act.clone()),
        WKind::LanguageRow { code, .. } => FocusId::Lang(code.clone()),
        _ => FocusId::Index(i),
    }
}

fn matches_focus(id: &FocusId, i: usize, wd: &Widget) -> bool {
    match (id, &wd.kind) {
        (FocusId::Slider(a), WKind::Slider { opt, .. }) => a == opt,
        (
            FocusId::Cycle(a),
            WKind::Button {
                act: Act::Cycle(opt),
                ..
            },
        ) => a == opt,
        (FocusId::Act(a), WKind::Button { act, .. }) => a == act,
        (FocusId::Lang(a), WKind::LanguageRow { code, .. }) => a == code,
        (FocusId::Index(idx), _) => *idx == i,
        _ => false,
    }
}
/// What the game must do after an interaction.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// Dropped resource packs accepted on the confirmation screen.
    ImportPacks(Vec<std::path::PathBuf>),
    /// The Pack Options page closed with changes: the player's choices of its entries, the
    /// ones that differ from their defaults (the others went back to theirs).
    PackOptions {
        entries: Vec<String>,
        chosen: std::collections::BTreeMap<String, horizon_polytone::pack::ConfigValue>,
    },
    /// An accepted button press or the release of a slider.
    ButtonClick,
    /// Horizon's own: download the Modrinth hit at this index of the pack
    /// screen's browser. The game owns the packs folder, not this screen.
    DownloadPack(usize),
    /// Close the options screens (back to the pause menu or title).
    Close,
    /// Settings changed: apply and save them.
    Changed,
    /// The reduced-debug-info option rebuilt the debug entry list.
    DebugEntriesChanged,
    /// A slider is being dragged: apply settings live but defer saving.
    SliderDrag,
    /// A category volume slider (`SoundSource` index) moved to a new value,
    /// after its `SliderDrag` (the sound slider's `onValueUpdate`).
    Volume(usize, f32),
    /// The language changed (reload translations).
    Language(String),
    /// The pack selection changed (reload resources).
    Packs(Vec<String>),
    /// Create World's Data Packs screen closed with a new selection.
    DataPacks(Vec<String>),
    OpenPackFolder,
    /// Data Packs' Open Pack Folder: create the folder if needed and open it.
    OpenDataPackFolder(std::path::PathBuf),
    OpenModFolder,
    /// A link on the Mods screen (Homepage, Issues, a license): confirm and open it.
    OpenLink(String),
    /// A click on the Mods screen's mod id or version: copy it.
    CopyText(String),
    /// Browse mods opened: read the registry's index into `mod_browser`.
    BrowseMods,
    /// Install (or update) the registry's content mod with this id.
    InstallMod(String),
    /// Open a registry project's page on the website.
    OpenModPage(String),
    /// Accessibility's Help link: confirm and open
    /// `CommonLinks.ACCESSIBILITY_HELP` over the options screen.
    AccessibilityHelp,
    /// The Shader Packs page opened: list `shaderpacks/` into `shader_packs`.
    ListShaderPacks,
    /// A shader pack was picked or turned off: save and apply it.
    ShaderPack,
    OpenShaderPackFolder,
    /// The Shader Options page opened: read the picked pack's options into `shader_options`.
    OpenShaderOptions,
    /// The Shader Options page closed with changes: save them and load the pack again.
    ShaderOptions,
    /// The options of `shader_packs[.0]`, a Horizon pack, changed: save them (the pack reloads
    /// with them).
    NativePackOptions(usize),
    /// Native mod `.0`'s page opened: the game reads its settings' kept values again.
    NativeSettings(usize),
    /// Setting `.1` of native mod `.0`'s page changed (`settings.mods`): the game keeps it and
    /// tells the mod (`setting_changed`).
    NativeSetting(usize, usize),
    /// Roll the credits (`CreditsAndAttributionScreen.openCreditsScreen`).
    Credits,
    /// PvP & HUD's Edit HUD: open the HUD editor over the game.
    HudEditor,
    /// Copy how an agent connects (the AI Agent page).
    CopyAgentDetails,
    /// The AI Agent page's recording button: start or stop.
    ToggleRecording,
    /// The AI Agent page's export button.
    ExportRepro,
    /// The narrator option changed (after its `Changed`): the option's
    /// `onValueUpdate`, `GameNarrator.updateNarratorStatus`.
    Narrator,
    /// The Music page opened: fill `music_groups` from the sound registry
    /// (the game owns it).
    ListMusicTracks,
    /// A Music page row: play this track of this event now.
    PlayMusicTrack(String, String),
    /// Open the recordings folder in the system file browser.
    OpenStudioFolder,
    /// Start playback from the selected Studio recording.
    OpenStudioReplay(std::path::PathBuf),
}
/// A row of the Music page: the track, and the pool it is played from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MusicTrack {
    /// The music event the track belongs to (`minecraft:music.game`).
    pub event: String,
    /// The sound id (`minecraft:music/game/calm1`).
    pub id: String,
}
/// One group of rows on the Music page: a situation, or a dimension's biomes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MusicGroup {
    /// The lang key of the heading the page shows (`horizon.pages.music.overworld`).
    pub label: String,
    /// The rows, in `sounds.json` order.
    pub tracks: Vec<MusicTrack>,
}
pub struct OptionsUi {
    frame_draw: Option<draw::FrameDraw>,
    pub stack: Vec<Page>,
    scroll: f32,
    pub(crate) dragging: Option<Opt>,
    /// Shift is held during the click (it resets a shader pack option to its default).
    pub shift: bool,
    /// The window's modifier keys (GLFW bits) when a key or button is bound: a mod's key keeps them.
    pub modifiers: u32,
    /// The pressed slider's own position (`AbstractSliderButton.value`) and
    /// when it last moved: the handle follows the pointer while the option
    /// keeps its stepped value, then snaps to it (`resetValue`).
    slider_value: Option<(Opt, f32, Instant)>,
    /// The pointer's last y while a scrollbar is held.
    pub(crate) scrollbar_drag_y: Option<f32>,
    /// Key Binds: the mapping waiting for a key (`MOD_KEY` for a registered mod key).
    pub selected_key: Option<&'static str>,
    /// The registered key waiting: (mod, mapping) in `mods.installed`.
    selected_mod_key: Option<(usize, usize)>,
    selected_page_key: Option<(usize, usize)>,
    pending_mod_key: Option<winit::keyboard::KeyCode>,
    /// Bindings: the action waiting for a pad button.
    pub selected_pad_action: Option<&'static str>,
    /// The glyphs menus show (`Glyphs::of`: the setting, else the pad's family, and a Nintendo
    /// pad's letters when it goes by them). The game sets it each frame.
    pub pad_glyphs: crate::pad_glyph::Glyphs,
    /// The pad's focus drives the page: its hidden pointer sits mid-widget, so a tooltip goes
    /// under the widget instead of beside the pointer.
    pub pad_driving: bool,
    pub languages: Vec<LanguageInfo>,
    language_choice: Option<String>,
    language_search: ui::EditBox,
    pub(crate) settings_search: ui::EditBox,
    /// Browse mods: the registry's projects, filled by the game.
    pub mod_browser: crate::mod_browser::ModBrowser,
    /// The project whose page is open (`Page::ModProject`).
    pub mod_project: String,
    /// The line about mods that wait for a restart to switch, for Browse mods' message.
    pub mods_note: Option<String>,
    /// The Mods screen (`Page::Mods`), made when the page opens.
    pub mods_screen: Option<crate::mods_screen::ModsScreen>,
    /// Horizon's `packBrowser` extra: the pack screen's Add button.
    pub pack_browser: bool,
    /// Packs: the packs found and the selection the screen opens with.
    pub pack_infos: Vec<crate::pack_screen::PackInfo>,
    pub pack_selection: Vec<String>,
    /// The open pack screen, made at its first frame.
    pub pack_screen: Option<crate::pack_screen::PackScreen>,
    /// The Packs page is Create World's Data Packs screen on this folder.
    data_pack_dir: Option<std::path::PathBuf>,
    /// The game's resources, for the built-in data pack's icon.
    data_pack_resources: std::path::PathBuf,
    /// Additional repository entries retained while the folder watcher refreshes.
    additional_data_pack_infos: Vec<crate::pack_screen::PackInfo>,
    /// The folder's entries when its packs were listed, and when they were
    /// last compared with it.
    data_pack_stamp: (Vec<(String, Option<std::time::SystemTime>)>, Instant),
    /// A reload failure, dismissed before returning to the current page.
    pub resource_error: Option<String>,
    /// When the pack screen's search box took the focus (its cursor blink).
    pack_focus: Instant,
    /// The selection carried over from a pack screen made again over a changed pack list
    /// (`update_pack_infos`) differs from the settings'.
    pack_reselected: bool,
    pub in_world: bool,
    /// AccessibilityOptionsScreen: the loaded world's experimental feature.
    pub minecart_improvements: bool,
    /// Scroll the selected language into the middle on the next layout.
    center_language: bool,
    /// The output of the renderer the options are drawn for, as of the last draw. It lives
    /// here, not on the graphics screen, which opening resets: `shell_mut` hands it over.
    output: crate::gfx::EffectiveOutput,
    graphics: shell::Shell<crate::graphics_ui::GraphicsScreen>,
    /// Controller Settings, on the settings shell.
    controller: shell::Shell<crate::controller_ui::ControllerScreen>,
    /// Horizon Extras, on the settings shell.
    extras: shell::Shell<crate::extras_ui::ExtrasScreen>,
    /// The radial menu's pages, on the settings shell.
    radial: shell::Shell<crate::radial_editor::RadialScreen>,
    /// The list of actions for one of the radial menu's slots.
    radial_pick: shell::Shell<crate::radial_editor::PickScreen>,
    /// Shaders: the shader packs and the picked one's options, on the settings shell.
    shader: shell::Shell<crate::shader_ui::ShaderScreen>,
    /// Pack Options, on the settings shell, and the packs' Polytone config entries by pack id
    /// (the game fills them beside `pack_infos`).
    pack_options: shell::Shell<crate::pack_options_ui::PackOptionsScreen>,
    pub pack_configs: std::collections::BTreeMap<String, horizon_polytone::pack::Configs>,
    pub split_profiles: crate::split_profiles::SplitProfiles,
    /// HDR calibration while its page is open.
    pub calibration: Option<crate::hdr_calibration::Calibration>,
    /// The window as calibration found it before making it fullscreen (the game keeps it).
    pub calibration_window: Option<crate::hdr_calibration::WindowState>,
    pub split_setup: crate::split_ui::SplitUi,
    pub studio: crate::studio_ui::StudioUi,
    /// The AI Agent page's status lines (set by the game while it shows).
    pub agent_status: Vec<String>,
    /// A repro recording is running: the AI Agent page's button says stop.
    pub agent_recording: bool,
    /// The Music page's groups (set by the game when it opens the page).
    pub music_groups: Vec<MusicGroup>,
    /// Keyboard focus: the interactive target currently focused.
    focus: Option<FocusId>,
    /// The focus came from keyboard or pad navigation, so the mouse wheel steps it
    /// from anywhere (`wheel`). A click focuses its widget too, for the highlight,
    /// but vanilla's wheel ignores that: only the hovered widget then reacts.
    focus_from_keyboard: bool,
    /// Whether a focused slider is in value-editing mode (`AbstractSliderButton.canChangeValue`).
    pub(crate) can_change_value: bool,
    /// The current GUI size (w, h).
    pub size: (f32, f32),
    /// The current window framebuffer dimensions [w, h].
    pub framebuffer: [i32; 2],
    /// Splitscreen: another player's options. Language and resource packs are the whole
    /// process's (every player's text and textures), so they're P1's to change.
    pub split_guest: bool,
    /// The list's rows as last laid out: each row's top and bottom (GUI
    /// pixels), for the narration's row under the mouse.
    row_bands: Vec<[f32; 2]>,
    /// The PvP & HUD items that take a whole row (`wide_extras`), measured
    /// when the page is drawn.
    wide_extras: Vec<crate::extras::Item>,
    /// The page layout's revision: every change to an input the layout reads
    /// (the page, the scroll, the search text, a setting's shown value) bumps
    /// it, so the layout is made again once and not on every draw or click.
    rev: u64,
    /// The page's layout at `layout_key`: shared by the drawing, the hit
    /// tests and the scroll, so all three read the same rectangles.
    layout_cache: Option<((f32, f32, u64, u32), std::sync::Arc<OptionsLayout>)>,
    /// How many layouts were made (the caching test).
    #[cfg(test)]
    layouts_built: u32,
}

impl Default for OptionsUi {
    fn default() -> Self {
        let mut search = ui::EditBox::new(0.0, 0.0, 200.0, 15.0);
        search.max_length = 32;
        let mut settings_search = ui::EditBox::new(0.0, 0.0, 200.0, 15.0);
        settings_search.focused = false;
        Self {
            frame_draw: None,
            stack: vec![Page::Main],
            scroll: 0.0,
            dragging: None,
            shift: false,
            modifiers: 0,
            slider_value: None,
            scrollbar_drag_y: None,
            selected_key: None,
            selected_mod_key: None,
            selected_page_key: None,
            pending_mod_key: None,
            selected_pad_action: None,
            pad_glyphs: crate::pad_glyph::Glyphs::default(),
            pad_driving: false,
            languages: Vec::new(),
            language_choice: None,
            language_search: search,
            settings_search,
            mod_browser: Default::default(),
            mod_project: String::new(),
            mods_note: None,
            mods_screen: None,
            pack_browser: false,
            pack_infos: Vec::new(),
            pack_selection: Vec::new(),
            pack_screen: None,
            data_pack_dir: None,
            data_pack_resources: Default::default(),
            additional_data_pack_infos: Vec::new(),
            data_pack_stamp: (Vec::new(), Instant::now()),
            resource_error: None,
            pack_focus: Instant::now(),
            pack_reselected: false,
            in_world: false,
            minecart_improvements: false,
            center_language: false,
            output: crate::gfx::EffectiveOutput::NONE,
            graphics: Default::default(),
            controller: Default::default(),
            extras: Default::default(),
            radial: Default::default(),
            radial_pick: Default::default(),
            shader: Default::default(),
            pack_options: Default::default(),
            pack_configs: Default::default(),
            split_profiles: Default::default(),
            calibration: None,
            calibration_window: None,
            split_setup: Default::default(),
            studio: Default::default(),
            agent_status: Vec::new(),
            agent_recording: false,
            music_groups: Vec::new(),
            focus: None,
            focus_from_keyboard: false,
            can_change_value: false,
            size: (427.0, 240.0),
            framebuffer: [1280, 720],
            split_guest: false,
            row_bands: Vec::new(),
            wide_extras: Vec::new(),
            rev: 0,
            layout_cache: None,
            #[cfg(test)]
            layouts_built: 0,
        }
    }
}
fn option_widget(s: &Settings, opt: Opt, x: f32, y: f32, w: f32) -> Widget {
    // OptionsSubScreen.addContents: the narrator button is active only while
    // the narrator is (`narratorButton.active = narrator.isActive()`).
    if !opt.available() || opt == Opt::Narrator && !horizon_narrator::is_active() {
        return inactive(button(x, y, w, opt.label(s), Act::Cycle(opt)));
    }
    // VideoSettingsScreen.tick: Max Anisotropy only works with Anisotropic filtering.
    if opt == Opt::MaxAnisotropy && s.file.int("textureFiltering") != 2 {
        return inactive(button(x, y, w, opt.label(s), Act::Cycle(opt)));
    }
    match opt.kind(s) {
        Kind::Slider(v) => Widget {
            x,
            y,
            w,
            h: 20.0,
            kind: WKind::Slider {
                label: opt.label(s),
                value: v,
                opt,
            },
            list: false,
            row: 0,
        },
        Kind::Cycle => button(x, y, w, opt.label(s), Act::Cycle(opt)),
    }
}
/// The least GUI size a shell screen lays its keys out for, before it has been drawn at its own.
const MIN_KEY_GUI: (f32, f32) = (480.0, 240.0);
/// The PvP & HUD page's rows: its items under their headers.
fn pvp_rows() -> Vec<Result<crate::extras::Item, &'static str>> {
    crate::pvp::rows()
        .into_iter()
        .map(|r| r.map(crate::extras::Item::Pvp))
        .collect()
}
/// The items of PvP & HUD's rows whose label, with any of their first values, is wider than a
/// half-width button holds (`width` measures text with the screen's font): those take the
/// whole row, so no label slides past its edges, in any language.
fn wide_extras(s: &Settings, width: impl Fn(&str) -> f32) -> Vec<crate::extras::Item> {
    // A button's label keeps 2 px off each edge (`AbstractButton.extractDefaultLabel`'s margin),
    // past which it slides back and forth.
    let room = 150.0 - 4.0;
    pvp_rows()
        .into_iter()
        .filter_map(Result::ok)
        .filter(|&item| {
            let mut e = s.extras.clone();
            (0..4).any(|_| {
                let label = Text::option_name_value(
                    Text::literal(item.caption_text()),
                    Text::literal(item.value(&e)),
                )
                .plain();
                item.cycle(&mut e, false);
                width(&label) > room
            })
        })
        .collect()
}
/// Rows of an OptionsList: (big?, options) or a header.
enum Row {
    Big(Opt),
    Pair(Opt, Option<Opt>),
    Header(&'static str),
    /// A heading the page makes at run time, by lang key (the Music page's groups).
    Heading(String),
    /// A line of grey text (a path).
    Note(String),
    Buttons(Widget, Option<Widget>),
}
/// A widget's tooltip (`setTooltip`), unwrapped: drawn while hovered, and
/// its narration's hint.
fn tooltip_text(s: &Settings, wd: &Widget) -> Option<Text> {
    let WKind::Button { act, .. } = &wd.kind else {
        let WKind::Slider { opt, .. } = &wd.kind else {
            return None;
        };
        let key = opt.tooltip_key(s);
        let Opt::Page(page, row) = opt else {
            return lang::has(&key).then(|| tr(&key));
        };
        return pages::tooltip(*page, *row);
    };
    let mut tip = None;
    if let Act::Extra(item) = act {
        tip = Some(Text::literal(item.description_text()));
    }
    if let Act::Push(page) = act
        && let Some(text) = pages::horizon_page_tooltip(*page)
    {
        tip = Some(tr(text));
    }
    let key = match act {
        Act::Cycle(opt) => Some(opt.tooltip_key(s)),
        Act::Push(Page::Language) => Some("options.language.tooltip".into()),
        Act::Push(Page::Accessibility) => Some("options.accessibility.tooltip".into()),
        Act::PvpPreset => Some("horizon.pvp.preset.tooltip".into()),
        Act::HudEditor => Some("horizon.pvp.editor.tooltip".into()),
        _ => None,
    };
    if let Some(key) = key.filter(|key| lang::has(key)) {
        tip = Some(tr(&key));
    }
    if let Act::Cycle(Opt::GraphicsBackend) = act {
        tip = Some(graphics_api_tooltip(s));
    }
    match act {
        Act::Cycle(opt) if !opt.available() => Some(tr("horizon.options.unavailable")),
        Act::PageRow(page, row) => pages::tooltip(*page, *row),
        Act::ModKey(i, k) => {
            let installed = s.mods.installed.get(*i)?;
            let mapping = installed.native.as_ref()?.mappings.get(*k)?;
            let (key, modifiers) = s.mods.binding(
                &installed.id,
                &mapping.id,
                (&mapping.key, mapping.modifiers),
            );
            let conflicts = key_conflicts(
                s,
                &installed.id,
                &mapping.id,
                &key,
                modifiers,
                mapping.context,
                key == mapping.key && modifiers == mapping.modifiers,
            );
            if conflicts.is_empty() {
                return tip;
            }
            Some(Text::translate(
                "controls.keybinds.duplicateKeybinds",
                &[Text::literal(conflicts.join(", "))],
            ))
        }
        Act::Key(name) => {
            let conflicts = key_conflicts(
                s,
                "",
                name,
                s.keys.get(name),
                0,
                crate::mod_list::KeyConflictContext::Universal,
                s.keys.is_default(name),
            );
            if conflicts.is_empty() {
                return tip;
            }
            let names = conflicts.join(", ");
            Some(Text::translate(
                "controls.keybinds.duplicateKeybinds",
                &[Text::literal(names)],
            ))
        }
        _ => tip,
    }
}
/// The value part of an option's message (`CycleButton.createLabelForValue`):
/// its `options.generic_value` message without the caption.
fn option_value(opt: Opt, s: &Settings) -> String {
    let full = opt.label(s).plain();
    let marker = "\u{1}";
    let template = generic(opt.caption(), Text::literal(marker)).plain();
    match template.split_once(marker) {
        Some((before, after))
            if full.len() >= before.len() + after.len()
                && full.starts_with(before)
                && full.ends_with(after) =>
        {
            full[before.len()..full.len() - after.len()].to_owned()
        }
        _ => full,
    }
}
/// What widget `wd` narrates, over what `ui::button` or `ui::slider`
/// recorded for it (`n`): a cycle button the value a click moves to (worked
/// out only while it may be narrated: under the mouse or focused), a key
/// bind's buttons `narrator.controls.*`, its tooltip as a hint; a list row's
/// widget belongs to the list keyed `list`.
fn narrate_widget(
    n: &mut crate::widgets::Narratable,
    s: &Settings,
    wd: &Widget,
    hover: bool,
    list: u64,
    framebuffer: [i32; 2],
) {
    use crate::widgets::What;
    if let WKind::Button { label, active, act } = &wd.kind {
        let t = |key: &str, args: &[Text]| Text::translate(key, args).plain();
        let bind = |name: Text, bound: bool| {
            if bound {
                let button = Text::translate("gui.narrate.button", std::slice::from_ref(label));
                t("narrator.controls.bound", &[name, button])
            } else {
                t("narrator.controls.unbound", &[name])
            }
        };
        let active = *active;
        match act {
            Act::Cycle(opt) if active => {
                let next = if hover || crate::widgets::narration_focused(n.rect) {
                    let mut next = s.clone();
                    opt.cycle(&mut next, false, framebuffer);
                    option_value(*opt, &next)
                } else {
                    String::new()
                };
                n.what = What::Cycle {
                    message: label.plain(),
                    next,
                    active,
                };
            }
            Act::ModKey(i, k) | Act::ResetModKey(i, k) => {
                if let Some(installed) = s.mods.installed.get(*i)
                    && let Some(mapping) = installed
                        .native
                        .as_ref()
                        .and_then(|page| page.mappings.get(*k))
                {
                    let title = if matches!(act, Act::ResetModKey(..)) {
                        t("narrator.controls.reset", &[tr(&mapping.label)])
                    } else {
                        let (key, _) = s.mods.binding(
                            &installed.id,
                            &mapping.id,
                            (&mapping.key, mapping.modifiers),
                        );
                        bind(tr(&mapping.label), key != "key.keyboard.unknown")
                    };
                    n.what = What::Named { title, active };
                }
            }
            Act::Key(name) => {
                let bound = s.keys.get(name) != "key.keyboard.unknown";
                n.what = What::Named {
                    title: bind(tr(name), bound),
                    active,
                };
            }
            Act::ResetKey(name) => {
                n.what = What::Named {
                    title: t("narrator.controls.reset", &[tr(name)]),
                    active,
                };
            }
            _ => {}
        }
    }
    n.hint = tooltip_text(s, wd).map(|tip| tip.plain());
    if wd.list {
        // Stable while the list scrolls: its row and column.
        n.key = crate::widgets::key_of(&(list, wd.row, wd.x.round() as i32));
        n.parent = Some((list, wd.row));
    }
}
/// A list's rows as they are laid out: the row of each widget pushed and
/// each row's band, for the narration (`ContainerObjectSelectionList`'s
/// entries and the one under the mouse).
#[derive(Default)]
struct ListRows {
    start: usize,
    top: f32,
    bands: Vec<[f32; 2]>,
}
impl ListRows {
    /// A row starts at `y` with the next widget pushed.
    fn begin(&mut self, out: &[Widget], y: f32) {
        self.start = out.len();
        self.top = y;
    }
    /// The row ends at `y`: the widgets pushed since `begin` are in it.
    fn end(&mut self, out: &mut [Widget], y: f32) {
        let row = self.bands.len();
        out[self.start..].iter_mut().for_each(|w| w.row = row);
        self.bands.push([self.top, y]);
    }
}
fn pairs(opts: &[Opt]) -> Vec<Row> {
    opts.chunks(2)
        .map(|c| Row::Pair(c[0], c.get(1).copied()))
        .collect()
}
impl OptionsUi {
    /// No widget keeps the highlight a click gave it; a focus the keyboard
    /// moved stays.
    pub fn clear_click_focus(&mut self) {
        if !self.focus_from_keyboard {
            self.focus = None;
        }
        if let Some(shell) = self.shell_mut() {
            shell.blur_search();
        }
    }
    /// A switch of `horizon-extras.txt` as the Extras page draws it: "Caption: On".
    fn extra_button(&self, s: &Settings, item: crate::extras::Item) -> Widget {
        let label = Text::option_name_value(
            Text::literal(item.caption_text()),
            Text::literal(item.value(&s.extras)),
        );
        button(0.0, 0.0, 150.0, label, Act::Extra(item))
    }
    fn settings_search_active(&self) -> bool {
        self.settings_search.focused
            && matches!(
                self.page(),
                Page::Video | Page::Controls | Page::KeyBinds | Page::ModBrowser
            )
    }
    pub fn page(&self) -> Page {
        *self.stack.last().unwrap_or(&Page::Main)
    }
    /// Whether the screen's focus is a search box that takes the keys (an
    /// `EditBox` that `canConsumeInput`): the language list's, the settings
    /// search, or the pack screen's.
    pub fn typing(&self) -> bool {
        self.resource_error.is_none()
            && match self.page() {
                Page::Language => true,
                Page::Packs => self
                    .pack_screen
                    .as_ref()
                    .is_some_and(|p| p.search_focused()),
                Page::Mods => self
                    .mods_screen
                    .as_ref()
                    .is_some_and(|p| p.search_focused()),
                Page::ControllerRadialName => self.radial_name_typing(),
                _ => match self.shell() {
                    Some(shell) => shell.typing(),
                    None => self.settings_search_active(),
                },
            }
    }
    /// Jump straight to a page (control socket / tests).
    pub fn show(&mut self, page: Page) {
        if page != Page::Main {
            self.push(page);
        }
    }
    pub fn open(&mut self, in_world: bool) {
        self.open_at(in_world, Page::Main);
    }
    /// Create World's Data Packs (`openDataPackSelectionScreen`): the Packs
    /// page on the new world's pack folder `dir`, `selected` chosen;
    /// `resources` holds the built-in pack's icon.
    pub fn open_data_packs(
        &mut self,
        dir: std::path::PathBuf,
        selected: Vec<String>,
        resources: std::path::PathBuf,
        additional: Vec<crate::pack_screen::PackInfo>,
    ) {
        self.open_at(false, Page::Packs);
        self.pack_infos = crate::datapack::screen_packs(&dir, &resources);
        self.pack_infos.extend(additional.iter().cloned());
        self.additional_data_pack_infos = additional;
        self.data_pack_resources = resources;
        self.pack_selection = selected;
        self.data_pack_stamp = (crate::datapack::folder_stamp(&dir), Instant::now());
        self.data_pack_dir = Some(dir);
    }
    /// `PackSelectionScreen.tick`'s watcher: once a second, the Data Packs
    /// folder's packs are listed again if it changed, the selection kept
    /// (the search text and scroll start over).
    fn watch_data_packs(&mut self, atlas: &HudAtlas, size: (f32, f32)) {
        let Some(dir) = &self.data_pack_dir else {
            return;
        };
        if self.data_pack_stamp.1.elapsed().as_secs() < 1 {
            return;
        }
        let stamp = crate::datapack::folder_stamp(dir);
        let changed = stamp != self.data_pack_stamp.0;
        self.data_pack_stamp = (stamp, Instant::now());
        if !changed {
            return;
        }
        self.pack_infos = crate::datapack::screen_packs(dir, &self.data_pack_resources);
        self.pack_infos
            .extend(self.additional_data_pack_infos.iter().cloned());
        if let Some(p) = self.pack_screen.take() {
            self.pack_selection = p.commit();
            self.packs(atlas, size).changed = p.changed;
        }
    }
    /// Steps a slider option forward or backward, derived from its own slider definition.
    pub fn step_slider_option(
        &mut self,
        s: &mut Settings,
        opt: Opt,
        forward: bool,
        width: f32,
    ) -> Vec<Event> {
        let volume = |s: &Settings| match opt {
            Opt::Volume(0) => Some(s.master_volume),
            Opt::Volume(i) => Some(s.volumes[i]),
            _ => None,
        };
        let before_vol = volume(s);
        opt.step_slider(s, forward, width);
        self.slider_value = None;
        let mut events = vec![Event::Changed, Event::SliderDrag];
        if let Opt::ModSetting(i, j) = opt {
            events.push(Event::NativeSetting(usize::from(i), usize::from(j)));
        }
        if let (Opt::Volume(source), Some(after)) = (opt, volume(s))
            && before_vol != Some(after)
        {
            events.push(Event::Volume(source, after));
        }
        events
    }
    /// Open a page whose Done/Escape returns directly to the calling screen.
    pub fn open_at(&mut self, in_world: bool, page: Page) {
        self.discard_draw();
        self.stack.clear();
        self.data_pack_dir = None;
        self.focus = None;
        self.focus_from_keyboard = false;
        self.can_change_value = false;
        self.push(page);
        self.in_world = in_world;
        self.selected_key = None;
        self.pending_mod_key = None;
    }
    fn push(&mut self, page: Page) {
        self.relayout();
        self.stack.push(page);
        self.scroll = 0.0;
        self.scrollbar_drag_y = None;
        self.focus = None;
        self.focus_from_keyboard = false;
        self.can_change_value = false;
        if matches!(
            page,
            Page::Video | Page::Controls | Page::KeyBinds | Page::ModBrowser
        ) {
            self.settings_search.set_value("");
            self.settings_search.focused = false;
        }
        if page == Page::Language {
            self.language_choice = None;
            self.language_search.set_value("");
            self.center_language = true;
        }
        if page == Page::Packs {
            self.pack_screen = None;
        }
        if page == Page::Mods {
            self.mods_screen = None;
        }
        if let Some(shell) = self.shell_mut() {
            shell.open();
        }
        if let Some(at) = Self::shader_page(page) {
            self.shader.show(at);
        }
        if let Some(at) = Self::graphics_page(page) {
            self.graphics.show(at);
        }
        if let Some(at) = Self::controller_page(page) {
            self.controller.show(at);
        }
        if page == Page::SplitSetup {
            self.split_setup.reset();
        }
        if page == Page::SplitProfiles {
            self.split_profiles.reset();
        }
        if page == Page::Studio {
            self.studio.refresh();
        }
    }
    /// A controller's or the keyboard's focus landed on `rect`: a row of a shell screen's list
    /// that is partly hidden scrolls all into view. How far it moved down, when it moved.
    pub fn reveal(&mut self, s: &Settings, size: (f32, f32), rect: [f32; 4]) -> Option<f32> {
        self.shell_mut()?.reveal(s, size, rect)
    }
    /// Whether a video settings screen is open, Horizon's or vanilla's: the fullscreen mode
    /// waits for it to close, and Ctrl with the wheel steps the GUI scale.
    pub fn video_open(&self) -> bool {
        self.page() == Page::Video || Self::graphics_page(self.page()).is_some()
    }
    /// Where the graphics screen opens for `page`, if it is one of its pages.
    fn graphics_page(page: Page) -> Option<crate::graphics_ui::Page> {
        match page {
            Page::Graphics => Some(crate::graphics_ui::Page::General),
            Page::GraphicsAdvanced => Some(crate::graphics_ui::Page::Advanced),
            Page::GraphicsHdr => Some(crate::graphics_ui::Page::Hdr),
            _ => None,
        }
    }
    fn title(page: Page) -> &'static str {
        match page {
            Page::Main => "options.title",
            Page::Video => "options.videoTitle",
            Page::Sound => "options.sounds.title",
            Page::Skin => "options.skinCustomisation.title",
            Page::Online => "options.online.title",
            Page::Controls => "controls.title",
            Page::KeyBinds => "controls.keybinds.title",
            Page::Mouse => "options.mouse_settings.title",
            Page::Chat => "options.chat.title",
            Page::Accessibility => "options.accessibility.title",
            Page::Language => "options.language.title",
            Page::Font => "options.font.title",
            Page::Packs => "resourcePack.title",
            Page::Mods => "horizon.pages.mods",
            Page::ModBrowser => "horizon.mods.browse.title",
            Page::ModProject => "horizon.mods.browse.project",
            Page::NativeMod(_) => "Mod Settings",
            Page::Added(i) => pages::title(i),
            Page::ShaderPacks => "horizon.pages.shader_packs",
            Page::ShaderOptions => "horizon.pages.shader_options",
            Page::PackOptions => "horizon.pack.options.title",
            Page::Graphics | Page::GraphicsAdvanced | Page::GraphicsHdr | Page::HdrCalibration => {
                "options.videoTitle"
            }
            Page::Controller | Page::PadBinds => "horizon.pages.controller",
            Page::ControllerRadial => "horizon.pages.controller_radial",
            Page::ControllerRadialName => "horizon.radial.name_title",
            Page::ControllerRadialPick => "horizon.radial.pick_title",
            Page::Extras => "horizon.pages.extras",
            Page::Pvp => "horizon.pages.pvp",
            Page::Credits => "credits_and_attribution.screen.title",
            Page::Horizon => "horizon.pages.settings",
            Page::Studio => "horizon.pages.studio",
            Page::Agent => "horizon.pages.agent",
            Page::SplitProfiles => "horizon.pages.split_profiles",
            Page::SplitSetup => "horizon.pages.split_setup",
            Page::Music => "horizon.pages.music",
        }
    }
    fn scrollbar_x(&self, width: f32) -> f32 {
        (width / 2.0).floor()
            + match self.page() {
                Page::KeyBinds => 178.0,
                Page::Language => 118.0,
                _ => 163.0,
            }
    }
    fn filter_option_rows(&self, rows: Vec<Row>) -> Vec<Row> {
        let query = self.settings_search.value.to_lowercase();
        if query.is_empty() {
            return rows;
        }
        let mut filtered = Vec::new();
        let mut pending_header = None;
        for row in rows {
            let matches = |opt: Opt| lang::get(opt.caption()).to_lowercase().contains(&query);
            let visible = match row {
                Row::Header(key) => {
                    pending_header = Some(key);
                    continue;
                }
                Row::Big(opt) if matches(opt) => vec![Row::Big(opt)],
                Row::Pair(a, b) => pairs(
                    &[Some(a), b]
                        .into_iter()
                        .flatten()
                        .filter(|opt| matches(*opt))
                        .collect::<Vec<_>>(),
                ),
                // Keep navigation available regardless of the query.
                Row::Buttons(a, b) => {
                    filtered.push(Row::Buttons(a, b));
                    continue;
                }
                _ => Vec::new(),
            };
            if !visible.is_empty() {
                // Emit a section heading only when its first matching option is found.
                if let Some(key) = pending_header.take() {
                    filtered.push(Row::Header(key));
                }
                filtered.extend(visible);
            }
        }
        filtered
    }

    /// Browse mods: what the registry offers this game, by the search box.
    fn mod_browser_rows(&self) -> Vec<Row> {
        use crate::mod_browser::Status;
        let b = &self.mod_browser;
        match &b.status {
            Status::Idle | Status::Loading => {
                return vec![Row::Note(lang::get("horizon.mods.browse.loading"))];
            }
            Status::Failed(why) => {
                return vec![Row::Note(lang::tr("horizon.mods.browse.failed", &[why]))];
            }
            Status::Ready => {}
        }
        let found = b.found(&self.settings_search.value);
        if found.is_empty() {
            return vec![Row::Note(lang::get("horizon.mods.browse.none"))];
        }
        let mut rows = Vec::new();
        for i in found {
            let l = &b.listings[i];
            let mut label = Text::literal(format!("{} {}", l.name, l.version));
            if l.has_update() {
                label = label.append(tr("horizon.mods.browse.has_update"));
            } else if l.installed.is_some() {
                label = label.append(tr("horizon.mods.browse.is_installed"));
            }
            rows.push(Row::Buttons(
                button(0.0, 0.0, 310.0, label, Act::ModProject(i)),
                None,
            ));
            rows.extend(wrap(&l.summary, 64).into_iter().take(1).map(Row::Note));
        }
        rows
    }
    /// A registry project's page: what it is, Install or Update, its description and
    /// changelog. An installed mod is on already: the install turned it on.
    fn mod_project_rows(&self) -> Vec<Row> {
        let b = &self.mod_browser;
        let Some(l) = b.listing(&self.mod_project) else {
            return vec![Row::Note(lang::get("horizon.mods.browse.none"))];
        };
        let mut rows = vec![Row::Note(format!("{} {}", l.name, l.version))];
        let by = [
            lang::tr("horizon.mods.browse.by", &[&l.authors.join(", ")]),
            l.license.clone(),
            l.published.clone(),
        ];
        let by: Vec<&str> = by
            .iter()
            .map(String::as_str)
            .filter(|t| !t.is_empty())
            .collect();
        rows.push(Row::Note(by.join(" · ")));
        let labels: Vec<&str> = l
            .categories
            .iter()
            .chain(&l.tags)
            .map(String::as_str)
            .collect();
        if !labels.is_empty() {
            rows.push(Row::Note(labels.join(", ")));
        }
        let installing = b.installing.as_deref() == Some(l.id.as_str());
        let install = match (&l.installed, installing) {
            (_, true) => inactive(button(
                0.0,
                0.0,
                150.0,
                tr("horizon.mods.browse.installing"),
                Act::None,
            )),
            (None, false) => button(
                0.0,
                0.0,
                150.0,
                tr("horizon.mods.browse.install"),
                Act::InstallMod,
            ),
            (Some(_), false) if l.has_update() => button(
                0.0,
                0.0,
                150.0,
                tr("horizon.mods.browse.update"),
                Act::InstallMod,
            ),
            (Some(_), false) => inactive(button(
                0.0,
                0.0,
                150.0,
                tr("horizon.mods.browse.installed"),
                Act::None,
            )),
        };
        let page = button(
            0.0,
            0.0,
            150.0,
            tr("horizon.mods.browse.open_page"),
            Act::OpenModPage,
        );
        rows.push(Row::Buttons(install, Some(page)));
        if let Some((message, _)) = &b.message {
            rows.extend(wrap(message, 64).into_iter().map(Row::Note));
        }
        let description = crate::mod_browser::plain_lines(if l.description.is_empty() {
            &l.summary
        } else {
            &l.description
        });
        rows.extend(
            description
                .iter()
                .flat_map(|line| wrap(line, 64))
                .map(Row::Note),
        );
        let changelog = crate::mod_browser::plain_lines(&l.changelog);
        if !changelog.is_empty() {
            rows.push(Row::Heading("horizon.mods.browse.changelog".into()));
            rows.extend(
                changelog
                    .iter()
                    .flat_map(|line| wrap(line, 64))
                    .map(Row::Note),
            );
        }
        rows
    }
    /// A native mod's page: its name, its keys as Key Binds rows (the
    /// player's choice, else the default), then its buttons.
    fn native_rows(&self, s: &Settings, i: usize) -> Vec<Row> {
        let Some(m) = s.mods.installed.get(i) else {
            return Vec::new();
        };
        let mut rows = vec![Row::Note(m.title.clone())];
        let Some(page) = &m.native else {
            return rows;
        };
        for (j, setting) in page.settings.iter().enumerate() {
            let widget = match (&setting.value, u16::try_from(i), u16::try_from(j)) {
                (crate::mod_list::NativeSettingValue::Range { .. }, Ok(i), Ok(j)) => Widget {
                    x: 0.0,
                    y: 0.0,
                    w: 310.0,
                    h: 20.0,
                    kind: WKind::Slider {
                        label: mod_setting_label(setting),
                        value: setting.value.fraction(),
                        opt: Opt::ModSetting(i, j),
                    },
                    list: false,
                    row: 0,
                },
                _ => button(
                    0.0,
                    0.0,
                    310.0,
                    mod_setting_label(setting),
                    Act::NativeSetting(i, j),
                ),
            };
            rows.push(Row::Buttons(widget, None));
        }
        rows
    }

    fn list_rows(&self, s: &Settings) -> Vec<Row> {
        match self.page() {
            Page::SplitSetup => Vec::new(),
            Page::Pvp => {
                // Each section's switches two to a row, as vanilla's
                // OptionsList.addSmall lays out an options screen; one whose label would run
                // past half a row in this language (`wide_extras`) gets the whole row.
                let extra = |item: crate::extras::Item, width: f32| {
                    let value = Text::option_name_value(
                        Text::literal(item.caption_text()),
                        Text::literal(item.value(&s.extras)),
                    )
                    .plain();
                    let label = if item.key_name().is_some() && self.selected_key == item.key_name()
                    {
                        Text::literal(format!("> {value} <")).with_color(text::YELLOW)
                    } else {
                        Text::literal(value)
                    };
                    let widget = button(0.0, 0.0, width, label, Act::Extra(item));
                    // A setting of a switch that is off does nothing: inactive,
                    // as vanilla greys out an option that can't apply.
                    if item
                        .parent()
                        .is_some_and(|parent| parent.switch(&s.extras) == Some(false))
                    {
                        inactive(widget)
                    } else {
                        widget
                    }
                };
                // PvP & HUD opens with its preset (Horizon Settings has the way there).
                let preset = button(0.0, 0.0, 150.0, tr("horizon.pvp.preset"), Act::PvpPreset);
                // The editor moves what the game shows: only from a world.
                let edit = button(0.0, 0.0, 150.0, tr("horizon.pvp.editor"), Act::HudEditor);
                let edit = if self.in_world { edit } else { inactive(edit) };
                let mut rows = vec![Row::Buttons(preset, Some(edit))];
                let mut section = Vec::new();
                let flush = |rows: &mut Vec<Row>, section: &mut Vec<crate::extras::Item>| {
                    let mut half = None;
                    for item in section.drain(..) {
                        if self.wide_extras.contains(&item) {
                            rows.extend(half.take().map(|h| Row::Buttons(extra(h, 150.0), None)));
                            rows.push(Row::Buttons(extra(item, 310.0), None));
                        } else if let Some(h) = half.take() {
                            rows.push(Row::Buttons(extra(h, 150.0), Some(extra(item, 150.0))));
                        } else {
                            half = Some(item);
                        }
                    }
                    rows.extend(half.map(|h| Row::Buttons(extra(h, 150.0), None)));
                };
                for row in pvp_rows() {
                    match row {
                        Err(header) => {
                            flush(&mut rows, &mut section);
                            rows.push(Row::Header(header));
                        }
                        Ok(item) => section.push(item),
                    }
                }
                flush(&mut rows, &mut section);
                rows
            }
            Page::Horizon => self.horizon_rows(s),
            Page::Agent => {
                let mut rows = vec![Row::Buttons(
                    self.extra_button(s, crate::extras::Item::AgentControl),
                    Some(button(
                        0.0,
                        0.0,
                        150.0,
                        tr("horizon.agent.copy_details"),
                        Act::CopyAgentDetails,
                    )),
                )];
                rows.push(Row::Buttons(
                    button(
                        0.0,
                        0.0,
                        150.0,
                        tr(if self.agent_recording {
                            "horizon.agent.record_stop"
                        } else {
                            "horizon.agent.record_session"
                        }),
                        Act::ToggleRecording,
                    ),
                    Some(button(
                        0.0,
                        0.0,
                        150.0,
                        tr("horizon.agent.export_repro"),
                        Act::ExportRepro,
                    )),
                ));
                rows.push(Row::Buttons(
                    self.extra_button(s, crate::extras::Item::ReproKeys),
                    None,
                ));
                rows.extend(self.agent_status.iter().cloned().map(Row::Note));
                rows
            }
            Page::Video => {
                // Vanilla's VideoSettingsScreen as vanilla lays it out. No menu opens it (Video
                // Settings... opens the graphics screen, which has all of it); it stays for
                // `horizon-ctl options video-vanilla` and the tests that pin vanilla's screen.
                let mut rows = vec![Row::Header("options.video.display.header")];
                rows.push(Row::Big(Opt::ExclusiveMode));
                rows.extend(pairs(&[
                    Opt::MaxFps,
                    Opt::Vsync,
                    Opt::InactivityFps,
                    Opt::GuiScale,
                    Opt::Fullscreen,
                    Opt::ExclusiveFullscreen,
                    Opt::Gamma,
                    Opt::GraphicsBackend,
                ]));
                rows.push(Row::Header("options.video.quality.header"));
                rows.push(Row::Big(Opt::Preset));
                rows.extend(pairs(&[
                    Opt::BiomeBlend,
                    Opt::RenderDistance,
                    Opt::ChunkBuilder,
                    Opt::SimulationDistance,
                    Opt::SmoothLighting,
                    Opt::Clouds,
                    Opt::Particles,
                    Opt::Mipmaps,
                    Opt::EntityShadows,
                    Opt::EntityDistance,
                    Opt::MenuBlur,
                    Opt::CloudRange,
                    Opt::CutoutLeaves,
                    Opt::ImprovedTransparency,
                    Opt::TextureFiltering,
                    Opt::MaxAnisotropy,
                    Opt::WeatherRadius,
                ]));
                rows.push(Row::Header("options.video.preferences.header"));
                rows.extend(pairs(&[
                    Opt::Autosave,
                    Opt::Vignette,
                    Opt::AttackIndicator,
                    Opt::ChunkFade,
                ]));
                self.filter_option_rows(rows)
            }
            Page::Sound => {
                // SoundOptionsScreen, without Audio Device: Horizon opens the
                // output device itself and cannot switch it.
                let mut rows = vec![Row::Big(Opt::Volume(0))];
                let others: Vec<Opt> = (1..SOUND_SOURCES.len()).map(Opt::Volume).collect();
                rows.extend(pairs(&others));
                rows.extend(pairs(&[Opt::ShowSubtitles, Opt::DirectionalAudio]));
                rows.extend(pairs(&[Opt::MusicFrequency, Opt::MusicToast]));
                rows.push(Row::Big(Opt::SoundDevice));
                // Horizon's music page (an extra): the built-in tracks, by
                // situation, once it is on in Horizon Extras.
                if s.extras.music_page {
                    rows.push(Row::Buttons(
                        button(
                            0.0,
                            0.0,
                            310.0,
                            tr("horizon.pages.open.music"),
                            Act::Push(Page::Music),
                        ),
                        None,
                    ));
                }
                rows
            }
            Page::Music => {
                if self.music_groups.is_empty() {
                    return vec![Row::Header("horizon.pages.music.empty")];
                }
                let mut rows = Vec::new();
                for group in &self.music_groups {
                    rows.push(Row::Heading(group.label.clone()));
                    for track in &group.tracks {
                        // Left: the track, played now. Right: whether its pool
                        // still picks it (vanilla until one is turned off).
                        let on = !s.extras.music_off.contains(&track.id);
                        let play = button(
                            0.0,
                            0.0,
                            150.0,
                            Text::literal(track_name(&track.id).to_owned()),
                            Act::PlayMusicTrack(track.event.clone(), track.id.clone()),
                        );
                        let toggle = button(
                            0.0,
                            0.0,
                            150.0,
                            on_off(on),
                            Act::MusicTrackOn(track.id.clone(), on),
                        );
                        rows.push(Row::Buttons(play, Some(toggle)));
                    }
                }
                rows
            }
            Page::Skin => {
                // SkinCustomizationScreen: the seven layers, then Main Hand.
                let mut opts: Vec<Opt> = (0..MODEL_PART_KEYS.len()).map(Opt::ModelPart).collect();
                // Horizon's own: the outer layer as real pixels, not one box.
                opts.push(Opt::SkinLayers3d);
                opts.push(Opt::MainHand);
                pairs(&opts)
            }
            Page::Online => {
                // OnlineOptionsScreen without the Xbox and friends-list parts.
                let mut rows = vec![Row::Header("options.online.friends.header")];
                rows.extend(pairs(&[Opt::InGameNotification, Opt::SharePresence]));
                rows.push(Row::Header("options.online.servers.header"));
                rows.push(Row::Big(Opt::AllowServerListing));
                rows.push(Row::Header("options.online.realms.header"));
                rows.push(Row::Big(Opt::RealmsNotifications));
                rows
            }
            Page::Controls => {
                let mut rows = vec![Row::Buttons(
                    button(
                        0.0,
                        0.0,
                        150.0,
                        tr("options.mouse_settings"),
                        Act::Push(Page::Mouse),
                    ),
                    Some(button(
                        0.0,
                        0.0,
                        150.0,
                        tr("controls.keybinds"),
                        Act::Push(Page::KeyBinds),
                    )),
                )];
                let controls = vanilla_control_options();
                rows.extend(pairs(&controls));
                // Horizon's controller page, after vanilla's rows.
                rows.push(Row::Buttons(
                    button(
                        0.0,
                        0.0,
                        310.0,
                        tr("horizon.pages.open.controller"),
                        Act::Push(Page::Controller),
                    ),
                    None,
                ));
                self.filter_option_rows(rows)
            }
            Page::Mouse => pairs(MOUSE_OPTIONS),
            Page::Chat => pairs(&[
                Opt::ChatVisibility,
                Opt::ChatColors,
                Opt::ChatLinks,
                Opt::ChatLinksPrompt,
                Opt::ChatOpacity,
                Opt::TextBackgroundOpacity,
                Opt::ChatScale,
                Opt::ChatLineSpacing,
                Opt::ChatWidth,
                Opt::ChatHeightFocused,
                Opt::ChatHeightUnfocused,
                Opt::Narrator,
                Opt::AutoSuggestions,
                Opt::HideMatchedNames,
                Opt::ReducedDebugInfo,
                Opt::OnlySecureChat,
                Opt::SaveChatDrafts,
            ]),
            Page::Accessibility => {
                let mut rows = vec![Row::Buttons(
                    option_widget(s, Opt::Narrator, 0.0, 0.0, 150.0),
                    Some(button(
                        0.0,
                        0.0,
                        150.0,
                        tr("options.controls"),
                        Act::Push(Page::Controls),
                    )),
                )];
                rows.extend(pairs(&ACCESSIBILITY_OPTIONS[1..]));
                rows
            }
            Page::Font => pairs(&[Opt::ForceUnicode, Opt::JapaneseGlyphs]),
            Page::ModBrowser => self.mod_browser_rows(),
            Page::ModProject => self.mod_project_rows(),
            Page::NativeMod(i) => self.native_rows(s, i),
            Page::Added(i) => self.added_rows(i),
            _ => {
                let _ = s;
                Vec::new()
            }
        }
    }
    /// A section heading of a list (`Row::Header`, `Row::Heading`), at `y`.
    fn heading_row(&self, out: &mut Vec<Widget>, label: Text, y: &mut f32, w: f32, top: f32) {
        let padding = if *y + self.scroll <= top + 2.0 {
            0.0
        } else {
            18.0
        };
        let label = label.with_style(&Style {
            bold: Some(true),
            underlined: Some(true),
            ..Style::default()
        });
        out.push(Widget {
            x: 0.0,
            y: *y + 2.0 + padding + 4.0,
            w,
            h: 9.0,
            kind: WKind::Text {
                text: label,
                color: u32::MAX,
                centered: true,
            },
            list: false,
            row: 0,
        });
        *y += padding + 18.0;
    }
    fn clamp_scroll(&mut self, top: f32, bottom: f32, content: f32) {
        let max = (content - (bottom - top)).max(0.0);
        self.scroll = self.scroll.clamp(0.0, max);
    }
    /// The page's list as the narrator reads it: Language's
    /// `ObjectSelectionList` ("Selected: <language>"), or the
    /// `ContainerObjectSelectionList` the rows' widgets belong to, with the row
    /// under the mouse (`getEntryAtPosition`: within the row's width).
    fn narrate_list(
        &self,
        widgets: &[Widget],
        key: u64,
        (top, bottom): (f32, f32),
        w: f32,
        cursor: (f32, f32),
    ) {
        use crate::widgets::{ListKind, Narratable, Row as Narrated};
        let rect = [0.0, top, w, bottom - top];
        let in_list = cursor.1 >= top && cursor.1 < bottom;
        if self.page() == Page::Language {
            crate::widgets::narratable(|| {
                let rows: Vec<(&Widget, &Text, bool)> = widgets
                    .iter()
                    .filter_map(|wd| match &wd.kind {
                        WKind::LanguageRow {
                            label, selected, ..
                        } => Some((wd, label, *selected)),
                        _ => None,
                    })
                    .collect();
                let narrated = |(wd, label, _): &(&Widget, &Text, bool)| {
                    Narrated::new(
                        wd.row,
                        Text::translate("narrator.select", &[(*label).clone()]).plain(),
                    )
                };
                let hovered = rows
                    .iter()
                    .find(|(wd, ..)| in_list && ui::hovered(cursor, wd.x, wd.y, wd.w, wd.h));
                let selected = rows.iter().find(|(_, _, selected)| *selected);
                Narratable::list(
                    ListKind::Object,
                    rows.len(),
                    hovered.map(narrated),
                    selected.map(narrated),
                    rect,
                )
                .key(key)
            });
            return;
        }
        // OptionsList rows are 310 wide, KeyBindsList's 340.
        let half = if self.page() == Page::KeyBinds {
            170.0
        } else {
            155.0
        };
        let hovered_row = (in_list && (cursor.0 - (w / 2.0).floor()).abs() < half)
            .then(|| {
                self.row_bands
                    .iter()
                    .position(|[a, b]| cursor.1 >= *a && cursor.1 < *b)
            })
            .flatten();
        crate::widgets::narratable(|| {
            Narratable::container_list(self.row_bands.len(), hovered_row, rect).key(key)
        });
    }
    /// Mouse press/drag. Returns events for the game.
    pub fn click(
        &mut self,
        s: &mut Settings,
        atlas: &HudAtlas,
        x: f32,
        y: f32,
        size: (f32, f32),
        framebuffer: [i32; 2],
        drag: bool,
        right: bool,
    ) -> Vec<Event> {
        let events = self.click_unrevised(s, atlas, x, y, size, framebuffer, drag, right);
        // A click that changed nothing the layout reads (an empty spot, a
        // focus move) keeps the built layout.
        if !events.is_empty() {
            self.relayout();
        }
        events
    }
    /// `click` without the layout revision: the events it answered with say
    /// whether the layout's inputs moved.
    fn click_unrevised(
        &mut self,
        s: &mut Settings,
        atlas: &HudAtlas,
        x: f32,
        y: f32,
        size: (f32, f32),
        framebuffer: [i32; 2],
        drag: bool,
        right: bool,
    ) -> Vec<Event> {
        if self.resource_error.is_some() {
            if !drag
                && !right
                && ui::hovered((x, y), size.0 / 2.0 - 100.0, size.1 - 28.0, 200.0, 20.0)
            {
                self.resource_error = None;
                return vec![Event::ButtonClick];
            }
            return Vec::new();
        }
        if self.page() == Page::SplitSetup {
            use crate::split_ui::Click;
            if right {
                return Vec::new();
            }
            self.split_setup.on = s.extras.splitscreen;
            return match self.split_setup.click((x, y), size, drag) {
                Click::Nothing => Vec::new(),
                Click::Pressed => vec![Event::ButtonClick],
                Click::Switch => {
                    crate::extras::Item::Splitscreen.cycle(&mut s.extras, false);
                    vec![Event::ButtonClick, Event::Changed]
                }
                Click::Done => {
                    let mut events = vec![Event::ButtonClick];
                    events.extend(self.back(s));
                    events
                }
            };
        }
        if self.page() == Page::SplitProfiles {
            use crate::split_profiles::Click;
            if right || drag {
                return Vec::new();
            }
            return match self.split_profiles.click(atlas, (x, y), size) {
                Click::Nothing => Vec::new(),
                Click::Pressed => vec![Event::ButtonClick],
                Click::Done => {
                    let mut events = vec![Event::ButtonClick];
                    events.extend(self.back(s));
                    events
                }
            };
        }
        if self.page() == Page::Studio {
            if right || drag {
                return Vec::new();
            }
            use crate::studio_ui::StudioAction;
            return match self.studio.click((x, y), size) {
                StudioAction::None => Vec::new(),
                StudioAction::Refresh => vec![Event::ButtonClick],
                StudioAction::OpenReplay(path) => {
                    vec![Event::ButtonClick, Event::OpenStudioReplay(path)]
                }
                StudioAction::OpenFolder => vec![Event::ButtonClick, Event::OpenStudioFolder],
                StudioAction::Back => {
                    let mut events = vec![Event::ButtonClick];
                    events.extend(self.back(s));
                    events
                }
            };
        }
        if self.page() == Page::HdrCalibration
            && let Some(c) = &mut self.calibration
        {
            if right {
                return Vec::new();
            }
            let outcome = c.click(&atlas.font, (x, y), size, drag);
            return self.calibration_outcome(s, outcome);
        }
        if self.page() == Page::ControllerRadialName {
            return self.click_radial_name(s, atlas, (x, y), size, drag || right);
        }
        if self.shell().is_some() {
            // Waiting on a controller button: a click cancels the wait.
            if !drag && self.selected_pad_action.take().is_some() {
                return Vec::new();
            }
            // Waiting on a key: a mouse button binds, as on Key Binds.
            if self.selected_key.is_some() && !drag {
                let button = if right {
                    winit::event::MouseButton::Right
                } else {
                    winit::event::MouseButton::Left
                };
                return self.bind_mouse(s, button);
            }
            let shift = self.shift;
            self.framebuffer = framebuffer;
            self.shell_framebuffers();
            let outputs = self.shell_mut().map_or_else(Vec::new, |shell| {
                shell.click(s, (x, y), size, drag, right, shift)
            });
            return self.shell_events(s, outputs);
        }
        if self.page() == Page::Mods {
            let mut events = Vec::new();
            if drag {
                self.mods_page(s, size).drag(y as f64);
                return events;
            }
            if right {
                return events;
            }
            let m = crate::screen_draw::Measure(&atlas.font);
            let screen = self.mods_page(s, size);
            let before = screen.search_focused();
            let (action, sound) = screen.click(&m, x as f64, y as f64);
            if !before && screen.search_focused() {
                self.pack_focus = Instant::now();
            }
            if sound {
                events.push(Event::ButtonClick);
            }
            use crate::mods_screen::Action as Do;
            match action {
                Do::None => {}
                Do::OpenFolder => events.push(Event::OpenModFolder),
                Do::Back => events.extend(self.back(s)),
                Do::Config(i) => {
                    self.push(Page::NativeMod(i));
                    events.push(Event::NativeSettings(i));
                }
                Do::Link(url) => events.push(Event::OpenLink(url)),
                Do::Copy(text) => events.push(Event::CopyText(text)),
            }
            return events;
        }
        if self.page() == Page::Packs {
            let mut events = Vec::new();
            if drag {
                self.packs(atlas, size).drag(y as f64);
                return events;
            }
            if right {
                return events;
            }
            let m = crate::screen_draw::Measure(&atlas.font);
            let p = self.packs(atlas, size);
            if p.button_at(&m, x as f64, y as f64) {
                events.push(Event::ButtonClick);
            }
            let before = p.search_focused();
            let action = p.click(&m, x as f64, y as f64);
            if !before && p.search_focused() {
                self.pack_focus = Instant::now();
            }
            match action {
                crate::pack_screen::Action::Import(paths) => events.push(Event::ImportPacks(paths)),
                crate::pack_screen::Action::OpenFolder => events.push(match &self.data_pack_dir {
                    Some(dir) => Event::OpenDataPackFolder(dir.clone()),
                    None => Event::OpenPackFolder,
                }),
                crate::pack_screen::Action::Done => events.extend(self.back(s)),
                crate::pack_screen::Action::BrowsePick(i) => {
                    events.push(Event::DownloadPack(i));
                }
                crate::pack_screen::Action::Options(id) => self.open_pack_options(&id),
                crate::pack_screen::Action::None => {}
            }
            return events;
        }
        let layout = self.built_layout(s, size.0, size.1);
        let (widgets, top, bottom, content) = layout.parts();
        self.size = size;
        self.framebuffer = framebuffer;
        let mut events = Vec::new();
        if !drag
            && matches!(
                self.page(),
                Page::Video | Page::Controls | Page::KeyBinds | Page::ModBrowser
            )
        {
            let search = &mut self.settings_search;
            search.focused = ui::hovered((x, y), search.x, search.y, search.width, search.height);
            if search.focused {
                self.focus = Some(FocusId::Search);
                self.focus_from_keyboard = false;
                self.can_change_value = false;
                search.click(atlas, x, false, false);
                self.scroll = 0.0;
                events.push(Event::ButtonClick);
                return events;
            }
        }
        if !drag && self.page() == Page::Language {
            let search = &mut self.language_search;
            search.focused = ui::hovered((x, y), search.x, search.y, search.width, search.height);
            if search.focused {
                self.focus = Some(FocusId::Search);
                self.focus_from_keyboard = false;
                self.can_change_value = false;
                search.click(atlas, x, false, false);
                self.scroll = 0.0;
                return events;
            }
        }
        if drag {
            if let Some(previous_y) = self.scrollbar_drag_y {
                if let Some((_, thumb)) = ui::scrollbar_thumb(top, bottom, content, self.scroll) {
                    let max = content - (bottom - top);
                    self.scroll = if y < top {
                        0.0
                    } else if y > bottom {
                        max
                    } else {
                        self.scroll + (y - previous_y) * max / (bottom - top - thumb)
                    };
                    self.clamp_scroll(top, bottom, content);
                    self.scrollbar_drag_y = Some(y);
                } else {
                    self.scrollbar_drag_y = None;
                }
                return events;
            }
            if let Some(opt) = self.dragging
                && let Some(wd) = widgets
                    .iter()
                    .find(|wd| matches!(&wd.kind, WKind::Slider { opt: o, .. } if *o == opt))
            {
                let v = ui::slider_value(wd.x, wd.w, x);
                self.slider_value = Some((opt, v, Instant::now()));
                drag_slider(opt, s, v, &mut events);
            }
            return events;
        }
        // Key Binds: a mouse button pressed while waiting binds it.
        if self.selected_key.is_some() {
            let button = if right {
                winit::event::MouseButton::Right
            } else {
                winit::event::MouseButton::Left
            };
            return self.bind_mouse(s, button);
        }
        // AbstractWidget accepts only the primary button. Rebinding above
        // accepts other mouse buttons, but is not a widget activation.
        if right {
            return events;
        }
        if ui::scrollbar_thumb(top, bottom, content, self.scroll).is_some()
            && ui::hovered((x, y), self.scrollbar_x(size.0), top, 6.0, bottom - top)
        {
            self.scrollbar_drag_y = Some(y);
            return events;
        }
        for (wd_idx, wd) in widgets.iter().enumerate() {
            if !ui::hovered((x, y), wd.x, wd.y, wd.w, wd.h) {
                continue;
            }
            if wd.list && (y < top || y >= bottom) {
                continue;
            }
            // AbstractWidget.mouseClicked rejects inactive buttons before the
            // container can focus them.
            if matches!(&wd.kind, WKind::Button { active: false, .. }) {
                return events;
            }
            // A click focuses the widget for its highlight only: vanilla's wheel
            // steps a keyboard-focused widget from anywhere, not a clicked one.
            self.focus = Some(focus_id(wd_idx, wd));
            self.focus_from_keyboard = false;
            match &wd.kind {
                WKind::Button {
                    active: true, act, ..
                } => {
                    self.can_change_value = false;
                    events.push(Event::ButtonClick);
                    match act {
                        Act::Push(p) => {
                            if *p == Page::ShaderPacks {
                                // The pack list, and the picked pack's own settings with it.
                                events.push(Event::ListShaderPacks);
                                s.shader_options = None;
                                events.push(Event::OpenShaderOptions);
                            }
                            if *p == Page::Music {
                                events.push(Event::ListMusicTracks);
                            }
                            if *p == Page::ModBrowser {
                                events.push(Event::BrowseMods);
                            }
                            self.push(*p)
                        }
                        Act::Done => events.extend(self.back(s)),
                        Act::Cycle(opt) => {
                            events.extend(self.cycle_option(s, *opt, self.shift, framebuffer));
                        }
                        Act::Extra(item) => {
                            if let Some(name) = item.key_name() {
                                self.selected_key = Some(name);
                            } else {
                                item.cycle(&mut s.extras, self.shift);
                                events.push(Event::Changed);
                            }
                        }
                        Act::PlayMusicTrack(event, track) => {
                            events.push(Event::PlayMusicTrack(event.clone(), track.clone()));
                        }
                        Act::MusicTrackOn(track, was_on) => {
                            s.extras
                                .music_off
                                .retain(|off| off.as_str() != track.as_str());
                            if *was_on {
                                s.extras.music_off.push(track.clone());
                            }
                            events.push(Event::Changed);
                        }
                        Act::Key(name) => self.selected_key = Some(*name),
                        Act::ResetKey(name) => {
                            s.keys.set(name, KeyBinds::default_of(name));
                            events.push(Event::Changed);
                        }
                        Act::ResetKeys => {
                            s.keys.reset_all();
                            s.mods.keys.clear();
                            events.push(Event::Changed);
                        }
                        Act::ModKey(i, k) => {
                            self.selected_key = Some(MOD_KEY);
                            self.selected_mod_key = Some((*i, *k));
                            self.pending_mod_key = None;
                        }
                        Act::ResetModKey(i, k) => {
                            if let Some((id, mapping)) = mod_mapping(s, *i, *k) {
                                s.mods.reset_binding(&id, &mapping);
                                events.push(Event::Changed);
                            }
                        }
                        Act::PageRow(p, r) => events.extend(self.page_act(*p, *r, self.shift)),
                        Act::NativeSetting(i, j) => {
                            let setting = s
                                .mods
                                .installed
                                .get_mut(*i)
                                .and_then(|m| m.native.as_mut())
                                .and_then(|page| page.settings.get_mut(*j));
                            if let Some(setting) = setting {
                                setting.value.click();
                                events.push(Event::NativeSetting(*i, *j));
                            }
                        }
                        Act::ModProject(i) => {
                            if let Some(listing) = self.mod_browser.listings.get(*i) {
                                self.mod_project = listing.id.clone();
                                self.push(Page::ModProject);
                            }
                        }
                        Act::InstallMod => {
                            self.mod_browser.message = None;
                            events.push(Event::InstallMod(self.mod_project.clone()));
                        }
                        Act::OpenModPage => {
                            if let Some(l) = self.mod_browser.listing(&self.mod_project) {
                                events.push(Event::OpenModPage(l.page.clone()));
                            }
                        }
                        Act::PvpPreset => {
                            crate::pvp::apply_preset(s);
                            events.push(Event::Changed);
                        }
                        Act::HudEditor => events.push(Event::HudEditor),
                        Act::AccessibilityHelp => events.push(Event::AccessibilityHelp),
                        Act::Credits => events.push(Event::Credits),
                        Act::CopyAgentDetails => events.push(Event::CopyAgentDetails),
                        Act::ToggleRecording => events.push(Event::ToggleRecording),
                        Act::ExportRepro => events.push(Event::ExportRepro),
                        Act::None => {}
                    }
                }
                WKind::Slider { opt, .. } => {
                    self.can_change_value = true;
                    self.dragging = Some(*opt);
                    let v = ui::slider_value(wd.x, wd.w, x);
                    self.slider_value = Some((*opt, v, Instant::now()));
                    drag_slider(*opt, s, v, &mut events);
                }
                WKind::LanguageRow { code, .. } => {
                    self.can_change_value = false;
                    if self.language_choice.as_deref() == Some(code.as_str()) {
                        events.extend(self.back(s));
                    } else {
                        self.language_choice = Some(code.clone());
                        self.relayout();
                    }
                }
                _ => {}
            }
            break;
        }
        events
    }
    /// Where a slider's handle sits: its own position while held (or until
    /// a delayed one applies), else the option's stepped value.
    fn slider_shown(&self, opt: Opt, value: f32) -> f32 {
        match self.slider_value {
            Some((o, v, at)) if o == opt && slider_unsnapped(o, self.dragging == Some(o), at) => v,
            _ => value,
        }
    }
    pub fn release(&mut self) -> Vec<Event> {
        self.scrollbar_drag_y = None;
        if let Some(p) = &mut self.pack_screen {
            p.release();
        }
        if let Some(p) = &mut self.mods_screen {
            p.release();
        }
        // AbstractSliderButton suppresses the press sound and plays once on
        // release, even if the pointer has left the widget or its value stayed.
        let slider = self.dragging.take().is_some();
        let shell_slider = self.shell_mut().and_then(|shell| shell.release());
        let calibration_slider = self.calibration.as_mut().is_some_and(|c| c.release());
        let split_drop = self.split_setup.release();
        if slider || shell_slider.is_some() || calibration_slider {
            let mut events = vec![Event::ButtonClick, Event::Changed];
            // What the screen does once a drag ends: a shader pack's option is saved, and the
            // pack rebuilt, once per drag.
            let done = shell_slider.into_iter().flatten();
            events.extend(done.filter_map(|output| match output {
                shell::Output::Event(event) => Some(event),
                _ => None,
            }));
            events
        } else if split_drop {
            vec![Event::ButtonClick]
        } else {
            Vec::new()
        }
    }
    pub fn scroll(&mut self, s: &Settings, size: (f32, f32), amount: f32) {
        let layout = self.built_layout(s, size.0, size.1);
        let (_, top, bottom, content) = layout.parts();
        self.scroll -= amount * 18.0;
        self.clamp_scroll(top, bottom, content);
        self.relayout();
    }
    /// `CycleButton.cycleValue` on an option, then what the option changes:
    /// the preset's values, High Contrast's built-in pack.
    fn cycle_option(
        &mut self,
        s: &mut Settings,
        opt: Opt,
        back: bool,
        framebuffer: [i32; 2],
    ) -> Vec<Event> {
        let mut events = Vec::new();
        opt.cycle(s, back, framebuffer);
        if opt == Opt::ReducedDebugInfo {
            events.push(Event::DebugEntriesChanged);
        }
        if opt == Opt::Preset {
            apply_preset(s);
        }
        if opt == Opt::HighContrast {
            // Options.highContrast: the built-in pack follows the option.
            let on = s.resource_packs.iter().any(|id| id == "high_contrast");
            if on != s.high_contrast {
                s.resource_packs.retain(|id| id != "high_contrast");
                if s.high_contrast {
                    s.resource_packs.push("high_contrast".into());
                }
                // PackSelectionScreen reads the repository each time it
                // opens: the next one starts from this selection.
                self.pack_selection = s.resource_packs.clone();
                self.pack_screen = None;
                events.push(Event::Packs(s.resource_packs.clone()));
            }
        }
        events.push(Event::Changed);
        if opt == Opt::Narrator {
            events.push(Event::Narrator);
        }
        events
    }
    /// The mouse wheel at the cursor. A vanilla list keeps the wheel for itself:
    /// `AbstractContainerWidget` inherits `AbstractScrollArea.mouseScrolled`, which
    /// scrolls and never reaches the list's rows. A cycle button outside a list is
    /// the screen's own child, so it takes the step (`CycleButton.mouseScrolled`: up
    /// goes back). `AbstractSliderButton` does not override it, so a slider outside
    /// a list does nothing.
    ///
    /// **Horizon extra (diverges from Vanilla 26.3, off by default):** "Scroll wheel
    /// changes settings" lets a hovered list row take the step too, and a slider or
    /// cycle button focused by the keyboard (Tab, the arrows, a pad) step on the
    /// wheel wherever the cursor is. Focus left by a click does not count
    /// (`focus_from_keyboard`): honouring it toggled the clicked option on every
    /// later wheel event, from anywhere on the screen, while the list never scrolled.
    pub fn wheel(
        &mut self,
        s: &mut Settings,
        size: (f32, f32),
        cursor: (f32, f32),
        framebuffer: [i32; 2],
        amount: f32,
    ) -> Vec<Event> {
        let events = self.wheel_unrevised(s, size, cursor, framebuffer, amount);
        if !events.is_empty() {
            self.relayout();
        }
        events
    }
    /// `wheel` without the layout revision: a scroll bumps it in `scroll`, and
    /// the events of a setting's step say whether its label moved.
    fn wheel_unrevised(
        &mut self,
        s: &mut Settings,
        size: (f32, f32),
        cursor: (f32, f32),
        framebuffer: [i32; 2],
        amount: f32,
    ) -> Vec<Event> {
        self.size = size;
        self.framebuffer = framebuffer;
        if self.resource_error.is_some() {
            return Vec::new();
        }
        self.shell_framebuffers();
        if let Some(shell) = self.shell_mut() {
            let outputs = shell.wheel(s, size, cursor, amount);
            return self.shell_events(s, outputs);
        }
        if self.page() == Page::Studio {
            self.studio.wheel(amount);
            return Vec::new();
        }
        let plain = self.resource_error.is_none()
            && !(self.pack_screen.is_some() && self.stack.last() == Some(&Page::Packs))
            && self.stack.last() != Some(&Page::Mods);
        if !plain {
            self.scroll_at(s, size, cursor, amount);
            return Vec::new();
        }
        let extra = s.extras.wheel_cycles_settings;
        // The extra: a keyboard-focused slider or cycle button intercepts the
        // wheel before hover detection (see doc-comment above). A click focuses
        // its widget for the highlight too, and the wheel must not act on that:
        // it would toggle the clicked option from anywhere on the screen and the
        // list would never scroll.
        if extra && amount != 0.0 && self.focus_from_keyboard {
            let layout = self.built_layout(s, size.0, size.1);
            let (widgets, _, _, _) = layout.parts();
            match &self.focus {
                Some(FocusId::Slider(opt)) => {
                    let opt = *opt;
                    let width = widgets
                        .iter()
                        .find(|wd| matches!(&wd.kind, WKind::Slider { opt: o, .. } if *o == opt))
                        .map_or(150.0, |wd| wd.w);
                    return self.step_slider_option(s, opt, amount > 0.0, width);
                }
                Some(FocusId::Cycle(opt)) => {
                    let opt = *opt;
                    let mut events = vec![Event::ButtonClick];
                    events.extend(self.cycle_option(s, opt, amount > 0.0, framebuffer));
                    return events;
                }
                _ => {}
            }
        }
        enum HoveredControl {
            Cycle(Opt),
            Extra(crate::extras::Item),
        }
        let layout = self.built_layout(s, size.0, size.1);
        let (widgets, top, bottom, _) = layout.parts();
        // A list's row takes the wheel only with the extra on.
        let takes_wheel = |wd: &Widget| {
            ui::hovered(cursor, wd.x, wd.y, wd.w, wd.h)
                && (!wd.list || (extra && cursor.1 >= top && cursor.1 < bottom))
        };
        let hovered = widgets.iter().find_map(|wd| match &wd.kind {
            WKind::Button {
                active: true,
                act: Act::Cycle(opt),
                ..
            } if takes_wheel(wd) => Some(HoveredControl::Cycle(*opt)),
            WKind::Button {
                active: true,
                act: Act::Extra(item),
                ..
            } if takes_wheel(wd) => Some(HoveredControl::Extra(*item)),
            _ => None,
        });
        match hovered {
            Some(HoveredControl::Cycle(opt)) if amount != 0.0 => {
                self.cycle_option(s, opt, amount > 0.0, framebuffer)
            }
            Some(HoveredControl::Extra(item)) if amount != 0.0 => {
                item.cycle(&mut s.extras, amount > 0.0);
                vec![Event::Changed]
            }
            Some(_) => Vec::new(),
            None => {
                self.scroll_at(s, size, cursor, amount);
                Vec::new()
            }
        }
    }
    /// The mouse wheel at the cursor.
    pub fn scroll_at(&mut self, s: &Settings, size: (f32, f32), cursor: (f32, f32), amount: f32) {
        if self.resource_error.is_some() {
            return;
        }
        if let Some(shell) = self.shell_mut() {
            shell.scroll(s, size, amount);
            return;
        }
        if self.page() == Page::Studio {
            self.studio.wheel(amount);
            return;
        }
        if self.page() == Page::Mods {
            if let Some(p) = &mut self.mods_screen {
                p.scroll(cursor.0 as f64, cursor.1 as f64, amount as f64);
            }
            return;
        }
        match &mut self.pack_screen {
            Some(p) if self.stack.last() == Some(&Page::Packs) => {
                p.scroll(cursor.0 as f64, cursor.1 as f64, amount as f64)
            }
            _ => self.scroll(s, size, amount),
        }
    }
    /// `VideoSettingsScreen.mouseScrolled`: with Ctrl held the wheel steps the GUI scale
    /// (auto counts as one past the maximum) and the list goes back to the top.
    pub fn video_wheel_gui_scale(
        &mut self,
        s: &mut Settings,
        framebuffer: [i32; 2],
        amount: f32,
    ) -> Vec<Event> {
        let max = i64::from(crate::window::calculate_scale(
            framebuffer,
            0,
            s.force_unicode,
        ));
        let old = if s.gui_scale == 0 {
            max + 1
        } else {
            i64::from(s.gui_scale)
        };
        let step = i64::from(amount > 0.0) - i64::from(amount < 0.0);
        let new = old + step;
        if new == 0 || new > max {
            return Vec::new();
        }
        s.gui_scale = new as u32;
        self.scroll = 0.0;
        self.relayout();
        vec![Event::Changed]
    }
    /// The Mods screen at this GUI size, made on first use from the installed mods.
    fn mods_page(&mut self, s: &Settings, size: (f32, f32)) -> &mut crate::mods_screen::ModsScreen {
        let (w, h) = (size.0 as i32, size.1 as i32);
        let in_world = self.in_world;
        let screen = self
            .mods_screen
            .get_or_insert_with(|| crate::mods_screen::ModsScreen::new(w, h, in_world, &s.mods));
        if screen.width != w || screen.height != h {
            screen.resize(w, h);
        }
        screen
    }
    /// The pack screen at this GUI size, made on first use.
    fn packs(&mut self, atlas: &HudAtlas, size: (f32, f32)) -> &mut crate::pack_screen::PackScreen {
        let m = crate::screen_draw::Measure(&atlas.font);
        let (w, h) = (size.0 as i32, size.1 as i32);
        match &mut self.pack_screen {
            Some(p) => {
                if p.width != w || p.height != h {
                    p.resize(&m, w, h);
                }
            }
            None => {
                self.pack_focus = Instant::now();
                let title = if self.data_pack_dir.is_some() {
                    "dataPack.title"
                } else {
                    "resourcePack.title"
                };
                let mut screen = crate::pack_screen::PackScreen::new(
                    &m,
                    crate::text::Text::translate(title, &[]),
                    w,
                    h,
                    self.in_world,
                    self.pack_infos.clone(),
                    &self.pack_selection,
                );
                screen.browsing_allowed = self.pack_browser;
                screen.changed = std::mem::take(&mut self.pack_reselected);
                self.pack_screen = Some(screen);
            }
        }
        // The cards of packs with Polytone options carry their button (data packs have none).
        let with_options = if self.data_pack_dir.is_none() {
            self.pack_configs
                .iter()
                .filter(|(_, c)| !c.entries().is_empty())
                .map(|(id, _)| id.clone())
                .collect()
        } else {
            Default::default()
        };
        let screen = self.pack_screen.as_mut().expect("made above");
        screen.with_options = with_options;
        screen
    }
    /// Escape or Done: leave the current page, applying page results.
    /// After an input on HDR calibration: Save keeps what it landed on, Cancel what was
    /// set before; both go back to the graphics screen's HDR page.
    fn calibration_outcome(
        &mut self,
        s: &mut Settings,
        outcome: crate::hdr_calibration::Outcome,
    ) -> Vec<Event> {
        use crate::hdr_calibration::Outcome;
        match outcome {
            Outcome::Nothing | Outcome::Changed => Vec::new(),
            Outcome::Click => vec![Event::ButtonClick],
            Outcome::Save | Outcome::Cancel => {
                if let Some(c) = self.calibration.take()
                    && outcome == Outcome::Save
                {
                    c.save(s);
                }
                let mut events = vec![Event::ButtonClick];
                events.extend(self.back(s));
                events
            }
        }
    }
    /// HDR calibration's test picture, while its page is open, in a GUI of `size`.
    pub fn calibration_picture(&self, atlas: &HudAtlas, size: (f32, f32)) -> Option<Picture> {
        self.calibration
            .as_ref()
            .filter(|_| self.page() == Page::HdrCalibration)
            .map(|c| c.picture(&atlas.font, size))
    }
    pub fn back(&mut self, s: &mut Settings) -> Vec<Event> {
        self.relayout();
        let mut events = Vec::new();
        // A pack's screen goes back to the one that opened it.
        if self.page() == Page::ShaderOptions
            && let Some(options) = &mut s.shader_options
            && options.path.pop().is_some()
        {
            self.scroll = 0.0;
            return events;
        }
        let page = self.stack.pop().unwrap_or(Page::Main);
        self.scroll = 0.0;
        self.scrollbar_drag_y = None;
        self.dragging = None;
        self.slider_value = None;
        self.selected_key = None;
        self.pending_mod_key = None;
        self.selected_pad_action = None;
        self.focus = None;
        self.focus_from_keyboard = false;
        self.settings_search.focused = false;
        self.language_search.focused = false;
        self.can_change_value = false;
        match page {
            Page::PackOptions => {
                let screen = std::mem::take(&mut self.pack_options.screen);
                events.extend(screen.finished());
            }
            Page::ShaderOptions | Page::ShaderPacks => {
                if s.shader_options.as_ref().is_some_and(|o| o.changed) {
                    events.push(Event::ShaderOptions);
                } else {
                    s.shader_options = None;
                }
            }
            Page::Language => {
                if let Some(code) = self.language_choice.take()
                    && code != s.language
                {
                    s.language = code.clone();
                    events.push(Event::Language(code));
                }
            }
            Page::Mods => self.mods_screen = None,
            // Browse mods may have installed some: the list is made again.
            Page::ModBrowser => self.mods_screen = None,
            Page::Packs => {
                // PackSelectionScreen.onClose: PackSelectionModel.commit.
                let data = self.data_pack_dir.take();
                if let Some(p) = self.pack_screen.take()
                    && p.changed
                {
                    let ids = p.commit();
                    if data.is_some() {
                        events.push(Event::DataPacks(ids));
                    } else {
                        // `PackSelectionModel.commit` already drops `server/…`
                        // ids; keep the same filter if the screen is rebuilt.
                        let ids = crate::packs::player_pack_ids(ids);
                        s.resource_packs = ids.clone();
                        self.pack_selection = ids.clone();
                        events.push(Event::Packs(ids));
                    }
                }
            }
            _ => {}
        }
        events.push(Event::Changed);
        if self.stack.is_empty() {
            events.push(Event::Close);
        }
        events
    }
    /// Typed text (the language search box).
    pub fn type_text(&mut self, atlas: &HudAtlas, text: &str) {
        if self.resource_error.is_some() {
            return;
        }
        if self.page() == Page::Packs
            && let Some(p) = &mut self.pack_screen
        {
            p.type_text(&crate::screen_draw::Measure(&atlas.font), text);
            return;
        }
        if self.page() == Page::Mods
            && let Some(p) = &mut self.mods_screen
        {
            p.type_text(&crate::screen_draw::Measure(&atlas.font), text);
            return;
        }
        if self.page() == Page::Language {
            self.language_search.insert_text(text);
            self.scroll = 0.0;
            self.relayout();
        } else if self.page() == Page::SplitProfiles {
            self.split_profiles.type_text(text);
        } else if self.page() == Page::ControllerRadialName {
            self.type_radial_name(text);
        } else if let Some(shell) = self.shell_mut() {
            if shell.typing() {
                shell.type_text(text);
            }
        } else if self.settings_search_active() {
            self.settings_search.insert_text(text);
            self.scroll = 0.0;
            self.relayout();
        }
    }
    /// Ctrl+A/C/V/X in the language or pack search box; false when neither
    /// is shown with the focus.
    pub fn shortcut(
        &mut self,
        atlas: &HudAtlas,
        shortcut: crate::clipboard::EditShortcut,
        clipboard: &mut dyn crate::clipboard::Clipboard,
    ) -> bool {
        if self.resource_error.is_some() || self.selected_key.is_some() {
            return false;
        }
        if self.page() == Page::Packs
            && let Some(p) = &mut self.pack_screen
        {
            let m = crate::screen_draw::Measure(&atlas.font);
            return p.shortcut(&m, shortcut, clipboard);
        }
        if self.page() == Page::Mods
            && let Some(p) = &mut self.mods_screen
        {
            let m = crate::screen_draw::Measure(&atlas.font);
            return p.shortcut(&m, shortcut, clipboard);
        }
        if self.page() == Page::Language {
            if self.language_search.shortcut(shortcut, clipboard) {
                self.scroll = 0.0;
            }
            // Select all, copy, paste and cut: the box's text is a layout input.
            self.relayout();
            return true;
        }
        if self.page() == Page::SplitProfiles {
            return self.split_profiles.shortcut(shortcut, clipboard);
        }
        if self.page() == Page::ControllerRadialName {
            return self.radial_name_shortcut(shortcut, clipboard);
        }
        if let Some(shell) = self.shell_mut() {
            return shell.shortcut(shortcut, clipboard);
        }
        if self.settings_search_active() {
            if self.settings_search.shortcut(shortcut, clipboard) {
                self.scroll = 0.0;
            }
            self.relayout();
            return true;
        }
        false
    }
    fn bind_key(&mut self, s: &mut Settings, value: String) -> Vec<Event> {
        // The waiting marker and the binding are both layout inputs.
        self.relayout();
        self.pending_mod_key = None;
        let Some(name) = self.selected_key.take() else {
            return Vec::new();
        };
        if name == pages::PAGE_KEY {
            return self.bind_page_key(value);
        }
        if name == MOD_KEY {
            let (i, k) = self.selected_mod_key.take().unwrap_or_default();
            let Some((id, mapping)) = mod_mapping(s, i, k) else {
                return Vec::new();
            };
            // A modifier key bound on its own is the key, not a modifier held with itself.
            let own = match value.as_str() {
                "key.keyboard.left.shift" | "key.keyboard.right.shift" => crate::mod_list::SHIFT,
                "key.keyboard.left.control" | "key.keyboard.right.control" => {
                    crate::mod_list::CONTROL
                }
                "key.keyboard.left.alt" | "key.keyboard.right.alt" => crate::mod_list::ALT,
                _ => 0,
            };
            let modifiers = self.modifiers
                & (crate::mod_list::SHIFT | crate::mod_list::CONTROL | crate::mod_list::ALT)
                & !own;
            s.mods.set_binding(&id, &mapping, &value, modifiers);
            return vec![Event::Changed];
        }
        if !s.extras.set_key(name, value.clone()) {
            s.keys.set(name, &value);
        }
        vec![Event::Changed]
    }
    /// Mouse bindings use the same settings and persistence as keyboard bindings.
    pub fn bind_mouse(
        &mut self,
        s: &mut Settings,
        button: winit::event::MouseButton,
    ) -> Vec<Event> {
        self.bind_key(s, keybinds::mouse_name(button))
    }
    /// Completes a modifier-only mod binding when its key is released.
    pub fn key_released(&mut self, s: &mut Settings, key: winit::keyboard::KeyCode) -> Vec<Event> {
        if self.selected_key == Some(MOD_KEY) && self.pending_mod_key == Some(key) {
            return self.bind_key(
                s,
                keybinds::key_name(key)
                    .unwrap_or("key.keyboard.unknown")
                    .to_owned(),
            );
        }
        Vec::new()
    }
    /// Keys while the options are open. Returns events; `handled` false lets
    /// Escape fall through.
    pub fn key(
        &mut self,
        s: &mut Settings,
        atlas: &HudAtlas,
        key: winit::keyboard::KeyCode,
        text: Option<&str>,
    ) -> Vec<Event> {
        self.key_modifiers(s, atlas, key, text, false)
    }

    /// Keys while the options are open, including Shift modifier state.
    pub fn key_modifiers(
        &mut self,
        s: &mut Settings,
        atlas: &HudAtlas,
        key: winit::keyboard::KeyCode,
        text: Option<&str>,
        shift: bool,
    ) -> Vec<Event> {
        let events = self.key_modifiers_unrevised(s, atlas, key, text, shift);
        if !events.is_empty() {
            self.relayout();
        }
        events
    }
    /// `key_modifiers` without the layout revision: search text and the keys
    /// that change a setting bump it themselves, and the events of the rest
    /// say whether the layout's inputs moved.
    fn key_modifiers_unrevised(
        &mut self,
        s: &mut Settings,
        atlas: &HudAtlas,
        key: winit::keyboard::KeyCode,
        text: Option<&str>,
        shift: bool,
    ) -> Vec<Event> {
        use winit::keyboard::KeyCode as K;
        if self.page() == Page::HdrCalibration
            && let Some(c) = &mut self.calibration
        {
            let outcome = c.key(key);
            return self.calibration_outcome(s, outcome);
        }
        if self.resource_error.is_some() {
            if matches!(key, K::Escape | K::Enter | K::NumpadEnter) {
                self.resource_error = None;
            }
            return Vec::new();
        }
        if self.page() == Page::Packs
            && let Some(p) = &mut self.pack_screen
        {
            let m = crate::screen_draw::Measure(&atlas.font);
            match key {
                // ConfirmScreen.keyPressed: Escape answers no.
                K::Escape if p.confirming() => {
                    p.cancel_confirm(&m);
                    return Vec::new();
                }
                K::Backspace => {
                    p.erase(&m, 1);
                    return Vec::new();
                }
                _ => {}
            }
        }
        if self.page() == Page::Mods
            && let Some(p) = &mut self.mods_screen
        {
            let m = crate::screen_draw::Measure(&atlas.font);
            match key {
                K::Backspace => {
                    p.erase(&m, 1);
                    return Vec::new();
                }
                K::ArrowUp => {
                    p.step(-1);
                    return Vec::new();
                }
                K::ArrowDown => {
                    p.step(1);
                    return Vec::new();
                }
                _ => {}
            }
        }
        if let Some(action) = self.selected_pad_action.take() {
            // As on Key Binds, Escape unbinds; other keys cancel.
            if key == K::Escape {
                s.controller.bind(action, None);
            }
            return vec![Event::Changed];
        }
        if self.selected_key.is_some() {
            // KeyBindsScreen.keyPressed: Escape unbinds.
            if self.selected_key == Some(MOD_KEY)
                && matches!(
                    key,
                    K::ShiftLeft
                        | K::ShiftRight
                        | K::ControlLeft
                        | K::ControlRight
                        | K::AltLeft
                        | K::AltRight
                )
            {
                self.pending_mod_key = Some(key);
                return Vec::new();
            }
            let value = if key == K::Escape {
                "key.keyboard.unknown".to_owned()
            } else {
                keybinds::key_name(key)
                    .unwrap_or("key.keyboard.unknown")
                    .to_owned()
            };
            return self.bind_key(s, value);
        }
        if self.page() == Page::SplitProfiles && self.split_profiles.key(key, text) {
            return Vec::new();
        }
        if self.page() == Page::ControllerRadialName {
            return self.radial_name_key(s, key, text);
        }

        // The size it was drawn at; before the first draw, one its keys can lay out at.
        let gui = if self.size.0 > 0.0 && self.size.1 > 0.0 {
            self.size
        } else {
            MIN_KEY_GUI
        };
        self.shell_framebuffers();
        if let Some(shell) = self.shell_mut() {
            let outputs = shell.key(s, key, text, shift, gui);
            return self.shell_events(s, outputs);
        }

        if key == K::Escape {
            return self.back(s);
        }

        // Search field text entry when search box is focused
        if self.focus == Some(FocusId::Search) {
            if key != K::Tab && key != K::ArrowDown {
                let search = if self.page() == Page::Language {
                    &mut self.language_search
                } else {
                    &mut self.settings_search
                };
                if !search.key(key, false, shift)
                    && let Some(t) = text
                {
                    let typed: String = t.chars().filter(|c| !c.is_control()).collect();
                    if !typed.is_empty() {
                        search.insert_text(&typed);
                    }
                }
                self.scroll = 0.0;
                self.relayout();
                return Vec::new();
            }
        } else if self.page() == Page::Language && self.focus.is_none() {
            if !self.language_search.key(key, false, shift)
                && let Some(t) = text
            {
                let typed: String = t.chars().filter(|c| !c.is_control()).collect();
                if !typed.is_empty() {
                    self.language_search.insert_text(&typed);
                }
            }
            self.scroll = 0.0;
            self.relayout();
        } else if self.settings_search_active() && self.focus.is_none() {
            if !self.settings_search.key(key, false, shift)
                && let Some(t) = text
            {
                let typed: String = t.chars().filter(|c| !c.is_control()).collect();
                if !typed.is_empty() {
                    self.settings_search.insert_text(&typed);
                }
            }
            self.scroll = 0.0;
            self.relayout();
        }

        // Gather focusables on the current page
        let layout = self.built_layout(s, self.size.0, self.size.1);
        let (widgets, top, bottom, content) = layout.parts();
        let has_search = (self.page() == Page::Language) || self.settings_search_active();
        let mut focusables: Vec<(FocusId, [f32; 4], bool)> = Vec::new();
        if has_search {
            let search = if self.page() == Page::Language {
                &self.language_search
            } else {
                &self.settings_search
            };
            focusables.push((
                FocusId::Search,
                [search.x, search.y, search.width, search.height],
                false,
            ));
        }
        for (i, wd) in widgets.iter().enumerate() {
            match &wd.kind {
                WKind::Button { active: true, .. }
                | WKind::Slider { .. }
                | WKind::LanguageRow { .. } => {
                    focusables.push((focus_id(i, wd), [wd.x, wd.y, wd.w, wd.h], wd.list));
                }
                _ => {}
            }
        }
        if focusables.is_empty() {
            return Vec::new();
        }

        // 1. Tab / Shift+Tab navigation
        if key == K::Tab {
            let at = self
                .focus
                .as_ref()
                .and_then(|fid| focusables.iter().position(|(id, ..)| id == fid));
            let next_idx = match at {
                None => {
                    if shift {
                        focusables.len() - 1
                    } else {
                        0
                    }
                }
                Some(p) => {
                    if shift {
                        (p + focusables.len() - 1) % focusables.len()
                    } else {
                        (p + 1) % focusables.len()
                    }
                }
            };
            let (next_id, rect, in_list) = &focusables[next_idx];
            self.focus = Some(next_id.clone());
            self.focus_from_keyboard = true;
            self.can_change_value = matches!(next_id, FocusId::Slider(_));
            if *in_list {
                if rect[1] < top {
                    self.scroll -= (top - rect[1]) + 4.0;
                } else if rect[1] + rect[3] > bottom {
                    self.scroll += (rect[1] + rect[3] - bottom) + 4.0;
                }
                self.clamp_scroll(top, bottom, content);
            }
            return Vec::new();
        }

        // 2. Left / Right Arrow keys
        if matches!(key, K::ArrowLeft | K::ArrowRight) {
            let forward = key == K::ArrowRight;
            // If focused on a slider and in can_change_value mode: adjust the slider value!
            if let Some(FocusId::Slider(opt)) = self.focus
                && self.can_change_value
            {
                let width = widgets
                    .iter()
                    .find(|wd| matches!(wd.kind, WKind::Slider { opt: o, .. } if o == opt))
                    .map_or(150.0, |wd| wd.w);
                return self.step_slider_option(s, opt, forward, width);
            }
            // Otherwise, navigate horizontally using 2D focus stepping
            if let Some(fid) = &self.focus {
                if let Some(cur_pos) = focusables.iter().position(|(id, ..)| id == fid) {
                    let from = focusables[cur_pos].1;
                    let rects: Vec<[f32; 4]> = focusables.iter().map(|f| f.1).collect();
                    let dir = if forward {
                        gamepad::Button::DPadRight
                    } else {
                        gamepad::Button::DPadLeft
                    };
                    if let Some(next_idx) = gamepad::focus_step_rect(from, dir, &rects) {
                        let (next_id, rect, in_list) = &focusables[next_idx];
                        self.focus = Some(next_id.clone());
                        self.focus_from_keyboard = true;
                        self.can_change_value = false;
                        if *in_list {
                            if rect[1] < top {
                                self.scroll -= (top - rect[1]) + 4.0;
                            } else if rect[1] + rect[3] > bottom {
                                self.scroll += (rect[1] + rect[3] - bottom) + 4.0;
                            }
                            self.clamp_scroll(top, bottom, content);
                        }
                    }
                }
            } else {
                self.focus = Some(focusables[0].0.clone());
                self.focus_from_keyboard = true;
                self.can_change_value = false;
            }
            return Vec::new();
        }

        // 3. Up / Down Arrow keys
        if matches!(key, K::ArrowUp | K::ArrowDown) {
            let down = key == K::ArrowDown;
            if let Some(fid) = &self.focus {
                if let Some(cur_pos) = focusables.iter().position(|(id, ..)| id == fid) {
                    let from = focusables[cur_pos].1;
                    let rects: Vec<[f32; 4]> = focusables.iter().map(|f| f.1).collect();
                    let dir = if down {
                        gamepad::Button::DPadDown
                    } else {
                        gamepad::Button::DPadUp
                    };
                    let step = gamepad::focus_step_strict(from, dir, &rects)
                        .or_else(|| gamepad::focus_step_vague(from, dir, &rects));
                    if let Some(next_idx) = step {
                        let (next_id, rect, in_list) = &focusables[next_idx];
                        self.focus = Some(next_id.clone());
                        self.focus_from_keyboard = true;
                        self.can_change_value = false;
                        if *in_list {
                            if rect[1] < top {
                                self.scroll -= (top - rect[1]) + 4.0;
                            } else if rect[1] + rect[3] > bottom {
                                self.scroll += (rect[1] + rect[3] - bottom) + 4.0;
                            }
                            self.clamp_scroll(top, bottom, content);
                        }
                    } else {
                        // Off the edge of visible rows: scroll the list
                        self.scroll += if down { 18.0 } else { -18.0 };
                        self.clamp_scroll(top, bottom, content);
                    }
                }
            } else {
                self.focus = Some(focusables[0].0.clone());
                self.focus_from_keyboard = true;
                self.can_change_value = false;
            }
            return Vec::new();
        }

        // 4. Enter / Space selection keys
        if matches!(key, K::Enter | K::NumpadEnter | K::Space) {
            match &self.focus {
                Some(FocusId::Slider(_)) => {
                    self.can_change_value = !self.can_change_value;
                    return Vec::new();
                }
                Some(FocusId::Cycle(opt)) => {
                    let mut events = vec![Event::ButtonClick];
                    events.extend(self.cycle_option(s, *opt, shift, self.framebuffer));
                    return events;
                }
                Some(FocusId::Act(act)) => {
                    let mut events = vec![Event::ButtonClick];
                    let act = act.clone();
                    match act {
                        Act::Push(p) => {
                            self.focus = None;
                            self.focus_from_keyboard = false;
                            self.can_change_value = false;
                            if p == Page::ShaderPacks {
                                // The pack list, and the picked pack's own settings with it.
                                events.push(Event::ListShaderPacks);
                                s.shader_options = None;
                                events.push(Event::OpenShaderOptions);
                            }
                            if p == Page::Music {
                                events.push(Event::ListMusicTracks);
                            }
                            if p == Page::ModBrowser {
                                events.push(Event::BrowseMods);
                            }
                            self.push(p);
                        }
                        Act::Done => {
                            self.focus = None;
                            self.focus_from_keyboard = false;
                            self.can_change_value = false;
                            events.extend(self.back(s));
                        }
                        Act::Cycle(opt) => {
                            events.extend(self.cycle_option(s, opt, shift, self.framebuffer));
                        }
                        Act::Extra(item) => {
                            if let Some(name) = item.key_name() {
                                self.selected_key = Some(name);
                            } else {
                                item.cycle(&mut s.extras, shift);
                                events.push(Event::Changed);
                            }
                        }
                        Act::PlayMusicTrack(event, track) => {
                            events.push(Event::PlayMusicTrack(event, track.clone()));
                        }
                        Act::MusicTrackOn(track, was_on) => {
                            s.extras
                                .music_off
                                .retain(|off| off.as_str() != track.as_str());
                            if was_on {
                                s.extras.music_off.push(track.clone());
                            }
                            events.push(Event::Changed);
                        }
                        Act::Key(name) => self.selected_key = Some(name),
                        Act::ResetKey(name) => {
                            s.keys.set(name, KeyBinds::default_of(name));
                            events.push(Event::Changed);
                        }
                        Act::ResetKeys => {
                            s.keys.reset_all();
                            s.mods.keys.clear();
                            events.push(Event::Changed);
                        }
                        Act::ModKey(i, k) => {
                            self.selected_key = Some(MOD_KEY);
                            self.selected_mod_key = Some((i, k));
                            self.pending_mod_key = None;
                        }
                        Act::ResetModKey(i, k) => {
                            if let Some((id, mapping)) = mod_mapping(s, i, k) {
                                s.mods.reset_binding(&id, &mapping);
                                events.push(Event::Changed);
                            }
                        }
                        Act::PageRow(p, r) => events.extend(self.page_act(p, r, shift)),
                        Act::NativeSetting(i, j) => {
                            let setting = s
                                .mods
                                .installed
                                .get_mut(i)
                                .and_then(|m| m.native.as_mut())
                                .and_then(|page| page.settings.get_mut(j));
                            if let Some(setting) = setting {
                                setting.value.click();
                                events.push(Event::NativeSetting(i, j));
                            }
                        }
                        Act::ModProject(i) => {
                            if let Some(listing) = self.mod_browser.listings.get(i) {
                                self.mod_project = listing.id.clone();
                                self.push(Page::ModProject);
                            }
                        }
                        Act::InstallMod => {
                            self.mod_browser.message = None;
                            events.push(Event::InstallMod(self.mod_project.clone()));
                        }
                        Act::OpenModPage => {
                            if let Some(l) = self.mod_browser.listing(&self.mod_project) {
                                events.push(Event::OpenModPage(l.page.clone()));
                            }
                        }
                        Act::PvpPreset => {
                            crate::pvp::apply_preset(s);
                            events.push(Event::Changed);
                        }
                        Act::HudEditor => events.push(Event::HudEditor),
                        Act::AccessibilityHelp => events.push(Event::AccessibilityHelp),
                        Act::Credits => events.push(Event::Credits),
                        Act::CopyAgentDetails => events.push(Event::CopyAgentDetails),
                        Act::ToggleRecording => events.push(Event::ToggleRecording),
                        Act::ExportRepro => events.push(Event::ExportRepro),
                        Act::None => {}
                    }
                    return events;
                }
                Some(FocusId::Lang(code)) => {
                    // `LanguageSelectionList.Entry.keyPressed`: the selection
                    // key (`isSelection`: Enter, Space, keypad Enter) selects
                    // the row and finishes the screen at once (`onDone`), so
                    // the language applies on that one press.
                    let mut events = vec![Event::ButtonClick];
                    self.language_choice = Some(code.clone());
                    events.extend(self.back(s));
                    return events;
                }
                _ => {}
            }
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests;

fn preset_from_slider(v: f32) -> usize {
    let v = if v >= 1.0 { 0.99999 } else { v };
    ((v * 4.0).floor() as usize).min(3)
}
#[cfg(test)]
#[path = "options_ui_graphics_tests.rs"]
mod graphics_tests;
#[path = "options_ui_pages.rs"]
mod pages;
