Sefer sharp edges so far:


# Come back to ideas - Sous
2. Census: the machinery exists, and it is not a judge question. The CLI's report page already renders exactly this inventory, per glyph, convicted or not: totals, per-book spread, both neighbour tables, placement, cluster shapes. It reads the merged book aggregates natively, the same rows the Expediter keeps resident. So the change is one new door, census(), returning a wire buffer of per-glyph rows straight from the resident totals, no judging involved, plus a reader for it. The "every site of a glyph" enumeration is already there: it is find with the glyph as the needle, over the same projection, with the same hit shape. The per-glyph total falls out of the census row and removes Sefer's Rarity special case. Agreed it needs a short spec, mainly to pin the row layout, because it is a new wire code and a new reader table. Not a rush; half a day when you want it.


# Sefer agent said:
