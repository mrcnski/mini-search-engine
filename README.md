# Mini Search Engine

## Goals

This is a mini search engine specifically designed for searching programming
documentation using existing technologies.

The priorities are **search relevance** and **latency**. The maximum allowable
search latency is **50ms**.

### Crawling

Our goals for crawling were as follows:

1. The crawler should stay within the subdomain/domain/path restrictions found
   [here](./domains).
1. No more than 10,000 pages should be indexed from any single domain.
1. Proxy use is not needed. (Though see ["Proxy Use"](#proxy-use) below for how
   we could potentially incorporate a proxy.)

The Rust crate [spider](https://github.com/spider-rs/spider) was chosen to
perform crawling. We leveraged the Rust ecosystem because we were already using
a Rust library for indexing (see ["Indexing"](#indexing) below), and `spider` is
the most well-known Rust-language crawler. The
[benchmarks](https://github.com/spider-rs/spider/blob/main/benches/BENCHMARKS.md)
showed that `spider` was capable of running on a single machine with good
enough performance for our requirements, mentioned above.

#### Proxy Use

`spider` also has the advantage of supporting proxy rotation, in case we wanted
to leverage proxies in the future. This could be done by setting up some proxy
servers (or paying for such a service) and using spider's `.with_proxies`
method.

### Indexing

Because latency was a priority,
[tantivy](https://github.com/quickwit-oss/tantivy) was chosen to provide
indexing. tantivy is capable of single-digit latencies. This also allowed us to
easily opt-in to the Rust language and ecosystem and write a high-performance
and robust solution.

tantivy is optimized for single-node systems and may be the less scalable
option. This was deemed acceptable due to the limited scope of our use-case (a
maximum of ~1.6 million pages total). Solutions such as
[Vespa](https://github.com/vespa-engine/vespa) are more suitable for larger,
distributed systems, and provide additional features that were not needed here.

## Installation/Deployment

### Native (recommended for best performance)

- Clone the repository:

```
git clone --recurse-submodules https://github.com/mrcnski/mini-search-engine
```

- Install any missing dependencies (e.g. `apt-get update && apt-get install -y build-essential pkg-config libssl-dev`).
- Make sure [Rust is installed](https://www.rust-lang.org/tools/install).
- `cargo run --release`

### Docker

- Clone the repository:

```
git clone --recurse-submodules https://github.com/mrcnski/mini-search-engine
```

- `docker compose up`

## Technical Details

Here we describe the technical details such as **architecture**, **challenges
faced**, and the **ranking strategy**.

### Architecture

1. If requested, the search engine will first **crawl** all desired domains.
   - We start multiple tasks (capped at 16) for each domain in our list. By
     crawling many domains at once we hopefully spread out the workload and
     avoid excessively spamming any single domain. This way we minimize the
     chance of getting blocked/rate-limited.
   - For each domain we crawl up to 16 pages simultaneously, sending the pages
     to the background indexing job.
   - We do not do any further crawling after the initial crawl has completed as
     this was deemed out of scope, but see "Possible Future Directions" below.
2. The **indexer** runs in the background, indexing any pages that it receives
   from the crawler.
   - The indexer will periodically commit at a regular interval if new documents
     have been added. In general, indexing throughput is higher with more time
     between commits, but there is a greater chance of losing data.
   - The index schema includes `title`, `description`, and `body`, and all three
     are considered (with decreasing priority) when searching the index.
   - The indexer also keeps a persistent embedded database containing statistics
     for all domains. These stats are used for the `/stats` page.
   - Once we start up the server (see below), the indexer handles any search
     queries sent to it by the server and returns a list of most relevant
     results. (See "Ranking Strategy".)
3. Once the initial crawl has completed, we start up the **server**.
   - The server could just as well start before the crawl has completed, with a
     message that the full corpus is not yet available. I simply chose to leave
     this out of scope. See "Possible Future Directions" below.

### Ranking Strategy

`tantivy` already returns the results of a search ranked by relevancy,
incorporating industry-standard techniques such as BM25 scoring. However, this
project tries to improve search relevancy further with some simple strategies:

- **Stemming** is used for more relevant results. This has performance
  implications, so only basic stemming is used.
  - **Lemmatization** was considered, but not used because of the additional
    overhead at query time. In the future, the actual performance hit could be
    measured and this choice reconsidered.
- In addition to querying page text, we also query over titles and descriptions
  and prioritize (**"boost"**) these fields.
- Initially, the search "clojure for loop" would return a result for Crystal
  ranked higher than results for Clojure. `tantivy` doesn't seem to have an easy
  API for boosting specific terms, but I hacked together a solution that adds a
  **boost factor** to each term (e.g. `"clojure for loop"` -> `"clojure^2.5 for
  loop"`). I asked AI to generate a list of terms to boost from the list of
  domains.
- I tried to enable **fuzzy search** for some fields but ran into a bug - see
  "Challenges Faced" below.

### Challenges Faced

#### Crawling

- I was confused about the usage of the `spider` crawler due to unclear
  documentation. Fortunately, I was able to [ask the developer and get a quick
  response](https://github.com/spider-rs/spider/issues/253) without diving into
  the source code.
- After implementing the `stats` page, I was able to see that some domains were
  not being crawled:
  - **Invalid HTTPS certificates:** I enabled this by adding the
    `.with_danger_accept_invalid_certs(true)` setting. While this is a
    security concern, we are prioritizing relevance over security. :)
  - **Meta refresh redirects:** Some domains had a homepage with HTML content
    like: `<meta http-equiv="refresh" content="0; url=en/latest/contents.html"
    />`. In the browser this is detected as an HTML redirect (as opposed to an
    HTTP redirect). Since we are only making HTTP requests (not running a
    headless browser), `spider` was [not handling this
    case](https://github.com/spider-rs/spider/issues/255). I was not able to
    make this work with `spider`'s API, so I forked it and making a hacky
    modification to the source code to handle this case. I added some unit tests
    and also confirmed that my "fix" worked with manual testing.
  - **Javascript-rendered pages:** Some pages, like `forum.crystal-lang.org`,
    seem to use Javascript to render the page. Since we are not using
    Javascript rendering, the crawler is not able to crawl these pages.
  - **Iframes:** Sites like `cran.r-project.org/` appear to render the whole
    site in an iframe. I did not attempt to handle this case.
  - **Broken domains:** Some domains like `modernizr.com` are no longer
    functional. There is nothing we can do in this situation.
- **Headless crawling:** `spider` has a `chrome` feature which enables headless
  crawling with Chrome. Although we didn't need to render Javascript, I thought
  it could help with some of the other issues discussed above. For example, I
  assumed that the browser would detect and handle things like meta refresh
  redirects. `spider` also has a `smart` feature which always attempts to crawl
  a site using only HTTP, only switching to headless browsing when needed.
  Unfortunately, I ran into [numerous
  issues](https://github.com/orgs/spider-rs/discussions/261) with these
  features, and the one time that it worked, `spider` still did not handle some
  of the edge cases discussed above. Since there was not much gain, I did not
  investigate headless crawling more as it uses significantly more system
  resources.

#### Indexing

- When I first attempted to index the full corpus I started seeing "Too many
  open files" errors. Instead of raising the open file limit, I simply put a cap
  on the amount of domains that can be crawled simultaneously (there was already
  a cap on how many tasks we spawn per domain). I was happy to see how useful
  the error messages are -- which had immediately pointed me to the problem --
  and how robust the application is in the face of unexpected errors!

#### Performance

- Initially I was getting almost 50ms times for a small test index, which was
  dangerously close to the maximum latency allowed. I played around with the
  `FAST` flag on tantivy schema fields. Setting it on the `title` and
  `description` fields boosted the query performance by almost 2x!
  Interestingly, setting it on the `body` field brought the performance back
  down to original levels.
- I continued to see performance issues, so I played around with a number of
  strategies. I enabled a token limit on fast fields and enabled Rust compiler
  optimizations. The biggest impact seemed to come from replacing the system
  allocator with `jemalloc`, resulting in another **huge** performance boost!
  (On my Mac machine, at least.) (I then switched to `mimalloc` because
  `jemalloc` was segfaulting on my Mac.)
- I was still seeing fairly high latencies. After measuring the different
  stages of `search`, I eventually realized that snippets were being generated
  from large `<script>` elements. I added additional filtering for these
  elements before indexing the text of a page.
- I also decided not to *containerize* the application to avoid any possible
  performance hit (even though the penalty is usually very small).

#### Ranking strategy

- `tantivy` does not provide a way to **boost** specific terms for more relevant
  results. I fixed this by manually editing the query string to add a boost
  factor to specific terms. See "Ranking Strategy" above.
- I tried setting some fields to **fuzzy** (matching with Levenshtein distance)
  to catch user typos or similar terms. This unfortunately broke snippet
  generation. I wrote [an issue about
  it](https://github.com/quickwit-oss/tantivy/issues/2576).
- `tantivy` does not expose its **search parameters**, e.g. [for
  BM25](https://github.com/quickwit-oss/tantivy/issues/2195), so I was not able
  to experiment with these. In the future, it may be possible to submit a change
  request or fork the project to allow customizing these parameters. However, I
  assume that the default parameters were chosen with considerable care and are
  suitable for most documents.

## Possible Future Directions

Possible future changes/enhancements include:

- [ ] Allow the webpage to be functional before the initial indexing has completed.
- [ ] Live crawling and updating of the index on new or updated pages.
- [ ] The application could be scaled to multiple nodes if desired.
- [ ] We could add proxy rotation to raise crawling throughput.
- [X] ~~Support for quoted search terms, and other special syntax.~~ tantivy
      supports this out-of-the-box.
- [ ] Support for lemmatization for even more relevant results. (But this should
      be measured as it may affect query performance.)
- [ ] Limit page title and description lengths before indexing, like Google does
      (to avoid keyword stuffing, since we boost these fields).
