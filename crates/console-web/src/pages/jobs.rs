//! Jobs page - List and manage jobs

use leptos::prelude::*;

#[component]
pub fn Jobs() -> impl IntoView {
    view! {
        <div class="jobs-page">
            <h1>"Jobs"</h1>
            <div class="controls">
                <input type="text" placeholder="Search jobs..." />
            </div>
            <table class="jobs-table">
                <thead>
                    <tr>
                        <th>"ID"</th>
                        <th>"Name"</th>
                        <th>"Status"</th>
                        <th>"Actions"</th>
                    </tr>
                </thead>
                <tbody>
                    <tr>
                        <td>"-"</td>
                        <td>"-"</td>
                        <td>"-"</td>
                        <td>
                            <button>"View"</button>
                            <button>"Cancel"</button>
                        </td>
                    </tr>
                </tbody>
            </table>
        </div>
    }
}
