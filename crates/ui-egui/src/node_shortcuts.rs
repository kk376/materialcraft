//! Node creation shortcuts add a library entry under the pointer in the material graph

use craft_keymap::{Chord, Key, Mods, Platform};
use egui::{Event, InputState};

/// The library categories whose entries can have a creation shortcut
pub(crate) const CATEGORIES: [&str; 2] = ["Atomic nodes", "Graph items"];

/// The chord on a fresh key press, using the event's own modifiers
pub(crate) fn event_chord(event: &Event, platform: Platform) -> Option<Chord> {
    let Event::Key { key, pressed: true, repeat: false, modifiers, .. } = event else { return None };

    Some(Chord::new(Mods::from_egui(modifiers, platform), Key::from_egui(*key)?))
}

/// Preserve the stored syntax where Ctrl means Command on macOS
pub(crate) fn text(chord: Chord) -> String {
    chord.portable().replace("Ctrl+", "Control+").replace("Cmd+", "Ctrl+")
}

/// The entry whose shortcut was pressed this frame, if any
pub(crate) fn pressed<'a>(shortcuts: &'a std::collections::BTreeMap<String, String>, input: &InputState, platform: Platform) -> Option<&'a str> {
    if shortcuts.is_empty() {
        return None;
    }

    input.events.iter().filter_map(|event| event_chord(event, platform)).find_map(|pressed| {
        shortcuts.iter().find_map(|(entry, text)| {
            let mut assigned = crate::menus::parse_shortcut(text)?;
            assigned.mods = assigned.mods.effective(platform);
            (assigned == pressed).then_some(entry.as_str())
        })
    })
}

/// The app command or canvas action already bound to the chord
pub(crate) fn taken_by(chord: Chord, platform: Platform) -> Option<&'static str> {
    if chord.mods.is_none() {
        match chord.key {
            Key::Space | Key::Tab => return Some("Open node search"),
            Key::F => return Some("Frame graph"),
            Key::Escape | Key::Enter => return Some("Close or confirm"),
            _ => {}
        }
    }

    crate::menus::shortcut_table().into_iter().find_map(|(id, text)| {
        let mut assigned = crate::menus::parse_shortcut(text)?;
        assigned.mods = assigned.mods.effective(platform);
        (assigned == chord).then_some(id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shortcut_reads_back_as_the_keys_that_made_it() {
        let plain = Chord::new(Mods::CMD, Key::B);
        let both = Chord::new(Mods { cmd: true, ctrl: true, shift: true, ..Mods::NONE }, Key::B);

        assert_eq!(text(plain), "Ctrl+B");
        assert_eq!(crate::menus::parse_shortcut(&text(both)), Some(both));
        assert_eq!(crate::menus::parse_shortcut("Cmd+B"), Some(plain));
    }

    #[test]
    fn a_shortcut_the_app_already_uses_is_named() {
        assert_eq!(taken_by(Chord::new(Mods::CMD, Key::S), Platform::Windows), Some("file.save"));
        assert_eq!(taken_by(Chord::plain(Key::F), Platform::Windows), Some("Frame graph"));
        assert_eq!(taken_by(Chord::new(Mods::CTRL, Key::S), Platform::Mac), None);
    }

    #[test]
    fn key_events_keep_their_own_modifiers_and_ignore_repeat() {
        let key = egui::Key::B;
        let event = |modifiers, repeat| Event::Key { key, physical_key: None, pressed: true, repeat, modifiers };
        let plain = event(egui::Modifiers::NONE, false);
        let alt = event(egui::Modifiers::ALT, false);
        let raw = egui::RawInput { events: vec![plain, Event::ModifiersChanged(egui::Modifiers::ALT), alt], ..Default::default() };
        let shortcuts = [("node:blur".to_owned(), "Alt+B".to_owned()), ("node:levels".to_owned(), "B".to_owned())].into();

        egui::Context::default()
            .run_ui(raw, |ui| {
                assert_eq!(ui.input(|input| pressed(&shortcuts, input, Platform::Windows)), Some("node:levels"));
            })
            .drop_without_applying_deltas();

        assert!(event_chord(&event(egui::Modifiers::ALT, true), Platform::Windows).is_none());
    }
}
