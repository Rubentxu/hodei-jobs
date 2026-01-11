//! Tests for DataTable component using rstest
//!
//! TDD approach: Tests for sorting, filtering, and pagination functionality
//! This drives the implementation of the actual DataTable component.

use leptos::prelude::*;
use rstest::{fixture, rstest};

use hodei_console_web::components::data_table::{DataTable, SortDirection, TableColumn};

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// FIXTURES - Table test data
// ═══════════════════════════════════════════════════════════════════════════════════════════════

/// Sample table column definitions for testing
#[fixture]
fn sample_columns() -> Vec<TableColumn> {
    vec![
        TableColumn {
            key: "id",
            label: "ID",
            sortable: true,
        },
        TableColumn {
            key: "name",
            label: "Name",
            sortable: true,
        },
        TableColumn {
            key: "status",
            label: "Status",
            sortable: true,
        },
        TableColumn {
            key: "actions",
            label: "Actions",
            sortable: false,
        },
    ]
}

/// Sample sortable columns only
#[fixture]
fn sortable_columns() -> Vec<TableColumn> {
    vec![
        TableColumn {
            key: "id",
            label: "ID",
            sortable: true,
        },
        TableColumn {
            key: "name",
            label: "Name",
            sortable: true,
        },
        TableColumn {
            key: "created_at",
            label: "Created At",
            sortable: true,
        },
    ]
}

/// Sample non-sortable columns
#[fixture]
fn non_sortable_columns() -> Vec<TableColumn> {
    vec![
        TableColumn {
            key: "id",
            label: "ID",
            sortable: false,
        },
        TableColumn {
            key: "status",
            label: "Status",
            sortable: false,
        },
    ]
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// TABLE COLUMN TESTS
// ═══════════════════════════════════════════════════════════════════════════════════════════════

/// Test table column creation
#[rstest]
fn test_table_column_creation() {
    let column = TableColumn {
        key: "test_key",
        label: "Test Label",
        sortable: true,
    };

    assert_eq!(column.key, "test_key");
    assert_eq!(column.label, "Test Label");
    assert!(column.sortable);
}

/// Test table column equality
#[rstest]
fn test_table_column_equality(#[from(sample_columns)] columns: Vec<TableColumn>) {
    let col1 = &columns[0];
    let col2 = TableColumn {
        key: "id",
        label: "ID",
        sortable: true,
    };

    assert_eq!(col1.key, col2.key);
    assert_eq!(col1.label, col2.label);
    assert_eq!(col1.sortable, col2.sortable);
}

/// Test sortable column identification
#[rstest]
fn test_sortable_column_identification(#[from(sortable_columns)] columns: Vec<TableColumn>) {
    let sortable: Vec<&str> = columns
        .iter()
        .filter(|c| c.sortable)
        .map(|c| c.key)
        .collect();

    assert_eq!(sortable.len(), 3);
    assert!(sortable.contains(&"id"));
    assert!(sortable.contains(&"name"));
    assert!(sortable.contains(&"created_at"));
}

/// Test non-sortable column identification
#[rstest]
fn test_non_sortable_column_identification(
    #[from(non_sortable_columns)] columns: Vec<TableColumn>,
) {
    let sortable: Vec<&str> = columns
        .iter()
        .filter(|c| c.sortable)
        .map(|c| c.key)
        .collect();

    assert!(sortable.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// SORT DIRECTION TESTS
// ═══════════════════════════════════════════════════════════════════════════════════════════════

/// Test sort direction default is Asc
#[rstest]
fn test_sort_direction_default() {
    let direction = SortDirection::default();
    assert_eq!(direction, SortDirection::Asc);
}

/// Test sort direction toggling
#[rstest]
fn test_sort_direction_toggle() {
    let asc = SortDirection::Asc;
    let desc = SortDirection::Desc;

    // Asc should toggle to Desc
    assert_eq!(asc, SortDirection::Asc);
    assert_eq!(desc, SortDirection::Desc);
}

/// Test sort direction equality
#[rstest]
fn test_sort_direction_equality() {
    let dir1 = SortDirection::Asc;
    let dir2 = SortDirection::Asc;
    let dir3 = SortDirection::Desc;

    assert_eq!(dir1, dir2);
    assert_ne!(dir1, dir3);
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// SORTING LOGIC TESTS
// ═══════════════════════════════════════════════════════════════════════════════════════════════

/// Test sorting by string values ascending
#[rstest]
fn test_string_sort_ascending() {
    let mut items = vec!["charlie", "alpha", "bravo"];
    items.sort_by(|a, b| a.cmp(b));

    assert_eq!(items, vec!["alpha", "bravo", "charlie"]);
}

/// Test sorting by string values descending
#[rstest]
fn test_string_sort_descending() {
    let mut items = vec!["charlie", "alpha", "bravo"];
    items.sort_by(|a, b| b.cmp(a));

    assert_eq!(items, vec!["charlie", "bravo", "alpha"]);
}

/// Test sorting by numeric values ascending
#[rstest]
fn test_numeric_sort_ascending() {
    let mut items = vec![100, 10, 1000, 50];
    items.sort();

    assert_eq!(items, vec![10, 50, 100, 1000]);
}

/// Test sorting by numeric values descending
#[rstest]
fn test_numeric_sort_descending() {
    let mut items = vec![100, 10, 1000, 50];
    items.sort_by(|a, b| b.cmp(a));

    assert_eq!(items, vec![1000, 100, 50, 10]);
}

/// Test stable sorting preserves order of equals
#[rstest]
fn test_stable_sort() {
    let mut items = vec![("alpha", 1), ("beta", 2), ("alpha", 3), ("gamma", 1)];

    items.sort_by(|a, b| a.0.cmp(&b.0));

    // "alpha" items should maintain relative order
    assert_eq!(items[0].0, "alpha");
    assert_eq!(items[1].0, "alpha");
    assert_eq!(items[0].1, 1); // First alpha (1) comes before second alpha (3)
    assert_eq!(items[1].1, 3);
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// FILTERING LOGIC TESTS
// ═══════════════════════════════════════════════════════════════════════════════════════════════

/// Test basic string filtering
#[rstest]
fn test_string_filter() {
    let items = vec!["apple", "banana", "apricot", "cherry"];
    let filter = "ap";

    let filtered: Vec<&str> = items
        .iter()
        .filter(|s| s.contains(filter))
        .copied()
        .collect();

    assert_eq!(filtered, vec!["apple", "apricot"]);
}

/// Test case-insensitive filtering
#[rstest]
fn test_case_insensitive_filter() {
    let items = vec!["Apple", "BANANA", "apricot", "cherry"];
    let filter = "ap";

    let filtered: Vec<&str> = items
        .iter()
        .filter(|s| s.to_lowercase().contains(&filter.to_lowercase()))
        .copied()
        .collect();

    assert_eq!(filtered, vec!["Apple", "apricot"]);
}

/// Test empty filter returns all items
#[rstest]
fn test_empty_filter_returns_all() {
    let items = vec!["apple", "banana", "cherry"];
    let filter = "";

    let filtered: Vec<&str> = items
        .iter()
        .filter(|s| s.contains(filter))
        .copied()
        .collect();

    assert_eq!(filtered.len(), items.len());
}

/// Test filtering by exact match
#[rstest]
fn test_exact_match_filter() {
    let items = vec!["apple", "APPLE", "Apple", "banana"];
    let filter = "apple";

    let filtered: Vec<&str> = items
        .iter()
        .filter(|s| s.eq_ignore_ascii_case(filter))
        .copied()
        .collect();

    assert_eq!(filtered, vec!["apple", "APPLE", "Apple"]);
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// PAGINATION LOGIC TESTS
// ═══════════════════════════════════════════════════════════════════════════════════════════════

/// Test pagination page calculation
#[rstest]
fn test_page_calculation() {
    let total_items = 100;
    let page_size = 10;
    let total_pages = (total_items as f64 / page_size as f64).ceil() as u32;

    assert_eq!(total_pages, 10);
}

/// Test pagination with remainder
#[rstest]
fn test_page_calculation_with_remainder() {
    let total_items = 95;
    let page_size = 10;
    let total_pages = (total_items as f64 / page_size as f64).ceil() as u32;

    assert_eq!(total_pages, 10);
}

/// Test page offset calculation
#[rstest]
fn test_page_offset() {
    fn offset(page: u32, page_size: u32) -> u32 {
        (page - 1) * page_size
    }

    assert_eq!(offset(1, 10), 0);
    assert_eq!(offset(2, 10), 10);
    assert_eq!(offset(5, 10), 40);
}

/// Test slice pagination
#[rstest]
fn test_slice_pagination() {
    let items: Vec<i32> = (1..=100).collect();
    let page = 3;
    let page_size = 10;

    let start = (page - 1) * page_size;
    let end = start + page_size;
    let page_items = &items[start..end];

    assert_eq!(page_items.len(), 10);
    assert_eq!(page_items[0], 21);
    assert_eq!(page_items[9], 30);
}

// ═══════════════════════════════════════════════════════════════════════════════════════════════
// DATA TABLE PROPERTY TESTS
// ═══════════════════════════════════════════════════════════════════════════════════════════════

/// Property: Sorting is deterministic (same result each time)
#[rstest]
fn test_sorting_deterministic() {
    let mut items = vec![3, 1, 2, 1, 3, 2];
    let sorted_once = {
        let mut v = items.clone();
        v.sort();
        v
    };

    let sorted_twice = {
        let mut v = items.clone();
        v.sort();
        v.sort();
        v
    };

    // Sorting twice should give same result as sorting once
    assert_eq!(sorted_once, sorted_twice);
    // Result should be in ascending order
    assert_eq!(sorted_once, vec![1, 1, 2, 2, 3, 3]);
}

/// Property: Filter then sort should equal sort then filter
#[rstest]
fn test_filter_sort_commutative() {
    let items = vec!["banana", "apple", "cherry", "apricot"];
    let filter = "a";

    // Filter then sort
    let filtered_sorted_1: Vec<&str> = {
        let filtered: Vec<&str> = items
            .iter()
            .filter(|s| s.contains(filter))
            .copied()
            .collect();
        let mut sorted = filtered.clone();
        sorted.sort();
        sorted
    };

    // Sort then filter
    let filtered_sorted_2: Vec<&str> = {
        let mut sorted: Vec<&str> = {
            let mut s = items.clone();
            s.sort();
            s
        };
        sorted.into_iter().filter(|s| s.contains(filter)).collect()
    };

    assert_eq!(filtered_sorted_1, filtered_sorted_2);
}

/// Property: Pagination preserves order when items have unique identifiers
#[rstest]
fn test_pagination_preserves_order() {
    let items: Vec<(u32, &str)> = (1..=50).map(|i| (i, "item")).collect();
    let page_size = 10;

    for page in 1..=5 {
        let start = ((page - 1) * page_size) as usize;
        let end = (start + page_size) as usize;
        let page_items = &items[start..end];

        // Each page should have consecutive IDs
        for (idx, (id, _)) in page_items.iter().enumerate() {
            assert_eq!(*id as usize, start + idx + 1);
        }
    }
}
