# Scripture Burrito schema, WA's fork

`schema/` from <https://github.com/Bible-Translation-Tools/scripture-burrito>,
branch `wycliffe-associates`, at `b8872ee` ("Add timestamp back as a required
field in identification", 2026-04-07). MIT, see `LICENSE`.

Tests only: `bento/tests/schema.rs` validates every file `create_bible` and
`edit` write, and the fixtures, against `metadata.schema.json`. Nothing at
runtime reads it. To move the pin, replace the folder and the commit above.
