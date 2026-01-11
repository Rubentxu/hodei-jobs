//! Theme management with dark mode support
//!
//! Provides theme state management, local storage persistence,
//! and CSS class toggling for dark/light mode.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Theme preference stored in localStorage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum Theme {
    #[serde(rename = "light")]
    Light,
    #[serde(rename = "dark")]
    Dark,
    #[serde(rename = "system")]
    System,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::System
    }
}

impl Theme {
    /// Check if dark mode is active
    #[must_use]
    pub fn is_dark(&self) -> bool {
        match self {
            Theme::Light => false,
            Theme::Dark => true,
            Theme::System => system_prefers_dark(),
        }
    }

    /// Get next theme in cycle: System -> Light -> Dark -> System
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Theme::System => Theme::Light,
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::System,
        }
    }

    /// Get display name
    #[must_use]
    pub fn display_name(&self) -> &'static str {
        match self {
            Theme::Light => "Light",
            Theme::Dark => "Dark",
            Theme::System => "System",
        }
    }

    /// Get icon name
    #[must_use]
    pub fn icon(&self) -> &'static str {
        match self {
            Theme::Light => "sun",
            Theme::Dark => "moon",
            Theme::System => "monitor",
        }
    }
}

/// System preference detection (only works in browser)
#[cfg(feature = "client")]
fn system_prefers_dark() -> bool {
    use wasm_bindgen::prelude::*;
    use web_sys::window;

    window()
        .and_then(|w| w.match_media("(prefers-color-scheme: dark)").ok())
        .and_then(|m| m.map(|m| m.matches()))
        .unwrap_or(false)
}

/// SSR fallback
#[cfg(not(feature = "client"))]
fn system_prefers_dark() -> bool {
    false
}

/// Global theme state key
const THEME_CONTEXT_KEY: &str = "hodei-theme-context";

/// Get or create theme signal
#[cfg(feature = "client")]
pub fn use_theme() -> RwSignal<Theme> {
    if let Some(ctx) = use_context::<RwSignal<Theme>>() {
        ctx
    } else {
        let theme = RwSignal::new(Theme::System);
        provide_context(theme);
        theme
    }
}

#[cfg(not(feature = "client"))]
pub fn use_theme() -> RwSignal<Theme> {
    if let Some(ctx) = use_context::<RwSignal<Theme>>() {
        ctx
    } else {
        let theme = RwSignal::new(Theme::System);
        provide_context(theme);
        theme
    }
}

/// Theme context for components
#[derive(Clone)]
pub struct ThemeContext {
    /// Current theme signal
    pub theme: RwSignal<Theme>,
    /// Set theme and persist
    pub set_theme: Callback<Theme>,
    /// Toggle to next theme
    pub toggle: Callback<()>,
    /// CSS class for dark mode
    pub is_dark: Signal<bool>,
}

#[cfg(feature = "client")]
impl ThemeContext {
    /// Create new theme context
    pub fn new() -> Self {
        let theme = use_theme();
        let set_theme = Callback::new(move |new_theme: Theme| {
            theme.set(new_theme);
            persist_theme(new_theme);
            apply_theme(new_theme);
        });
        let toggle = Callback::new(move |_| {
            let next = theme.get().next();
            theme.set(next);
            persist_theme(next);
            apply_theme(next);
        });
        let is_dark = Memo::new(move |_| theme.get().is_dark()).read_only();

        // Initialize from localStorage on client
        if let Some(saved) = load_saved_theme() {
            theme.set(saved);
            apply_theme(saved);
        } else {
            // Apply system preference on first load
            apply_theme(theme.get());
        }

        ThemeContext {
            theme,
            set_theme,
            toggle,
            is_dark,
        }
    }
}

#[cfg(not(feature = "client"))]
impl ThemeContext {
    pub fn new() -> Self {
        let theme = RwSignal::new(Theme::System);
        provide_context(theme);
        let set_theme = Callback::new(move |_: Theme| {});
        let toggle = Callback::new(|_| {});
        let is_dark = Signal::derive(|| false);
        ThemeContext {
            theme,
            set_theme,
            toggle,
            is_dark,
        }
    }
}

/// Persist theme to localStorage
#[cfg(feature = "client")]
fn persist_theme(theme: Theme) {
    use wasm_bindgen::prelude::*;
    use web_sys::window;

    let json = serde_json::to_string(&theme).ok();
    if let Some(json) = json {
        if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
            let _ = storage.set_item("hodei-theme", &json);
        }
    }
}

#[cfg(not(feature = "client"))]
fn persist_theme(_: Theme) {}

/// Load saved theme from localStorage
#[cfg(feature = "client")]
fn load_saved_theme() -> Option<Theme> {
    use wasm_bindgen::prelude::*;
    use web_sys::window;

    window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item("hodei-theme").ok().flatten())
        .and_then(|json| serde_json::from_str(&json).ok())
}

#[cfg(not(feature = "client"))]
fn load_saved_theme() -> Option<Theme> {
    None
}

/// Apply theme by toggling CSS class on document
#[cfg(feature = "client")]
fn apply_theme(theme: Theme) {
    use wasm_bindgen::prelude::*;
    use web_sys::window;

    if let Some(document) = window().and_then(|w| w.document()) {
        if let Some(html) = document.document_element() {
            if theme.is_dark() {
                html.class_list().add_1("dark").ok();
            } else {
                html.class_list().remove_1("dark").ok();
            }
        }
    }
}

#[cfg(not(feature = "client"))]
fn apply_theme(_: Theme) {}

/// Hook to use theme context in components
#[cfg(feature = "client")]
pub fn use_theme_context() -> ThemeContext {
    ThemeContext::new()
}

#[cfg(not(feature = "client"))]
pub fn use_theme_context() -> ThemeContext {
    ThemeContext::new()
}

/// Initialize theme CSS on page load (runs automatically)
#[cfg(feature = "client")]
#[wasm_bindgen(start)]
pub fn init_theme() {
    // Apply saved theme on page load
    if let Some(saved) = load_saved_theme() {
        apply_theme(saved);
    }
}
