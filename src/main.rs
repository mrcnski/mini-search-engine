fn main() {
    println!("Hello, world!");
}
use axum::{
    extract::Query,
    response::Html,
    routing::get,
    Router,
};
use serde::Deserialize;
use std::collections::HashMap;

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(handler));

    println!("Server starting on http://localhost:3000");
    axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn handler(Query(params): Query<HashMap<String, String>>) -> Html<String> {
    let params_html = if params.is_empty() {
        String::new()
    } else {
        format!("<div class='params'><h3>Query Parameters:</h3><ul>{}</ul></div>",
            params.iter()
                .map(|(k, v)| format!("<li>{}: {}</li>", k, v))
                .collect::<Vec<_>>()
                .join("")
        )
    };

    Html(format!(r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>Query Parameters Demo</title>
            <style>
                body {{ font-family: Arial, sans-serif; margin: 40px; }}
                .input-form {{ margin-bottom: 20px; }}
                .params {{ background: #f0f0f0; padding: 20px; border-radius: 5px; }}
            </style>
        </head>
        <body>
            <h1>Query Parameters Demo</h1>
            <div class="input-form">
                <input type="text" id="paramInput" placeholder="Enter key=value">
                <button onclick="addParam()">Add Parameter</button>
            </div>
            {}
            <script>
                function addParam() {{
                    const input = document.getElementById('paramInput').value;
                    if (input.includes('=')) {{
                        const [key, value] = input.split('=');
                        const currentUrl = new URL(window.location.href);
                        currentUrl.searchParams.set(key, value);
                        window.location.href = currentUrl.toString();
                    }}
                }}
            </script>
        </body>
        </html>
    "#, params_html))
}
