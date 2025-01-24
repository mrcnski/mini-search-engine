use axum::{extract::Query, response::Html};
use std::collections::HashMap;

use crate::consts;

pub async fn index_handler(Query(params): Query<HashMap<String, String>>) -> Html<String> {
    let params_html = if params.is_empty() {
        String::new()
    } else {
        let results = params
            .iter()
            .filter_map(|(k, v)| {
                if k == "q" {
                    Some(format!("<li>{v}</li>"))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("");

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

    {params_html}

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
