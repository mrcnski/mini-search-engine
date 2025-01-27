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

- I encountered some mild confusion around the usage of the `spider` crawler due
  to unclear docs. Luckily I was able to [ask the developer and get a quick
  response](https://github.com/spider-rs/spider/issues/253) without diving into
  the source code.
- Making the queries performant.
  - Initially I was getting 50ms times for a small test index, which was
    dangerously close to the maximum latency allowed. So I played around with
    the `FAST` flag on tantivy schema fields. Setting it on the `title` and
    `description` fields boosted the query performance by almost 2x!
    Interestingly, setting it on the `body` field brought the performance back
    down to original slow levels.
  - I continued to see performance issues, so I played around with a number of
    strategies. I enabled a token limit on fast fields and enabled Rust compiler
    optimizations. The biggest impact seemed to come from replacing the system
    allocator with `jemalloc`, resulting in another **huge** performance boost!
    (On my Mac machine, at least.)
  - I was still seeing fairly high latencies. After bugging them on Discord and
    measuring the different stages of `search`, I eventually realized that
    snippets were being generated from large `<script>` elements. I added
    additional filtering for these elements before indexing the text of a page.

TODO

### Ranking Strategy

TODO

## Possible Future Directions

TODO
