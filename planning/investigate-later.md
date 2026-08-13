# Investigate later

Simplification leads that look real but haven't been checked. Each one's
GOAL IS NET DELETION — if the investigation ends with more code than it
started, the answer was no. Nothing here is committed to; nothing here
blocks anything.

## Does the scanner need `ws_after_name` at all? (2026-08-13)

**The suspicion:** the delimiter-fold rule may be nothing more than
*openers and milestones fold, end markers don't* — pure SHAPE, zero table.

**Why it looks that way now.** Step 4.6 collapsed the scanner's use of the
column to a single bit: `folds_delimiter` reads six `Ws` variants and
distinguishes exactly one of them (`SingleNewline`) from all the others.
Row 0 is special-cased to "fold" anyway. So of 151 rows the column changes
the scanner's answer for exactly one marker: `\b`.

**What to check, concretely:**

1. Is `\b` even a real counterexample? Its row says `SingleNewline`, so
   today `\b x` leaves the space as content. If it folded instead, does
   anything break? Partition holds either way (the run just rides the
   marker span). `\b` takes no content at all, so a following space is
   already anomalous — which means the question is really "who reports
   it", not "which token owns the byte".
2. If lint is the honest owner of that report, the scanner's read of the
   column is dead weight, and the deletions are: `folds_delimiter`'s row
   lookup, the `Ws` import in scanner.rs, **`Hot.folds` and its resolve
   step entirely**, and the fold branch in `fused_plain` that exists only
   to serve `\b`. That is a genuine net deletion across two files.
3. The column itself STAYS regardless — lint needs required-vs-optional
   for its delimiter-whitespace findings, so this is about deleting a
   scanner READ, not a table column. Do not confuse the two.
4. Check the same question for the codegen side: if the scanner stops
   reading it, does the packed field's bit width still earn its place, or
   does it become a lint-only column that could move out of the hot
   `u128`/row? (Bit budget is `BITS_USED` in generated.rs.)

**Do it when:** any time after 5B. It is independent of the attribute work
and would make `Hot` smaller, so ideally BEFORE any further `HotIdx`
surgery.

**Watch out for:** the `AtLeastOneWhitespace` rows (`\v`) — HS *or*
newline. The scanner already never folds a newline (`ws_run_end` eats
space/tab only), so those rows are not counterexamples; they just make the
enum look more load-bearing to the scanner than it is.
