# OSM fixtures

- `small_extract.json`: hand-written, not from OpenStreetMap.
- `real_broomfield_co.json`: a real Overpass extract of about 0.3 km², south 39.900, west −105.095, north 39.905, east
  −105.088, fetched 2026-11-03. © OpenStreetMap contributors, under the
  [Open Database License](https://www.openstreetmap.org/copyright). Tests use it to hold the importer to real data. Any plant
  designed on it is invented.
- `real_broomfield_large_co.json`: a real Overpass extract of about 2.2 km² around it, south 39.895, west −105.10, north 39.910, east
  −105.08, fetched 2026-11-03 and reduced to what the importer reads. Same attribution and license. It holds amp placement to a road
  network generated layouts don't produce.
- `rules_extract.json`: hand-written, one case for each importer rule.
- `any_map/`: three cuts of real Overpass extracts fetched for task 15 (2026-11-03), each a box about 1 km across with every way that
  enters it kept whole, and reduced to the tags `design::osm` reads. © OpenStreetMap contributors, under the
  [Open Database License](https://www.openstreetmap.org/copyright). Task 25's test holds the version 2 designer to no issue on any
  node it builds on each; any plant designed on them is invented.
  - `seattle_ballard_block_wa.json`: south 47.6680, west −122.3895, north 47.6735, east −122.3835, Seattle's Ballard, with a building
    of 268 units and three more of 31 to 111.
  - `albany_center_ny.json`: south 42.6553, west −73.7923, north 42.6643, east −73.7803, Albany, NY.
  - `brattleboro_center_vt.json`: south 42.8472, west −72.5684, north 42.8562, east −72.5564, Brattleboro, VT.
