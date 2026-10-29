These are unchanged historical independent-review observations. The Rust probe
retains its original immutable reviewer-snapshot module path, so it is not a
standalone test against the current checkout. The current executable regression
is in src/source_rich_value.rs. The failing assertion accepted Binary64(-0)
as an Array length. Genuine Node assignment and defineProperty both normalized
that property to positive zero. The correction rejects impossible external
graphs and preserves canonical source +0 graphs and all76 previous wire frames.
