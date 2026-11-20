# Vendored for the page

MapLibre GL JS 4.7.1 (BSD-3-Clause, `MAPLIBRE-LICENSE.txt`), from `https://unpkg.com/maplibre-gl@4.7.1/dist/`. It's
built into the server binary and served at `/vendor/`, so the page loads where unpkg can't be reached, such as an
air-gapped NOC. The basemap, Nominatim and Overpass still need the internet, and only when you use a real place.

sha256 at download:

- `maplibre-gl.js`: `be9633c4d870e26fb37f1cfe5c5a77181667114003ea16207ac7850d8da8add1`
- `maplibre-gl.css`: `576b085fdd9487a65a19215328c1e086c07ce5bf6da09b666b3806d3d008dae9`

To upgrade, replace both files, update these hashes, and change the version in this file.
