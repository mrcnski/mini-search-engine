/// Boost factor applied to tech terms in queries
pub const TECH_TERM_BOOST: f32 = 2.5;

/// Tech terms to boost within queries. Generated from `domains` using AI.
pub const TECH_TERMS_TO_BOOST: &[&str] = &[
    "actix",
    "angular",
    "ansible",
    "astro",
    "aws",
    "azure",
    "bash",
    "c",
    "c-lang",
    "clang",
    "c++",
    "cpp",
    "c++-lang",
    "cpplang",
    "clojure",
    "clojurescript",
    "coffee",
    "coffeescript",
    "crystal",
    "crystal-lang",
    "css",
    "dart",
    "dart-lang",
    "dartlang",
    "deno",
    "django",
    "docker",
    "dotnet",
    "elixir",
    "elixir-lang",
    "elixirlang",
    "ember",
    "erlang",
    "erlang-lang",
    "erlanglang",
    "fastapi",
    "flask",
    "flutter",
    "gatsby",
    "git",
    "github",
    "gitlab",
    "go",
    "golang",
    "gradle",
    "graphql",
    "groovy",
    "haskell",
    "html",
    "java",
    "java-lang",
    "javalang",
    "javascript",
    "jenkins",
    "jquery",
    "js",
    "json",
    "jupyter",
    "kafka",
    "kotlin",
    "kotlin-lang",
    "kubernetes",
    "laravel",
    "linux",
    "lisp",
    "lua",
    "lua-lang",
    "lualang",
    "maven",
    "mongodb",
    "mysql",
    "nextjs",
    "nginx",
    "nim",
    "nim-lang",
    "nimlang",
    "nodejs",
    "nosql",
    "npm",
    "nuxt",
    "ocaml",
    "perl",
    "perl-lang",
    "php",
    "php-lang",
    "postgres",
    "postgresql",
    "python",
    "python-lang",
    "pythonlang",
    "r",
    "rails",
    "react",
    "reactjs",
    "redis",
    "redux",
    "ruby",
    "ruby-lang",
    "rubylang",
    "rust",
    "rust-lang",
    "rustlang",
    "scala",
    "scala-lang",
    "scalalang",
    "scheme",
    "shell",
    "shell-lang",
    "shelllang",
    "spring",
    "sql",
    "sqlite",
    "svelte",
    "swift",
    "swift-lang",
    "swiftlang",
    "terraform",
    "typescript",
    "ts",
    "vim",
    "vue",
    "vuejs",
    "webpack",
    "xml",
    "yaml",
    "zig",
];
