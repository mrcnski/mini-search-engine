# Mini Search Engine

## Goals

This is a mini search engine specifically designed for searching programming
documentation using existing technologies.

The priorities are **search relevance** and **latency**. The maximum allowable
search latency is 50ms.

### Crawling

Our goals for crawling were as follows:

- [ ] The crawler should stay within the subdomain/domain/path restrictions
      found [here](./domains).
- [ ] No more than 10,000 pages should be indexed from any single domain.
- [ ] Proxy use is not needed. (Though see ["Proxy Use"](#proxy-use) below for
      how we could potentially incorporate a proxy.)

The Rust crate [spider](https://github.com/spider-rs/spider) was chosen to
perform crawling. We leveraged the Rust ecosystem because we were already using
a Rust library for indexing (see ["Indexing"](#indexing) below), and `spider` is
the most well-known Rust-language crawler. The
[benchmarks](https://github.com/spider-rs/spider/blob/main/benches/BENCHMARKS.md)
showed that `spider` was capable of running on a single machine with good
enough performance for our requirements, mentioned above.

#### Proxy Use

`spider` also has the advantage of supporting proxy rotation, in case we wanted
to leverage proxies in the future.

TODO

### Indexing

Because latency was a priority,
[tantivy](https://github.com/quickwit-oss/tantivy) was chosen to provide
indexing. tantivy is capable of single-digit latencies. This also allowed us to
easily opt-in to the Rust language and ecosystem and write a high-performance
solution.

tantivy is optimized for single-node systems and may be the less scalable
option. This was deemed acceptable due to the limited scope of our use-case (a
maximum of ~1.6 million pages total). Solutions such as
[Vespa](https://github.com/vespa-engine/vespa) are more suitable for larger,
distributed systems, and provide additional features that were not needed here.

## Installation/Deployment

TODO

## Technical Details

Here we describe the technical details such as architecture, challenges faced,
and the ranking strategy.

### Architecture

TODO

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
    handle this case with `spider`'s API, so I resorted to forking it and making
    a hacky modification to the source code to handle this case. I added some
    unit tests and also confirmed that my "fix" worked with manual testing.
  - **Javascript-rendered pages:** Some pages, like `forum.crystal-lang.org`,
    seem to use Javascript to render the page. Since we are not using
    Javascript rendering, the crawler is not able to crawl these pages.
  - **Iframes:** Sites like `cran.r-project.org/` appear to render the whole
    site in an iframe. I did not attempt to handle this case.
  - **Broken domains:** some domains like `modernizr.com` are no longer
    functional. There is nothing we can do in this situation.
- **Headless crawling:** `spider` has a `chrome` feature which enables headless
  crawling with Chrome. Although we didn't need to render Javascript, I thought
  it could help with some of the other issues discussed above. `spider` also has
  a `smart` feature which first attempts HTTP crawling, only switching to
  headless browsing when needed. Unfortunately, I ran into [numerous
  issues](https://github.com/orgs/spider-rs/discussions/261) with these
  features, and the one time that it worked, `spider` still did not handle some
  of the edge cases discussed above. Since there was not much gain, I did not
  investigate headless crawling more as it uses significantly more system
  resources.

#### Performance

- Initially I was getting 50ms times for a small test index, which was
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

- `tantivy` does not provide a way to boost specific terms for more relevant
  results. I fixed this by manually editing the query string to add a boost
  factor to specific terms. See "Ranking Strategy" below.
- I tried setting some fields to fuzzy (matching with Levenshtein distance) to
  catch user typos or similar terms. This unfortunately broke snippet
  generation. I wrote [an issue about
  it](https://github.com/quickwit-oss/tantivy/issues/2576).
- `tantivy` does not expose its search parameters, e.g. [for
  BM25](https://github.com/quickwit-oss/tantivy/issues/2195), so I was not able
  to experiment with these. In the future, it may be possible to submit a change
  request or fork the project to allow customizing these parameters. However, I
  assume that the default parameters were chosen with a lot of care and are good
  for most documents.

### Ranking Strategy

`tantivy` already returns the results of a search ranked by relevancy,
incorporating industry-standard techniques such as BM25 scoring. However, this
project tries to improve search relevancy with some simple strategies:

- Stemming is used for more relevant results. This has performance implications,
  so only basic stemming is used.
  - Lemmatization was considered, but not used because of the additional
    overhead at query time. In the future, the actual performance hit could be
    measured and this choice reconsidered.
- In addition to querying page text, we also query over titles and descriptions
  and prioritize ("boost") these fields.
- Initially, the search "clojure for loop" would return a result for Crystal
  ranked higher than results for Clojure. `tantivy` doesn't seem to have an easy
  API for boosting specific terms, but I hacked together a solution that adds a
  boost factor to each term (e.g. `"clojure for loop"` -> `"clojure^2.5 for
  loop"`). I asked AI to generate a list of terms to boost from the list of
  domains.
- I tried to enable fuzzy search for some fields but ran into a bug - see
  "Challenges Faced" above.


## Possible Future Directions

TODO
