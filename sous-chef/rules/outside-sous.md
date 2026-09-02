# Explicitly outside Sous Chef

Onion or the editor owns these, because each requires structure, layout, or an
external authority rather than content convention:

- marker validity and unsupported markers;
- repeated, missing, bridged, or out-of-order verse markers;
- book/chapter title and label consistency;
- expected verse counts from a versification authority;
- section-title placement and other layout-shaped checks;
- metadata comparison against an external publication catalog.

One consequence reaches [hygiene](hygiene.md): through the Onion producer a
lone backslash never becomes a Sous finding, because Onion lexes it as a
marker — well-formed or not — and masks it out. Only a `\\` pair arrives as
content. A vref producer keeps every backslash as content.
