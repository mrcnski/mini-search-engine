# Mini Search Engine

## Goals

This is a mini search engine specifically designed for searching programming
documentation using existing technologies.

The priorities are **search relevance** and **latency**.

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

TODO

### Ranking Strategy

TODO

## Possible Future Directions

TODO
