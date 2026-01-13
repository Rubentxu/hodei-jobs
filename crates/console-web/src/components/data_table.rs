//! Data Table Component
//!
//! A sortable, paginated data table component.

use leptos::prelude::*;

/// Sort direction
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SortDirection {
    #[default]
    Asc,
    Desc,
}

/// Column definition for DataTable
#[derive(Clone, Debug)]
pub struct TableColumn {
    pub key: &'static str,
    pub label: &'static str,
    pub sortable: bool,
}

/// Wrapper for table rows
#[derive(Clone, Debug, Default)]
pub struct TableRowWrapper {
    pub id: String,
    pub values: std::collections::HashMap<String, String>,
}

impl TableRowWrapper {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            values: std::collections::HashMap::new(),
        }
    }

    pub fn with_value(mut self, key: &str, value: &str) -> Self {
        self.values.insert(key.to_string(), value.to_string());
        self
    }

    pub fn get_id(&self) -> &str {
        &self.id
    }
    pub fn get_value(&self, key: &str) -> String {
        self.values.get(key).cloned().unwrap_or_default()
    }
}

/// DataTable component - simplified version
#[component]
pub fn DataTable(
    /// Column definitions
    columns: Vec<TableColumn>,
    /// Data items
    data: Memo<Vec<TableRowWrapper>>,
    /// Loading state
    #[prop(default = false)]
    loading: bool,
    /// Page size
    #[prop(default = 10)]
    page_size: usize,
    /// Current page
    current_page: ReadSignal<usize>,
    /// Set page callback
    set_page: Action<(), ()>,
) -> impl IntoView {
    let total_pages = Signal::derive(move || {
        let total = data.get().len();
        if page_size == 0 {
            1
        } else {
            (total + page_size - 1) / page_size
        }
    });

    let tp = total_pages.get();
    let cp = current_page.get();
    let offset = (cp.saturating_sub(1)) * page_size;
    let page_data: Vec<TableRowWrapper> = data
        .get()
        .into_iter()
        .skip(offset)
        .take(page_size)
        .collect();

    let (tbody_html, page_info) = if loading {
        (
            r#"<tr><td colspan=""#.to_string()
                + &columns.len().to_string()
                + r#"" class="px-4 py-8 text-center"><span class="material-symbols-outlined animate-spin">sync</span><span class="ml-2 text-gray-500">Loading...</span></td></tr>"#,
            format!("Page {} of {}", cp, tp.max(1)),
        )
    } else if page_data.is_empty() {
        (
            r#"<tr><td colspan=""#.to_string()
                + &columns.len().to_string()
                + r#"" class="px-4 py-8 text-center text-gray-500">No data available</td></tr>"#,
            format!("Page {} of {}", cp, tp.max(1)),
        )
    } else {
        let rows: String = page_data.iter().map(|row| {
            format!(r#"<tr class="hover:bg-gray-50 transition-colors"><td class="px-4 py-3 text-sm">{}</td></tr>"#, row.get_id())
        }).collect();
        (rows, format!("Page {} of {}", cp, tp.max(1)))
    };

    let prev_disabled = cp == 1;
    let next_disabled = cp >= tp;

    view! {
        <div class="data-table">
            <div class="overflow-x-auto border border-gray-200 rounded-lg">
                <table class="w-full border-collapse">
                    <thead class="bg-gray-50">
                        <tr>
                            {columns.iter().map(|col| {
                                view! {
                                    <th class="px-4 py-3 text-left text-xs font-semibold text-gray-600 uppercase">
                                        {col.label}
                                    </th>
                                }
                            }).collect::<Vec<_>>()}
                        </tr>
                    </thead>
                    <tbody class="bg-white divide-y divide-gray-200" inner_html=tbody_html />
                </table>
            </div>
            <div class="mt-4 flex items-center justify-between">
                <span class="text-sm text-gray-600">
                    {page_info}
                </span>
                <div class="flex gap-2">
                    <button
                        class="p-1 rounded hover:bg-gray-100 disabled:opacity-50"
                        disabled=prev_disabled
                        on:click=move |_: leptos::ev::MouseEvent| {
                            if !prev_disabled { set_page.dispatch(()); }
                        }
                    >
                        <span class="material-symbols-outlined">"chevron_left"</span>
                    </button>
                    <button
                        class="p-1 rounded hover:bg-gray-100 disabled:opacity-50"
                        disabled=next_disabled
                        on:click=move |_: leptos::ev::MouseEvent| {
                            if !next_disabled { set_page.dispatch(()); }
                        }
                    >
                        <span class="material-symbols-outlined">"chevron_right"</span>
                    </button>
                </div>
            </div>
        </div>
    }
}
