// i18n.rs - Wielojęzyczność (internationalization)
use crate::types::Language;

#[allow(dead_code)]
pub struct TextResources {
    // Menu
    pub file_menu: String,
    pub new_project: String,
    pub open_project: String,
    pub save_project: String,
    pub exit: String,
    pub settings_menu: String,
    pub help_menu: String,
    pub about: String,
    // Timeline
    pub timeline: String,
    pub add_clip: String,
    pub remove_clip: String,
    pub split_clip: String,
    pub mark_in: String,
    pub mark_out: String,
    // Tools
    pub tool_hand: String,
    pub tool_scissors: String,
    // Status / Errors
    pub status_ready: String,
    pub status_project_loaded: String,
    pub status_project_saved: String,
    pub err_clip_boundary: String,
    pub err_no_clip_cursor: String,
    // Loading
    pub loading_thumbnails: String,
    pub loading_waveform: String,
    pub loading_language: String,
    // Settings
    pub settings_window_title: String,
    pub language_label: String,
    pub render_button: String,
}

impl TextResources {
    pub fn new(lang: Language) -> Self {
        match lang {
            Language::En => Self {
                file_menu: "File".into(),
                new_project: "New Project".into(),
                open_project: "Open Project".into(),
                save_project: "Save Project".into(),
                exit: "Exit".into(),
                settings_menu: "Settings".into(),
                help_menu: "Help".into(),
                about: "About".into(),
                timeline: "Timeline".into(),
                add_clip: "Add Clip".into(),
                remove_clip: "Remove Clip".into(),
                split_clip: "Split Clip".into(),
                mark_in: "Mark In".into(),
                mark_out: "Mark Out".into(),
                tool_hand: "Hand (A)".into(),
                tool_scissors: "Scissors (B)".into(),
                status_ready: "Ready".into(),
                status_project_loaded: "Project loaded".into(),
                status_project_saved: "Project saved".into(),
                err_clip_boundary: "Cannot cut at clip boundary".into(),
                err_no_clip_cursor: "No clip under cursor".into(),
                loading_thumbnails: "Loading thumbnails...".into(),
                loading_waveform: "Loading waveform...".into(),
                loading_language: "Switching language...".into(),
                settings_window_title: "Settings".into(),
                language_label: "Language".into(),
                render_button: "Render".into(),
            },
            Language::Pl => Self {
                file_menu: "Plik".into(),
                new_project: "Nowy projekt".into(),
                open_project: "Otwórz projekt".into(),
                save_project: "Zapisz projekt".into(),
                exit: "Wyjdź".into(),
                settings_menu: "Ustawienia".into(),
                help_menu: "Pomoc".into(),
                about: "O programie".into(),
                timeline: "Oś czasu".into(),
                add_clip: "Dodaj klip".into(),
                remove_clip: "Usuń klip".into(),
                split_clip: "Podziel klip".into(),
                mark_in: "Początek".into(),
                mark_out: "Koniec".into(),
                tool_hand: "Rączka (A)".into(),
                tool_scissors: "Nożyczki (B)".into(),
                status_ready: "Gotowy".into(),
                status_project_loaded: "Projekt wczytany".into(),
                status_project_saved: "Projekt zapisany".into(),
                err_clip_boundary: "Nie można ciąć na granicy klipu".into(),
                err_no_clip_cursor: "Brak klipu pod kursorem".into(),
                loading_thumbnails: "Ładowanie miniatur...".into(),
                loading_waveform: "Ładowanie fali dźwięku...".into(),
                loading_language: "Zmiana języka...".into(),
                settings_window_title: "Ustawienia".into(),
                language_label: "Język".into(),
                render_button: "Renderuj".into(),
            },
        }
    }
}
