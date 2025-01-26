use axum::{extract::Query, response::Html, Extension};
use std::{collections::HashMap, sync::Arc};

use crate::{
    consts,
    indexer::{Indexer, SearchResult},
};

pub async fn index_handler(
    Query(params): Query<HashMap<String, String>>,
    Extension(index): Extension<Arc<Indexer>>,
) -> Html<String> {
    let query = params
        .iter()
        .filter_map(|(k, v)| {
            if k == "q" {
                Some(v.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("");

    // Search index to get the results.
    let results_html = if query.is_empty() {
        String::new()
    } else {
        let results: String = index
            .search(&query, consts::RESULTS_PER_QUERY)
            .map(|results| {
                results
                    .into_iter()
                    .map(
                        |SearchResult {
                             title,
                             url,
                             snippet,
                        }| {
                            let snippet = snippet.to_html();
                            format!("<div><h3>{url}</h3><p>{snippet}</p></div>")
                        },
                    )
                    .collect::<Vec<_>>()
                    .join("")
            })
            // TODO: log, don't show full error to user
            .unwrap_or_else(|e| format!("ERROR: Could not get search results for '{query}': {e}"));

        format!(
            "<div class='results'>
                <h3>Search Results:</h3>
                <ul>
                    {results}
                </ul>
            </div>"
        )
    };
    let title = consts::NAME;

    Html(format!(
        r##"
<!DOCTYPE html>
<html>
<head>
    <title>Mini Search Engine</title>
    <style>
        body {{ font-family: Arial, sans-serif; margin: 40px; }}
        .input-form {{ margin-bottom: 20px; }}
        .results {{ background: #f0f0f0; padding: 20px; border-radius: 5px; }}
    </style>
</head>

<body>
    <h1>{title}</h1>

    <div class="input-form">
        <input type="text" id="searchInput" autofocus>
        <button id="searchButton" onclick="search()">Search</button>
    </div>

    {results_html}

    <script defer>
        function search() {{
            const input = document.getElementById('searchInput').value;
            if (!input.trim()) return;

            const currentUrl = new URL(window.location.href);
            currentUrl.searchParams.set("q", input);
            window.location.href = currentUrl.toString();
        }}

        // Make sure this code gets executed after the DOM is loaded.
        document.querySelector("#searchInput").addEventListener("keyup", event => {{
            if (event.key !== "Enter") return;
            document.querySelector("#searchButton").click();
            event.preventDefault();
        }});
    </script>
</body>
</html>
"##
    ))
}
