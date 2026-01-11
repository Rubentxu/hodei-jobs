//! Theme Toggle Button Component
//!
//! A button that cycles through theme options (Light -> Dark -> System).

use crate::components::theme::{Theme, use_theme_context};
use leptos::prelude::*;

/// Theme toggle component - cycles through Light/Dark/System
#[component]
pub fn ThemeToggle() -> impl IntoView {
    let theme_ctx = use_theme_context();

    view! {
        <button
            class="theme-toggle"
            title="Toggle theme"
            on:click=move |_| {
                theme_ctx.toggle.run(());
            }
        >
            <span class="theme-icon">
                {move || match theme_ctx.theme.get() {
                    Theme::Light => "☀️",
                    Theme::Dark => "🌙",
                    Theme::System => "💻",
                }}
            </span>
            <span class="theme-label">
                {move || theme_ctx.theme.get().display_name()}
            </span>
        </button>
    }
}

/// Theme toggle dropdown for full theme selection
#[component]
pub fn ThemeDropdown() -> impl IntoView {
    let theme_ctx = use_theme_context();
    let is_open = RwSignal::new(false);

    let toggle_open = move |_| is_open.update(|v| *v = !*v);
    let close_dropdown = move |_| is_open.set(false);

    let dropdown_content = move || {
        view! {
            <div class="theme-dropdown-menu">
                <button
                    class="theme-option"
                    class:active={move || theme_ctx.theme.get() == Theme::Light}
                    on:click=move |_| {
                        theme_ctx.set_theme.run(Theme::Light);
                        close_dropdown(());
                    }
                >
                    "☀️ Light"
                </button>
                <button
                    class="theme-option"
                    class:active={move || theme_ctx.theme.get() == Theme::Dark}
                    on:click=move |_| {
                        theme_ctx.set_theme.run(Theme::Dark);
                        close_dropdown(());
                    }
                >
                    "🌙 Dark"
                </button>
                <button
                    class="theme-option"
                    class:active={move || theme_ctx.theme.get() == Theme::System}
                    on:click=move |_| {
                        theme_ctx.set_theme.run(Theme::System);
                        close_dropdown(());
                    }
                >
                    "💻 System"
                </button>
            </div>
        }
    };

    view! {
        <div class="theme-dropdown">
            <button
                class="theme-dropdown-toggle"
                on:click=toggle_open
            >
                <span class="current-theme">
                    {move || match theme_ctx.theme.get() {
                        Theme::Light => "☀️ Light",
                        Theme::Dark => "🌙 Dark",
                        Theme::System => "💻 System",
                    }}
                </span>
            </button>

            <Show when=move || is_open.get()>
                {dropdown_content()}
            </Show>
        </div>
    }
}
