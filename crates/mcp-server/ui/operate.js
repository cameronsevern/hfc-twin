// The operations screen (task 28): the region as the stage, both worlds side by side, through the ops tools alone (invariant 8).
// The page only shows the worlds: every hazard, call, dispatch and repair is the world's own, drawn from its seed in the crate
// (invariants 4 and 5). A truck's smooth motion is display, interpolated between the times the world gives. All of it is
// simulated; the page says so once, up front (task 29).
(() => {
'use strict';
const START_HOUR = 6;
const WORLDS = ['reactive', 'twin'];
const WORLD_NAME = {reactive: 'From calls', twin: 'With the twin'};
const CREW = {Service: 'service tech', Line: 'line crew', Splicer: 'splicers'};
const SPEED_KEYS = {1: 1, 2: 5, 3: 15, 4: 'max'};
const DAY = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday'];
const reduceMotion = matchMedia('(prefers-reduced-motion: reduce)').matches;

const svg = (body, box = '0 0 32 32') => `<svg viewBox="${box}" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;
const ICON = {
  storm: svg('<path d="M9 20a6 6 0 1 1 2-11.6A7.5 7.5 0 0 1 25 11a5 5 0 0 1-1 9z"/><path d="M16 19l-3 5h5l-3 5"/>'),
  heat: svg('<path d="M13 6a3 3 0 0 1 6 0v12.5a6 6 0 1 1-6 0z"/><path d="M16 11v12"/><path d="M24 7h4M24 12h3"/>'),
  power: svg('<path d="M4 15L16 5l12 10"/><path d="M8 13v13h16V13"/><path d="M17 14l-3 5h4l-3 5"/><path d="M3 29L29 3"/>'),
  dig: svg('<path d="M3 27h26"/><path d="M6 22l6-12h7l3 7"/><path d="M19 10l6-5 3 4-5 4"/><path d="M8 27a3 3 0 0 1 6 0"/>'),
  car: svg('<path d="M4 21l3-8h14l4 8v5H4z"/><circle cx="10" cy="26" r="2.5"/><circle cx="20" cy="26" r="2.5"/><path d="M27 6l-2 5M30 10l-4 2"/>'),
  fiber: svg('<path d="M2 16h9"/><path d="M21 16h9"/><path d="M13 10l2 6-2 6"/><path d="M19 10l-2 6 2 6"/>'),
  truck: '<svg viewBox="0 0 24 18" aria-hidden="true"><path fill="currentColor" d="M1 3h13v10H1zM15 6h4.5L23 10v3h-8z"/><circle cx="6" cy="14.5" r="2.2" fill="currentColor" stroke="#0004"/><circle cx="18" cy="14.5" r="2.2" fill="currentColor" stroke="#0004"/></svg>',
  wrench: '<svg viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="M21 6.5a5.5 5.5 0 0 1-7.6 5.1L6 19a2.1 2.1 0 0 1-3-3l7.4-7.4A5.5 5.5 0 0 1 17.5 1l-3.3 3.3.9 3.6 3.6.9z"/></svg>',
  phone: '<svg viewBox="0 0 24 24" aria-hidden="true"><path fill="#fff" d="M6.6 10.8a15.1 15.1 0 0 0 6.6 6.6l2.2-2.2a1 1 0 0 1 1-.25 11.4 11.4 0 0 0 3.6.6 1 1 0 0 1 1 1V20a1 1 0 0 1-1 1A17 17 0 0 1 3 4a1 1 0 0 1 1-1h3.5a1 1 0 0 1 1 1c0 1.25.2 2.45.6 3.6a1 1 0 0 1-.25 1z"/></svg>',
  hub: '<svg viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="M2 10L12 3l10 7v11h-4v-8H6v8H2z"/><path fill="currentColor" d="M7 15h10v2H7zm0 3h10v2H7z"/></svg>',
  sun: '<circle cx="12" cy="12" r="4.5" fill="#f2b705"/><g stroke="#f2b705" stroke-width="2" stroke-linecap="round"><path d="M12 2v2.5M12 19.5V22M2 12h2.5M19.5 12H22M4.9 4.9l1.8 1.8M17.3 17.3l1.8 1.8M4.9 19.1l1.8-1.8M17.3 6.7l1.8-1.8"/></g>',
  moon: '<path fill="#9fb3d9" d="M15.5 3a9 9 0 1 0 5.5 15.6A8 8 0 0 1 15.5 3z"/>',
};

const TOOLS = [
  {id: 'storm', label: 'Storm', act: () => ({Storm: {hours: 6, wind_m_per_s: 25, rain_mm_per_hr: 20}}), what: 'A storm: 25 m/s wind and 20 mm/h of rain for 6 hours'},
  {id: 'heat', label: 'Heat wave', act: () => ({HeatWave: {hours: 8, delta_c: 12}}), what: 'A heat wave: 12 °C above normal for 8 hours'},
  {id: 'power', label: 'Power cut', placed: true, hint: 'Click the map where the power goes out: 600 ft around, for 3 hours.', act: at => ({PowerCut: {center: at, radius_ft: 600, hours: 3}}), what: 'The power goes out 600 ft around there for 3 hours'},
  {id: 'dig', label: 'Dig-in', placed: true, hint: 'Click the map near a buried cable: an excavator digs through the nearest one.', act: at => ({DigIn: {near: at}}), what: 'An excavator digs there, through the nearest buried cable if there is one'},
  {id: 'car', label: 'Car crash', placed: true, hint: 'Click the map near a pole or pedestal: a car hits the nearest one.', act: at => ({CarHit: {near: at}}), what: 'A car leaves the road there, into the nearest pole or pedestal'},
  {id: 'fiber', label: 'Fiber cut', act: () => 'FiberCut', what: 'A fiber cable is cut somewhere in the region'},
];

const SCORE_ROWS = [
  ['Customer-hours out', c => c.customer_minutes_out / 60],
  ['Homes out now', c => c.homes_out_now],
  ['Truck rolls', c => c.truck_rolls],
  ['Rolls that found nothing', c => c.no_trouble_found],
  ['Calls abandoned', c => c.calls_abandoned],
];
const avg = p => p[1] ? Math.round(p[0] / p[1]) : '–';
const ALL_ROWS = [
  ['Homes out now', c => c.homes_out_now],
  ['Customer-hours out', c => Math.round(c.customer_minutes_out / 60)],
  ['Calls', c => c.calls],
  ['Calls answered', c => c.calls_answered],
  ['Calls told of a known outage', c => c.calls_told_of_outage],
  ['Calls abandoned', c => c.calls_abandoned],
  ['Tickets', c => c.tickets],
  ['Truck rolls', c => c.truck_rolls],
  ['Repeat rolls', c => c.repeat_rolls],
  ['No trouble found', c => c.no_trouble_found],
  ['Faults fixed', c => c.fixes],
  ['Minutes, onset to first call', c => avg(c.onset_to_first_call)],
  ['Minutes, onset to first truck', c => avg(c.onset_to_dispatch)],
  ['Minutes, onset to fix', c => avg(c.onset_to_fix)],
];

const G = {
  open: false, started: false, layout: null, maps: {}, mapsReady: null, basemap: null,
  view: 'both', truth: false, armed: null, follow: null, inspect: null,
  playing: false, speed: 5, shownT: 0, pending: [], cur: null, inflight: false, since: 0, seriesSince: 0,
  series: {reactive: [], twin: []}, homeGeo: new Map(), homeNode: new Map(),
  status: {reactive: new Map(), twin: new Map()}, calls: {reactive: new Map(), twin: new Map()},
  trips: {reactive: new Map(), twin: new Map()}, rings: {reactive: [], twin: []}, watchDrives: true, slowed: false,
  trucks: {reactive: new Map(), twin: new Map()}, jobs: {reactive: new Map(), twin: new Map()}, truthPins: {reactive: new Map(), twin: new Map()},
  feed: [], feedKeys: new Set(), dayBase: null, lastFrame: 0, pace: [], announced: 0, routesAt: 0, opener: null,
};

// ---- time ----------------------------------------------------------------------------------------------------------
const clockOf = t => {
  const m = Math.floor(t / 60) + START_HOUR * 60;
  return `${DAY[Math.floor(m / 1440) % 7].slice(0, 3)} ${String(Math.floor(m / 60) % 24).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}`;
};
const hhmm = t => clockOf(t).slice(4);
const dayOf = t => Math.floor((t + START_HOUR * 3600) / 86400);
const newestT = () => G.pending.length ? G.pending[G.pending.length - 1].t_s : (G.cur ? G.cur.t_s : 0);

// One bad state must never stop the clock: an error is said once and the loop goes on.
let frameError = null;
function frame(now) {
  try { tick(now); } catch (e) {
    if (frameError !== e.message) { frameError = e.message; console.error(e); toast(`Something on this screen failed: ${esc(e.message)}. The worlds are unharmed.`, true); }
  }
  requestAnimationFrame(frame);
}
function tick(now) {
  const dt = Math.min(1, (now - (G.lastFrame || now)) / 1000);
  G.lastFrame = now;
  if (G.playing && G.cur) {
    const top = newestT();
    // Display pace only: while a truck is on the road in a world on screen, the clock slows so the drive can be watched.
    G.slowed = G.watchDrives && G.speed !== 'max' && G.speed > 1.2 && anyDriving();
    const rate = G.slowed ? 1.2 : G.speed;
    const want = G.speed === 'max' ? top : G.shownT + dt * rate * 60;
    G.shownT = Math.max(G.shownT, Math.min(want, top));
    G.pace.push([now, G.shownT, G.slowed]);
    while (G.pace.length && now - G.pace[0][0] > 3000) G.pace.shift();
  }
  while (G.pending.length && G.pending[0].t_s <= G.shownT) applyState(G.pending.shift());
  for (const w of WORLDS) {
    const q = G.rings[w];
    let n = 0;
    while (q.length && q[0].t_s <= G.shownT) { const c = q.shift(); if (n++ < 12 && shown(w)) ring(w, c.home); }
  }
  if (G.open) {
    pump();
    $('gClock').textContent = clockOf(G.shownT);
    moveTrucks(now);
    showPace();
  }
}
requestAnimationFrame(frame);

// Asks for the next stretch while this one plays, so the clock never waits on the sim unless the sim is slower than the speed.
async function pump() {
  if (!G.playing || G.inflight || !G.started) return;
  const max = G.speed === 'max';
  const chunk = max ? 10 : Math.max(1, Math.round(G.speed * 0.6));
  if (!max && newestT() - G.shownT > chunk * 60) return;
  G.inflight = true;
  try {
    await tool('ops_run', {minutes: chunk});
    await fetchState();
  } catch (e) {
    setSpeed(0);
    toast(plain(e.message), true);
  } finally {
    G.inflight = false;
  }
}

async function fetchState() {
  const st = await tool('ops_state', {since_s: G.since, series_since_s: G.seriesSince});
  G.since = st.t_s + 1;
  G.seriesSince = st.t_s + 1;
  for (const w of WORLDS) {
    G.series[w].push(...st[`series_${w}`]);
    const ws = st.worlds[w];
    if (!ws) continue;
    for (const tr of ws.trips) {
      const list = G.trips[w].get(tr.truck) || [];
      if (!list.some(x => x.depart_t_s === tr.depart_t_s)) { list.push(tr); list.sort((a, b) => a.depart_t_s - b.depart_t_s); }
      // A truck's drives long done are no use to draw.
      while (list.length > 4 && list[1].arrive_t_s < G.shownT) list.shift();
      G.trips[w].set(tr.truck, list);
    }
    // Each call rings at its own minute, not when the stretch that holds it is shown.
    G.rings[w].push(...ws.calls.filter(c => c.t_s >= G.shownT));
    G.rings[w].sort((a, b) => a.t_s - b.t_s);
  }
  G.pending.push(st);
  return st;
}

function showPace() {
  const el = $('gPace');
  const say = (text, behind = false) => { el.textContent = text; el.classList.toggle('behind', behind); };
  if (!G.started) return say('');
  if (!G.playing) return say('Paused: Space plays');
  if (G.slowed) return say('Slowed while trucks drive');
  const label = G.speed === 'max' ? 'As fast as it goes' : `${G.speed} world min a second`;
  if (G.pace.length < 2) return say(label);
  const [a, b] = [G.pace[0], G.pace[G.pace.length - 1]];
  const span = (b[0] - a[0]) / 1000;
  if (span < 2 || G.pace.some(p => p[2])) return say(label);
  const rate = (b[1] - a[1]) / 60 / span;
  if (G.speed === 'max') say(`As fast as it goes: ${rate.toFixed(0)} world min a second`);
  else if (rate < G.speed * 0.8) say(`The sim is behind: ${rate.toFixed(1)} world min a second`, true);
  else say(label);
}

function setSpeed(s) {
  if (s === 0) G.playing = false;
  else { G.speed = s; G.playing = true; }
  G.pace = [];
  document.querySelectorAll('#gSpeed button').forEach(b => {
    const v = b.dataset.speed === 'max' ? 'max' : Number(b.dataset.speed);
    b.setAttribute('aria-pressed', String(G.playing ? v === G.speed : v === 0));
  });
}

// ---- the maps ------------------------------------------------------------------------------------------------------
const S = ['coalesce', ['feature-state', 's'], 'up'];
function makeMaps() {
  if (G.mapsReady) return G.mapsReady;
  if (!MAP_OK) {
    $('gNotice').hidden = false;
    $('gNotice').textContent = "The map needs WebGL, which this browser doesn't offer. The clock, the disasters, the score, the board and the story still run.";
    $('gKey').hidden = true;
    G.mapsReady = Promise.resolve();
    return G.mapsReady;
  }
  const loads = WORLDS.map(w => new Promise(res => {
    const m = new maplibregl.Map({container: `gMap-${w}`, attributionControl: false, dragRotate: false, pitchWithRotate: false, center: [-105.09, 39.9], zoom: 14,
      style: {version: 8, sources: {}, layers: [{id: 'ground', type: 'background', paint: {'background-color': css('--ground')}}]}});
    m.touchZoomRotate.disableRotation();
    if (w === 'twin') m.addControl(new maplibregl.NavigationControl({showCompass: false}), 'bottom-left');
    G.maps[w] = m;
    m.on('load', () => { addLayers(m, w); res(); });
  }));
  // The cameras move together, so the same street is under the same storm in both worlds.
  let syncing = false;
  for (const w of WORLDS) G.maps[w].on('move', () => {
    if (syncing) return;
    syncing = true;
    const m = G.maps[w];
    for (const o of WORLDS) if (o !== w) G.maps[o].jumpTo({center: m.getCenter(), zoom: m.getZoom()});
    syncing = false;
  });
  G.mapsReady = Promise.all(loads);
  return G.mapsReady;
}

function addLayers(m, w) {
  for (const id of ['gStreets', 'gSpans', 'gFiber', 'gNodes', 'gRoutes', 'gCuts', 'gPreview']) m.addSource(id, {type: 'geojson', data: empty()});
  m.addSource('gHomes', {type: 'geojson', data: empty(), promoteId: 'home'});
  m.addLayer({id: 'gStreets', type: 'line', source: 'gStreets', paint: {'line-color': '#8d99a6', 'line-width': ['interpolate', ['linear'], ['zoom'], 13, 1, 17, 4], 'line-opacity': .55}});
  m.addLayer({id: 'gFiber', type: 'line', source: 'gFiber', filter: ['==', ['geometry-type'], 'LineString'], paint: {'line-color': css('--fiber'), 'line-width': 1.5, 'line-dasharray': [2, 2], 'line-opacity': .5}});
  m.addLayer({id: 'gSpans', type: 'line', source: 'gSpans', paint: {'line-color': css('--accent'), 'line-width': ['interpolate', ['linear'], ['zoom'], 13, .8, 17, 2.2], 'line-opacity': .45}});
  m.addLayer({id: 'gCutFill', type: 'fill', source: 'gCuts', paint: {'fill-color': css('--bad'), 'fill-opacity': .08}});
  m.addLayer({id: 'gCutEdge', type: 'line', source: 'gCuts', paint: {'line-color': css('--bad'), 'line-width': 2, 'line-dasharray': [2, 1.5]}});
  m.addLayer({id: 'gPreviewFill', type: 'fill', source: 'gPreview', paint: {'fill-color': css('--bad'), 'fill-opacity': .12}});
  m.addLayer({id: 'gPreviewEdge', type: 'line', source: 'gPreview', paint: {'line-color': css('--bad'), 'line-width': 2}});
  // Out homes glow, so a cluster of them reads from across the region.
  m.addLayer({id: 'gGlow', type: 'circle', source: 'gHomes', paint: {'circle-color': css('--bad'), 'circle-blur': 1, 'circle-radius': ['interpolate', ['linear'], ['zoom'], 12, 10, 17, 26], 'circle-opacity': ['match', S, 'out', .42, 'slow', .2, 0]}});
  m.addLayer({id: 'gHomes', type: 'circle', source: 'gHomes', paint: {
    'circle-color': ['match', S, 'out', css('--bad'), 'slow', css('--warn'), 'off', '#5d6670', css('--muted')],
    'circle-radius': ['interpolate', ['linear'], ['zoom'], 12, ['match', S, 'up', 1.3, 2.8], 16, ['match', S, 'up', 2.8, 5.5], 19, ['match', S, 'up', 5, 9]],
    'circle-opacity': ['match', S, 'up', .7, 1], 'circle-stroke-width': ['match', S, 'up', 0, 1], 'circle-stroke-color': css('--panel')}});
  m.addLayer({id: 'gNodes', type: 'circle', source: 'gNodes', paint: {'circle-radius': 4.5, 'circle-color': css('--plan'), 'circle-stroke-width': 1.5, 'circle-stroke-color': css('--panel')}});
  m.addLayer({id: 'gRoutes', type: 'line', source: 'gRoutes', layout: {'line-cap': 'round'}, paint: {'line-color': ['get', 'color'], 'line-width': 3, 'line-dasharray': [1, 1.2], 'line-opacity': .85}});
  m.on('click', e => {
    if (G.armed) { place(e.lngLat); return; }
    const hit = m.queryRenderedFeatures([[e.point.x - 5, e.point.y - 5], [e.point.x + 5, e.point.y + 5]], {layers: ['gHomes']})[0];
    if (hit) inspect({kind: 'home', w, id: hit.properties.home});
  });
  m.on('mousemove', e => {
    if (G.armed && G.armed.id === 'power') for (const o of WORLDS) G.maps[o].getSource('gPreview').setData(fc([circle([e.lngLat.lng, e.lngLat.lat], 600)]));
  });
  m.on('mouseenter', 'gHomes', () => { if (!G.armed) m.getCanvas().style.cursor = 'pointer'; });
  m.on('mouseleave', 'gHomes', () => { if (!G.armed) m.getCanvas().style.cursor = ''; });
}

const fc = features => ({type: 'FeatureCollection', features});
const pt = (c, props = {}) => ({type: 'Feature', geometry: {type: 'Point', coordinates: c}, properties: props});
// A circle of `ft` feet around a point, as a polygon, for a power cut's area.
function circle(c, ft) {
  const r = ft / 364000, k = Math.cos(c[1] * Math.PI / 180), ring = [];
  for (let i = 0; i <= 48; i++) { const a = i / 48 * 2 * Math.PI; ring.push([c[0] + r * Math.cos(a) / k, c[1] + r * Math.sin(a)]); }
  return {type: 'Feature', geometry: {type: 'Polygon', coordinates: [ring]}, properties: {}};
}

async function addBasemap(m) {
  if (!G.basemap) G.basemap = await (await fetch('https://tiles.openfreemap.org/styles/positron')).json();
  const style = G.basemap;
  if (m.getSource('bm-' + Object.keys(style.sources)[0])) return;
  for (const [id, source] of Object.entries(style.sources)) m.addSource(`bm-${id}`, source);
  if (style.glyphs) m.setGlyphs(style.glyphs);
  for (const layer of style.layers) m.addLayer({...layer, id: `bm-${layer.id}`, ...(layer.source ? {source: `bm-${layer.source}`} : {})}, 'gStreets');
  if (style.sprite) m.setSprite(style.sprite);
  m.addControl(new maplibregl.AttributionControl({compact: true, customAttribution: '<a href="https://www.openstreetmap.org/copyright" target="_blank" rel="noopener">© OpenStreetMap contributors</a>'}));
}

async function drawLayout() {
  const L = G.layout;
  G.homeGeo = new Map(L.homes.map(h => [h.home, [h.lon, h.lat]]));
  G.homeNode = new Map(L.homes.map(h => [h.home, h.service_group]));
  if (!MAP_OK) return;
  await makeMaps();
  for (const w of WORLDS) {
    const m = G.maps[w];
    m.getSource('gStreets').setData(L.streets);
    m.getSource('gSpans').setData(L.spans);
    m.getSource('gFiber').setData(L.fiber || empty());
    m.getSource('gNodes').setData(fc(L.nodes.map(n => pt([n.lon, n.lat], {sg: n.service_group}))));
    m.removeFeatureState({source: 'gHomes'});
    m.getSource('gHomes').setData(fc(L.homes.map(h => pt([h.lon, h.lat], {home: h.home, sg: h.service_group}))));
    for (const id of ['gRoutes', 'gCuts', 'gPreview']) m.getSource(id).setData(empty());
    if (G.hubMarker && G.hubMarker[w]) G.hubMarker[w].remove();
    const hub = document.createElement('div');
    hub.className = 'gHub';
    hub.innerHTML = ICON.hub;
    hub.title = 'The hub, where the trucks start';
    (G.hubMarker = G.hubMarker || {})[w] = new maplibregl.Marker({element: hub}).setLngLat(L.hub).addTo(m);
    if (L.real_geography) { try { await addBasemap(m); } catch (e) { toast("Couldn't load the basemap. The worlds run without it.", true); } }
  }
  for (const w of WORLDS) G.maps[w].resize();
  G.maps.twin.fitBounds(homeBounds(L), {padding: 40, duration: 0, maxZoom: 16.5});
}

// The camera frames the homes, and the hub only when it's close by: on a generated map the hub can sit kilometres off, and
// framing both would shrink the town to a strip. Trucks drive in from the edge instead.
function homeBounds(L) {
  const pts = L.homes.map(h => [h.lon, h.lat]).concat(L.nodes.map(n => [n.lon, n.lat]));
  if (!pts.length) return L.bounds;
  const lo = [Math.min(...pts.map(p => p[0])), Math.min(...pts.map(p => p[1]))], hi = [Math.max(...pts.map(p => p[0])), Math.max(...pts.map(p => p[1]))];
  const span = Math.hypot(hi[0] - lo[0], hi[1] - lo[1]), mid = [(lo[0] + hi[0]) / 2, (lo[1] + hi[1]) / 2];
  if (Math.hypot(L.hub[0] - mid[0], L.hub[1] - mid[1]) < span) {
    lo[0] = Math.min(lo[0], L.hub[0]); lo[1] = Math.min(lo[1], L.hub[1]); hi[0] = Math.max(hi[0], L.hub[0]); hi[1] = Math.max(hi[1], L.hub[1]);
  }
  return [lo, hi];
}

// ---- applying a state ----------------------------------------------------------------------------------------------
function applyState(st) {
  const prev = G.cur;
  G.cur = st;
  for (const w of WORLDS) {
    const ws = st.worlds[w];
    if (!ws) continue;
    const next = new Map(ws.homes_not_up.map(h => [h.home, h.status]));
    const m = G.maps[w];
    if (m && m.getSource && m.getSource('gHomes')) {
      for (const id of G.status[w].keys()) if (!next.has(id)) m.setFeatureState({source: 'gHomes', id}, {s: 'up'});
      for (const [id, s] of next) if (G.status[w].get(id) !== s) m.setFeatureState({source: 'gHomes', id}, {s});
    }
    G.status[w] = next;
    for (const c of ws.calls) {
      const list = G.calls[w].get(c.home) || [];
      if (list.some(x => x.id === c.id)) { Object.assign(list.find(x => x.id === c.id), c); continue; }
      list.push(c);
      G.calls[w].set(c.home, list);
    }
    syncJobs(w, ws.jobs);
    syncTruth(w, ws.truth);
    const tag = document.querySelector(`#gPane-${w} .gTag .n`);
    if (tag) tag.textContent = ws.counters.homes_out_now ? `${count(ws.counters.homes_out_now, 'home', 'homes')} out` : 'all working';
    for (const f of ws.feed) addFeed(w, f);
  }
  renderFeed();
  renderScore(st);
  renderSky(st.sky);
  if (!$('gBoard').hidden) renderBoard();
  if (G.inspect) renderInspect();
  if (!G.dayBase) G.dayBase = {t: st.t_s, reactive: st.counters_reactive, twin: st.counters_twin};
  if (prev && dayOf(st.t_s) > dayOf(prev.t_s)) endOfDay(st);
}

// Trucks over troubles over job pins over rings, whatever order they were made in.
const Z = {gRipple: 1, gJob: 2, gTruth: 3, gTruck: 4, gBurst: 5};
function marker(m, el, at, z = 1) { el.style.zIndex = String(z); return new maplibregl.Marker({element: el}).setLngLat(at).addTo(m); }

function ring(w, home) {
  const at = G.homeGeo.get(home), m = G.maps[w];
  if (!at || !m || !G.open) return;
  const el = document.createElement('div');
  el.className = 'gRipple';
  el.innerHTML = `<span>${ICON.phone}</span>`;
  const mk = marker(m, el, at);
  setTimeout(() => mk.remove(), reduceMotion ? 2500 : 2700);
}

function syncJobs(w, jobs) {
  const m = G.maps[w], have = G.jobs[w], open = new Set();
  for (const j of jobs) {
    if (j.outcome) continue;
    open.add(j.id);
    let e = have.get(j.id);
    if (!e && m && m.getSource) {
      // MapLibre owns the marker element's classes and transform, so what's ours goes on a child.
      const el = document.createElement('div');
      el.setAttribute('role', 'button');
      el.tabIndex = 0;
      el.onclick = ev => { ev.stopPropagation(); inspect({kind: 'job', w, id: j.id}); };
      el.onkeydown = ev => { if (ev.key === 'Enter' || ev.key === ' ') { ev.preventDefault(); el.click(); } };
      el.innerHTML = `<div>${ICON.wrench}</div>`;
      e = {el, inner: el.firstChild, mk: marker(m, el, [j.lon, j.lat], Z.gJob)};
      have.set(j.id, e);
    }
    if (e) {
      e.inner.className = `gJob ${j.crew}${j.truck ? '' : ' waiting'}`;
      e.el.setAttribute('aria-label', `Job ${j.id}: ${CREW[j.crew]} to ${j.target}, ${j.truck ? 'truck ' + j.truck : 'waiting for a truck'}`);
      e.el.title = `${CREW[j.crew]} to ${j.target}`;
    }
  }
  for (const [id, e] of have) if (!open.has(id)) { e.mk.remove(); have.delete(id); }
}

function syncTruth(w, items) {
  const m = G.maps[w], have = G.truthPins[w], keep = new Set();
  if (!m || !m.getSource) return;
  for (const t of G.truth ? items : []) {
    if (t.lon === null && !t.geo) continue;
    const at = t.geo ? [t.geo.lon_deg, t.geo.lat_deg] : null;
    if (!at) continue;
    const key = `${t.label}@${t.began_s}@${at}`;
    keep.add(key);
    if (have.has(key)) continue;
    const el = document.createElement('div');
    el.innerHTML = '<div class="gTruth"><b>!</b></div>';
    el.title = `${t.label}, since ${hhmm(t.began_s)}`;
    el.setAttribute('role', 'button');
    el.tabIndex = 0;
    el.setAttribute('aria-label', `What's really wrong: ${t.label}`);
    el.onclick = ev => { ev.stopPropagation(); inspect({kind: 'truth', w, key, item: t}); };
    el.onkeydown = ev => { if (ev.key === 'Enter' || ev.key === ' ') { ev.preventDefault(); el.click(); } };
    have.set(key, marker(m, el, at, Z.gTruth));
  }
  for (const [k, mk] of have) if (!keep.has(k)) { mk.remove(); have.delete(k); }
  const cuts = G.truth ? items.filter(t => t.radius_ft && t.geo).map(t => circle([t.geo.lon_deg, t.geo.lat_deg], t.radius_ft)) : [];
  m.getSource('gCuts').setData(fc(cuts));
}

// ---- trucks --------------------------------------------------------------------------------------------------------
// Where along a route a truck is at `t`, at an even pace from leaving to arriving: the same reading as the world's own
// `field::position`, done here only to draw it between states.
function along(path, from, to, t) {
  if (!path || !path.length) return null;
  if (t <= from || path.length === 1) return path[0];
  if (t >= to) return path[path.length - 1];
  const k = Math.cos(path[0][1] * Math.PI / 180);
  const seg = [];
  let total = 0;
  for (let i = 1; i < path.length; i++) { const d = Math.hypot((path[i][0] - path[i - 1][0]) * k, path[i][1] - path[i - 1][1]); seg.push(d); total += d; }
  let s = total * (t - from) / Math.max(1, to - from);
  for (let i = 0; i < seg.length; i++) {
    if (s <= seg[i]) { const f = seg[i] ? s / seg[i] : 0; return [path[i][0] + (path[i + 1][0] - path[i][0]) * f, path[i][1] + (path[i + 1][1] - path[i][1]) * f, i]; }
    s -= seg[i];
  }
  return path[path.length - 1];
}

const shown = w => G.view === 'both' || G.view === w;
// Free trucks park in a row beside the hub, a badge's width apart at the zoom a town is framed at.
const parked = i => G.layout ? [G.layout.hub[0] + 0.0006 * (i + 1), G.layout.hub[1] - 0.0002] : null;
// The drive a truck is on at the shown time, if any: its latest set off by then. A drive cut short is followed by the next.
function tripNow(w, id) {
  let tr = null;
  for (const x of G.trips[w].get(id) || []) if (x.depart_t_s <= G.shownT) tr = x;
  return tr;
}
function anyDriving() {
  for (const w of WORLDS) if (shown(w)) for (const id of G.trips[w].keys()) { const tr = tripNow(w, id); if (tr && G.shownT < tr.arrive_t_s) return true; }
  return false;
}
// What a truck is doing and where, at the shown time: from its drives, which a page gets every one of, and from the state
// for what's between them.
function truckView(w, k, i) {
  const tr = tripNow(w, k.id);
  if (tr && G.shownT < tr.arrive_t_s) return {doing: tr.job ? 'driving' : 'returning', at: along(tr.path, tr.depart_t_s, tr.arrive_t_s, G.shownT), trip: tr};
  if (!tr || G.cur.t_s >= tr.arrive_t_s) return {doing: k.doing, at: k.doing === 'idle' ? parked(i) : [k.lon, k.lat]};
  return tr.job ? {doing: 'working', at: tr.path[tr.path.length - 1]} : {doing: 'idle', at: parked(i)};
}

function moveTrucks(now) {
  if (!G.cur || !MAP_OK || !G.maps.twin) return;
  const routes = now - G.routesAt > 200;
  if (routes) G.routesAt = now;
  for (const w of WORLDS) {
    const ws = G.cur.worlds[w], m = G.maps[w];
    if (!ws || !m) continue;
    const have = G.trucks[w], seen = new Set(), lines = [];
    ws.trucks.forEach((k, i) => {
      seen.add(k.id);
      let e = have.get(k.id);
      if (!e) {
        const el = document.createElement('div');
        el.setAttribute('role', 'button');
        el.tabIndex = 0;
        el.onclick = ev => { ev.stopPropagation(); inspect({kind: 'truck', w, id: k.id}); };
        el.onkeydown = ev => { if (ev.key === 'Enter' || ev.key === ' ') { ev.preventDefault(); el.click(); } };
        el.innerHTML = `<div>${ICON.truck}<span>${k.id}</span></div>`;
        e = {el, inner: el.firstChild, mk: marker(m, el, [k.lon, k.lat], Z.gTruck), state: ''};
        have.set(k.id, e);
      }
      const v = truckView(w, k, i);
      const state = `gTruck ${k.crew} ${v.doing}${G.follow && G.follow.w === w && G.follow.id === k.id ? ' followed' : ''}`;
      if (e.state !== state) {
        e.state = state;
        e.inner.className = state;
        e.el.setAttribute('aria-label', `Truck ${k.id}, ${CREW[k.crew]}, ${v.doing}`);
        e.el.style.display = k.on_shift || v.doing !== 'idle' ? '' : 'none';
      }
      if (v.doing === 'working' && k.doing === 'working' && k.from_t_s !== null) e.inner.style.setProperty('--p', Math.max(0, Math.min(1, (G.shownT - k.from_t_s) / Math.max(1, k.to_t_s - k.from_t_s))).toFixed(3));
      const at = v.at && Number.isFinite(v.at[0]) && Number.isFinite(v.at[1]) ? v.at : null;
      if (at) e.mk.setLngLat([at[0], at[1]]);
      if (routes && v.doing === 'driving' && at) {
        const rest = [[at[0], at[1]], ...v.trip.path.slice((at[2] ?? 0) + 1)];
        if (rest.length > 1) lines.push({type: 'Feature', geometry: {type: 'LineString', coordinates: rest}, properties: {color: css(k.crew === 'Line' ? '--crew-line' : k.crew === 'Splicer' ? '--crew-splicer' : '--crew-service')}});
      }
      if (G.follow && G.follow.w === w && G.follow.id === k.id && at) m.jumpTo({center: [at[0], at[1]]});
    });
    for (const [id, e] of have) if (!seen.has(id)) { e.mk.remove(); have.delete(id); }
    if (routes && m.getSource('gRoutes')) m.getSource('gRoutes').setData(fc(lines));
  }
}

// ---- the sky -------------------------------------------------------------------------------------------------------
// Night, dawn and dusk from the world's hour; a storm and a heat wave from its environment. Nothing here is invented: the
// page tints by what the world says now.
function renderSky(sky) {
  if (!sky) return;
  const h = sky.hour;
  const night = h < 5 || h >= 21 ? 1 : h < 7 ? (7 - h) / 2 : h >= 19 ? (h - 19) / 2 : 0;
  const warm = Math.max(0, 1 - Math.abs(h - 6.3) / 1.2, 1 - Math.abs(h - 19.2) / 1.2);
  const storm = sky.storm_until_s ? Math.min(1, .4 + sky.rain_mm_per_hr / 40) : 0;
  const layers = [];
  if (night > 0) layers.push(`rgba(8, 18, 56, ${(.48 * night).toFixed(3)})`);
  if (warm > 0) layers.push(`rgba(255, 140, 60, ${(.13 * warm).toFixed(3)})`);
  if (storm > 0) layers.push(`rgba(45, 55, 68, ${(.3 * storm).toFixed(3)})`);
  if (sky.heat_delta_c > 0) layers.push(`rgba(255, 150, 40, ${Math.min(.14, sky.heat_delta_c / 80).toFixed(3)})`);
  const bg = layers.map(c => `linear-gradient(${c}, ${c})`).join(', ') || 'none';
  document.querySelectorAll('#gStage .gSky').forEach(el => { el.style.backgroundImage = bg; });
  document.querySelectorAll('#gStage .gRain').forEach(el => {
    el.style.opacity = sky.rain_mm_per_hr > 0 ? String(Math.min(.9, .3 + sky.rain_mm_per_hr / 30)) : '0';
    el.style.setProperty('--tilt', `${100 + Math.min(40, sky.wind_m_per_s * 1.2)}deg`);
  });
  $('gSun').innerHTML = night >= .5 ? ICON.moon : ICON.sun;
  const parts = [];
  if (sky.storm_until_s) parts.push(`<b>Storm</b> to ${hhmm(sky.storm_until_s)}<small>wind ${fmt(sky.wind_m_per_s, 0)} m/s, rain ${fmt(sky.rain_mm_per_hr, 0)} mm/h</small>`);
  if (sky.heat_delta_c > 0) parts.push(`<b>Heat wave</b> to ${hhmm(sky.heat_until_s)}<small>+${fmt(sky.heat_delta_c, 0)} °C</small>`);
  parts.push(`<b>${fmt(sky.temp_c, 0)} °C</b><small>${sky.storm_until_s || sky.heat_delta_c > 0 ? '' : 'fair'}</small>`);
  $('gWeather').innerHTML = parts.map(x => `<span>${x}</span>`).join('');
}

// ---- the score -----------------------------------------------------------------------------------------------------
function renderScore(st) {
  const a = st.counters_reactive, b = st.counters_twin;
  $('gRows').innerHTML = SCORE_ROWS.map(([label, f]) => {
    const x = f(a), y = f(b), top = Math.max(x, y, 1e-9);
    const show = v => Math.round(v).toLocaleString();
    return `<div class="gRow"><span class="lab">${esc(label)}</span><span class="val"><i style="background:var(--s1)"></i>${show(x)}<i style="background:var(--s2)"></i>${show(y)}</span>
      <div class="bars" aria-hidden="true"><div class="b" style="width:${(x / top * 100).toFixed(1)}%;background:var(--s1)"></div><div class="b" style="width:${(y / top * 100).toFixed(1)}%;background:var(--s2)"></div></div></div>`;
  }).join('');
  $('gTable').querySelector('tbody').innerHTML = ALL_ROWS.map(([label, f]) => `<tr><td>${esc(label)}</td><td>${esc(f(a))}</td><td>${esc(f(b))}</td></tr>`).join('');
  drawChart();
}

// Homes out over world time, both worlds: two 2 px lines on one axis, a legend above and the ends labelled.
function drawChart(hoverX = null) {
  const c = $('gChart'), dpr = devicePixelRatio || 1;
  const W = c.clientWidth, H = c.clientHeight;
  if (!W) return;
  c.width = W * dpr; c.height = H * dpr;
  const g = c.getContext('2d');
  g.scale(dpr, dpr);
  g.clearRect(0, 0, W, H);
  const upTo = G.cur ? G.cur.t_s : 0;
  const data = WORLDS.map(w => G.series[w].filter(s => s[0] <= upTo));
  const tMax = Math.max(6 * 3600, upTo);
  const vMax = Math.max(10, ...data.flat().map(s => s[1]));
  const nice = Math.pow(10, Math.floor(Math.log10(vMax))), yMax = Math.ceil(vMax / nice) * nice;
  const L = 30, R = 34, T = 6, B = 16;
  const x = t => L + (W - L - R) * t / tMax, y = v => T + (H - T - B) * (1 - v / yMax);
  g.font = '10px system-ui, sans-serif';
  g.strokeStyle = css('--line'); g.fillStyle = css('--muted'); g.lineWidth = 1;
  for (const v of [0, yMax / 2, yMax]) {
    g.beginPath(); g.moveTo(L, Math.round(y(v)) + .5); g.lineTo(W - R, Math.round(y(v)) + .5); g.stroke();
    g.textAlign = 'right'; g.textBaseline = 'middle'; g.fillText(Math.round(v).toLocaleString(), L - 4, y(v));
  }
  g.textAlign = 'left'; g.textBaseline = 'top'; g.fillText(clockOf(0), L, H - B + 3);
  g.textAlign = 'right'; g.fillText(clockOf(tMax), W - R, H - B + 3);
  const colors = [css('--s1'), css('--s2')];
  const ends = [];
  data.forEach((pts, i) => {
    if (!pts.length) return;
    g.strokeStyle = colors[i]; g.lineWidth = 2; g.lineJoin = 'round';
    g.beginPath();
    pts.forEach((s, j) => j ? g.lineTo(x(s[0]), y(s[1])) : g.moveTo(x(s[0]), y(s[1])));
    g.stroke();
    const last = pts[pts.length - 1];
    ends.push([y(last[1]), i, last]);
  });
  // Direct labels at the line ends when they don't collide; the legend above carries identity either way.
  if (ends.length === 2 && Math.abs(ends[0][0] - ends[1][0]) >= 11) for (const [yy, i, last] of ends) {
    g.fillStyle = css('--ink'); g.textAlign = 'left'; g.textBaseline = 'middle';
    g.fillText(i ? 'twin' : 'calls', x(last[0]) + 4, yy);
  }
  const tip = $('gTip');
  if (hoverX === null || !data[0].length) { tip.hidden = true; return; }
  const t = Math.max(0, Math.min(upTo, (hoverX - L) / (W - L - R) * tMax));
  const at = data.map(pts => pts.reduce((best, s) => Math.abs(s[0] - t) < Math.abs(best[0] - t) ? s : best, pts[0]));
  g.strokeStyle = css('--muted'); g.lineWidth = 1;
  g.beginPath(); g.moveTo(Math.round(x(at[0][0])) + .5, T); g.lineTo(Math.round(x(at[0][0])) + .5, H - B); g.stroke();
  at.forEach((s, i) => { if (!s) return; g.fillStyle = colors[i]; g.beginPath(); g.arc(x(s[0]), y(s[1]), 4, 0, 2 * Math.PI); g.fill(); g.strokeStyle = css('--panel'); g.lineWidth = 2; g.stroke(); });
  tip.hidden = false;
  tip.innerHTML = `<b>${clockOf(at[0][0])}</b><br>From calls: ${at[0][1]} homes out<br>With the twin: ${at[1] ? at[1][1] : '–'} homes out`;
  tip.style.left = `${Math.min(W - 150, Math.max(0, x(at[0][0]) + 8))}px`;
  tip.style.top = '0px';
}
$('gChart').addEventListener('mousemove', e => drawChart(e.offsetX));
$('gChart').addEventListener('mouseleave', () => drawChart());
addEventListener('resize', () => { if (G.open) drawChart(); });

// ---- the story -----------------------------------------------------------------------------------------------------
function addFeed(w, f) {
  // Lines about the plant itself are the same in both worlds, so they're shown once.
  const key = f.truth ? `truth|${f.t_s}|${f.text}` : `${w}|${f.t_s}|${f.text}`;
  if (G.feedKeys.has(key)) return;
  G.feedKeys.add(key);
  G.feed.push({...f, w: f.truth ? null : w, fresh: true});
  if (G.feed.length > 400) { const old = G.feed.shift(); G.feedKeys.delete(old.truth ? `truth|${old.t_s}|${old.text}` : `${old.w}|${old.t_s}|${old.text}`); }
}

// Node ids as a person reads them, the way the rest of the page names nodes.
const plainIds = text => String(text).replace(/\b[\w.:-]+\/sg-(\d+)\b/g, 'Node $1');
// Calls recede, since the map rings them; fixes stand out; the plant's truth is its own colour.
const feedKind = f => f.truth ? 'truth' : /^A call\b|calls? back|redial/i.test(f.text) ? 'call' : /\bfix|\bback\b|restor/i.test(f.text) ? 'fix' : '';
function renderFeed() {
  // In time order across both worlds; lines at the same minute keep the order they came in.
  const shown = G.feed.filter(f => (f.truth ? G.truth : (G.view === 'both' || f.w === G.view))).sort((a, b) => a.t_s - b.t_s);
  const list = $('gFeed');
  list.innerHTML = shown.slice(-120).reverse().map(f => `<li class="${f.fresh ? 'new' : ''} ${feedKind(f)}"><time>${hhmm(f.t_s)}</time>${f.w && G.view === 'both' ? `<span class="w ${f.w}">${f.w === 'twin' ? 'twin' : 'calls'}</span>` : ''}<span>${esc(plainIds(f.text))}</span>${f.geo ? `<button class="go" data-i="${G.feed.indexOf(f)}" aria-label="Show this on the map">Go</button>` : ''}</li>`).join('');
  if (!shown.length) list.innerHTML = `<li class="call"><span>Quiet so far: no calls and no trucks yet.${G.truth ? '' : ' The audience view shows what the weather and the plant are doing.'}</span></li>`;
  for (const f of G.feed) f.fresh = false;
  const newest = shown[shown.length - 1];
  if (newest && performance.now() - G.announced > 8000) { G.announced = performance.now(); $('gAnnounce').textContent = `${hhmm(newest.t_s)}: ${newest.text}`; }
}
$('gFeed').onclick = e => {
  const b = e.target.closest('button.go');
  if (!b || !MAP_OK) return;
  const f = G.feed[Number(b.dataset.i)];
  if (f && f.geo) G.maps.twin.flyTo({center: [f.geo.lon_deg, f.geo.lat_deg], zoom: Math.max(G.maps.twin.getZoom(), 16)});
};
$('gFeedMore').onclick = () => {
  const tall = $('gTicker').classList.toggle('tall');
  $('gFeedMore').setAttribute('aria-expanded', String(tall));
  $('gFeedMore').textContent = tall ? 'Less' : 'More';
};

// ---- disasters -----------------------------------------------------------------------------------------------------
$('gTools').insertAdjacentHTML('beforeend', TOOLS.map(t => `<button class="gTool" data-tool="${t.id}" aria-pressed="false" title="${esc(t.what)}${t.placed ? '. Then click the map.' : ''}">${ICON[t.id]}${esc(t.label)}${t.placed ? '<span class="k">then click</span>' : ''}</button>`).join(''));
$('gTools').onclick = e => {
  const b = e.target.closest('button[data-tool]');
  if (!b) return;
  const t = TOOLS.find(x => x.id === b.dataset.tool);
  if (!G.started) { toast('Start a world first.', true); return; }
  if (t.placed) { arm(G.armed && G.armed.id === t.id ? null : t); return; }
  arm(null);
  act(t, null);
};

function arm(t) {
  G.armed = t;
  document.querySelectorAll('.gTool').forEach(b => b.setAttribute('aria-pressed', String(!!t && b.dataset.tool === t.id)));
  document.querySelectorAll('#gStage .gPane').forEach(p => p.classList.toggle('armed', !!t));
  if (MAP_OK && G.maps.twin) for (const w of WORLDS) { G.maps[w].getCanvas().style.cursor = t ? 'crosshair' : ''; G.maps[w].getSource('gPreview').setData(empty()); }
  const hint = $('gHint');
  if (!t) { if (hint.dataset.kind === 'arm') { hint.hidden = true; hint.dataset.kind = ''; } return; }
  hint.dataset.kind = 'arm';
  hint.hidden = false;
  hint.innerHTML = `<span>${esc(t.hint)}</span>${MAP_OK ? '' : ''}<button class="gBtn" id="gHere">Drop it at the map's centre</button><button class="gBtn" id="gDisarm">Cancel (Esc)</button>`;
  $('gHere').onclick = () => {
    const c = MAP_OK && G.maps.twin ? G.maps.twin.getCenter() : null;
    place(c || {lng: (G.layout.bounds[0][0] + G.layout.bounds[1][0]) / 2, lat: (G.layout.bounds[0][1] + G.layout.bounds[1][1]) / 2});
  };
  $('gDisarm').onclick = () => arm(null);
}

function place(lngLat) {
  const t = G.armed;
  arm(null);
  if (t) act(t, {lon_deg: lngLat.lng, lat_deg: lngLat.lat});
}

async function act(t, at) {
  try {
    const r = await tool('ops_act', {action: t.act(at)});
    if (at && MAP_OK) for (const w of WORLDS) {
      const el = document.createElement('div');
      el.innerHTML = '<div class="gBurst"></div>';
      const mk = marker(G.maps[w], el, [at.lon_deg, at.lat_deg], Z.gBurst);
      setTimeout(() => mk.remove(), 1200);
    }
    toast(`${t.what}, at ${hhmm(r.applies_at_s)}, in both worlds.${G.truth ? '' : ' The audience view shows what it hits.'}`);
  } catch (e) { toast(plain(e.message), true); }
}

let toastTimer = null;
function toast(text, bad = false) {
  const hint = $('gHint');
  if (G.armed) return;
  hint.dataset.kind = 'toast';
  hint.hidden = false;
  hint.style.borderColor = bad ? 'var(--bad)' : 'var(--accent)';
  hint.innerHTML = `<span>${text}</span>`;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { if (hint.dataset.kind === 'toast') { hint.hidden = true; hint.style.borderColor = ''; } }, 4500);
}

// ---- the inspector -------------------------------------------------------------------------------------------------
function inspect(what) { G.inspect = what; renderInspect(); }
const SOURCE = s => s.Ticket !== undefined ? `ticket ${s.Ticket}, from calls` : s.Escalation !== undefined ? `job ${s.Escalation} finding nothing, sent on` : "the twin's reading of telemetry";
const OUTCOME = o => !o ? '' : o.Fixed ? `fixed: ${o.Fixed.what}` : o === 'NoTroubleFound' ? 'no trouble found' : o === 'PowerCompany' ? "no house power: the power company's to fix" : o === 'Cancelled' ? 'called off: the homes came back' : JSON.stringify(o);
function renderInspect() {
  const q = G.inspect, card = $('gInspect');
  if (!q || !G.cur) { card.hidden = true; return; }
  const ws = G.cur.worlds[q.w];
  let head = '', body = '', acts = '';
  if (q.kind === 'truck') {
    const k = ws.trucks.find(x => x.id === q.id);
    if (!k) { card.hidden = true; return; }
    // What it's doing at the shown time, from its drives where the state hasn't caught up.
    const v = truckView(q.w, k, 0);
    const jobId = v.trip ? v.trip.job : k.job;
    // A drive from a stretch not yet shown names a job only that stretch holds.
    const job = jobId ? [G.cur, ...G.pending].map(x => x.worlds[q.w] && x.worlds[q.w].jobs.find(j => j.id === jobId)).find(Boolean) : null;
    const until = v.trip ? v.trip.arrive_t_s : k.to_t_s;
    head = `Truck ${k.id}: ${CREW[k.crew]}`;
    body = v.doing === 'driving' ? `Driving to ${esc(job ? job.target : 'a job')}, there at ${hhmm(until)}.`
      : v.doing === 'working' ? `On site at ${esc(job ? job.target : 'a job')}${k.doing === 'working' ? `, done about ${hhmm(k.to_t_s)}` : ''}.`
      : v.doing === 'returning' ? `Going back to the hub, there at ${hhmm(until)}.` : k.on_shift ? 'At the hub, free.' : 'At the hub, off shift.';
    if (job) body += `<div class="muted">Sent by ${esc(SOURCE(job.source))}.</div>`;
    const following = G.follow && G.follow.w === q.w && G.follow.id === k.id;
    acts = MAP_OK ? `<button class="gBtn" id="gFollow">${following ? 'Stop following' : 'Follow it'}</button>` : '';
  } else if (q.kind === 'job') {
    const j = ws.jobs.find(x => x.id === q.id);
    if (!j) { card.hidden = true; return; }
    head = `Job ${j.id}: ${CREW[j.crew]}`;
    body = `To ${esc(j.target)} on ${esc(nodeShort(j.service_group))}. Made at ${hhmm(j.created_t_s)}, sent by ${esc(SOURCE(j.source))}.<div>${j.truck ? `Truck ${j.truck}${j.arrived_t_s ? `, on site since ${hhmm(j.arrived_t_s)}` : ', on the way'}.` : 'Waiting for a truck.'}</div>${j.outcome ? `<div>${esc(OUTCOME(j.outcome))}</div>` : ''}`;
  } else if (q.kind === 'home') {
    const s = G.status[q.w].get(q.id) || 'up';
    const calls = G.calls[q.w].get(q.id) || [];
    head = `A home on ${esc(nodeShort(G.homeNode.get(q.id) || ''))}`;
    body = `<div class="muted">${esc(q.id)}, by its modem's place in the plant: no name or address (invariant 10).</div>
      <div>${{up: 'Working.', slow: 'Working badly: slow, or in and out.', out: 'No service.', off: 'No house power, so the modem is off.'}[s]}${s === 'off' && !G.truth ? ' Its household knows that; an operator sees a modem offline.' : ''}</div>
      <div>${calls.length ? `${count(calls.length, 'call', 'calls')} from here: ${calls.map(c => `${hhmm(c.t_s)}${c.abandoned ? ' (gave up)' : c.answered ? '' : ' (waiting)'}`).join(', ')}.` : 'No calls from here.'}</div>`;
  } else if (q.kind === 'truth') {
    head = "What's really wrong";
    body = `${esc(q.item.label)}, since ${hhmm(q.item.began_s)}${q.item.service_group ? ` on ${esc(nodeShort(q.item.service_group))}` : ''}. <div class="muted">${esc(q.item.kind)}. The audience's view: the dispatcher never sees this.</div>`;
  }
  card.hidden = false;
  card.innerHTML = `<div class="head"><span>${head}</span><span class="tag">${esc(WORLD_NAME[q.w])}</span><button class="gBtn" id="gInspectX" aria-label="Close">×</button></div><div>${body}</div>${acts ? `<div class="acts">${acts}</div>` : ''}`;
  $('gInspectX').onclick = () => { G.inspect = null; card.hidden = true; };
  if ($('gFollow')) $('gFollow').onclick = () => {
    G.follow = G.follow && G.follow.id === q.id && G.follow.w === q.w ? null : {w: q.w, id: q.id};
    renderInspect();
  };
}
for (const w of WORLDS) $(`gMap-${w}`).addEventListener('mousedown', () => { if (G.follow) { G.follow = null; if (G.inspect) renderInspect(); } });

// ---- drawers -------------------------------------------------------------------------------------------------------
function drawer(id, btn, render) {
  const d = $(id), b = $(btn);
  b.onclick = () => {
    const open = d.hidden;
    for (const [o, ob] of [['gBoard', 'gBoardBtn'], ['gSources', 'gSourcesBtn']]) { $(o).hidden = true; $(ob).setAttribute('aria-expanded', 'false'); }
    if (!open) return;
    d.hidden = false;
    b.setAttribute('aria-expanded', 'true');
    // Focus moves in once there's something to focus: the sources load from the server first.
    Promise.resolve(render()).then(() => { const x = d.querySelector('button'); if (x) x.focus(); });
  };
}
const closeX = id => `<button class="gBtn" data-close="${id}" aria-label="Close">×</button>`;
document.addEventListener('click', e => {
  const b = e.target.closest && e.target.closest('#gStage [data-close]');
  if (!b) return;
  $(b.dataset.close).hidden = true;
  const btn = {gBoard: 'gBoardBtn', gSources: 'gSourcesBtn'}[b.dataset.close];
  if (btn) { $(btn).setAttribute('aria-expanded', 'false'); $(btn).focus(); }
});

function renderBoard() {
  const st = G.cur;
  if (!st) return;
  const shown = G.view === 'both' ? WORLDS : [G.view];
  const col = w => {
    const ws = st.worlds[w], c = ws.counters;
    const open = ws.jobs.filter(j => !j.outcome), done = ws.jobs.filter(j => j.outcome);
    const tickets = ws.tickets.filter(k => k.status !== 'Closed');
    return `<div class="gCol"><h3><span class="sw" style="background:var(--${w === 'twin' ? 's2' : 's1'})"></span>${esc(WORLD_NAME[w])}</h3>
      <div class="muted">Call centre: ${count(ws.calls_waiting, 'call', 'calls')} waiting; so far ${count(c.calls, 'call', 'calls')}, ${c.calls_told_of_outage} told of a known outage, ${c.calls_abandoned} abandoned.</div>
      <h2 style="margin-top:8px">Open tickets</h2><div class="gList">${tickets.map(k => `<div>Ticket ${k.id}, ${esc(nodeShort(k.service_group))}: ${count(k.calls, 'call', 'calls')}, ${esc(k.status === 'Dispatched' ? 'a truck has it' : 'waiting for a truck')}</div>`).join('') || '<div class="muted">None.</div>'}</div>
      <h2 style="margin-top:8px">Jobs</h2><div class="gList">${open.map(j => `<div><b>${esc(CREW[j.crew])}</b> to ${esc(j.target)}: ${j.truck ? `truck ${j.truck}, ${j.arrived_t_s ? 'on site' : 'on the way'}` : 'waiting for a truck'}<div class="muted">Sent by ${esc(SOURCE(j.source))} at ${hhmm(j.created_t_s)}</div></div>`).join('') || '<div class="muted">No open jobs.</div>'}
      ${done.map(j => `<div class="muted">Done ${hhmm(j.done_t_s)}: ${esc(CREW[j.crew])} at ${esc(j.target)}, ${esc(OUTCOME(j.outcome))}</div>`).join('')}</div></div>`;
  };
  $('gBoard').innerHTML = `<div class="head"><h2>Call centre and dispatch, at ${clockOf(st.t_s)}</h2>${closeX('gBoard')}</div>
    <div class="muted" style="margin-bottom:6px">What an operator sees: calls, tickets, jobs and trucks. Never the fault list.</div>
    <div class="gCols${shown.length === 1 ? ' one' : ''}">${shown.map(col).join('')}</div>`;
}
drawer('gBoard', 'gBoardBtn', renderBoard);

let ratesCache = null;
async function renderSources() {
  if (!ratesCache) ratesCache = await tool('ops_rates');
  const r = ratesCache.rates, L = G.layout;
  const tally = {};
  for (const v of Object.values(r)) tally[v.confidence] = (tally[v.confidence] || 0) + 1;
  $('gSources').innerHTML = `<div class="head"><h2>Where the numbers come from</h2>${closeX('gSources')}</div>
    <div class="muted">${L ? `Seed ${L.seed}. Hazards at ${L.intensity} times the rates below, so a demo sees weeks of trouble in a day. ` : ''}${L && L.scenario ? `Scenario: <b>${esc(L.scenario.title)}</b>. ${esc(L.scenario.blurb)} ` : ''}Every plant, household, call and truck is simulated. ${Object.entries(tally).map(([k, n]) => `${n} ${k.toLowerCase()}`).join(', ')}.</div>
    <table style="margin-top:8px"><thead><tr><th>Rate</th><th>Value</th><th>Basis</th></tr></thead><tbody>${Object.entries(r).map(([k, v]) => `<tr><td><b>${esc(k)}</b><div class="muted">${esc(v.note)}</div></td><td>${esc(v.value)} ${esc(v.unit)}</td><td><span class="tag">${esc(v.confidence)}</span>${v.source_url ? `<div><a href="${esc(v.source_url)}" target="_blank" rel="noopener">source</a></div>` : ''}</td></tr>`).join('')}</tbody></table>`;
}
drawer('gSources', 'gSourcesBtn', () => renderSources().catch(e => toast(plain(e.message), true)));

// ---- the end of a day ----------------------------------------------------------------------------------------------
function endOfDay(st) {
  setSpeed(0);
  const base = G.dayBase, day = DAY[(dayOf(st.t_s) - 1) % 7];
  const rows = [['Customer-hours out', c => c.customer_minutes_out / 60], ['Truck rolls', c => c.truck_rolls], ['Rolls that found nothing', c => c.no_trouble_found], ['Calls', c => c.calls], ['Calls abandoned', c => c.calls_abandoned], ['Faults fixed', c => c.fixes]];
  const d = (f, w) => Math.round(f(st[`counters_${w}`]) - f(base[w])).toLocaleString();
  $('gDay').hidden = false;
  $('gDay').innerHTML = `<div class="box" role="alertdialog" aria-labelledby="gDayHead"><h2 id="gDayHead" style="font-size:16px;text-transform:none;color:var(--ink)">${day} is over</h2>
    <table><thead><tr><th></th><th>From calls</th><th>With the twin</th></tr></thead><tbody>${rows.map(([l, f]) => `<tr><td>${esc(l)}</td><td>${d(f, 'reactive')}</td><td>${d(f, 'twin')}</td></tr>`).join('')}</tbody></table>
    <div style="display:flex;gap:8px;margin-top:10px"><button class="gBtn primary" id="gDayOn">Play on</button><button class="gBtn" id="gDayStay">Stay paused</button></div></div>`;
  G.dayBase = {t: st.t_s, reactive: st.counters_reactive, twin: st.counters_twin};
  $('gDayOn').focus();
  $('gDayOn').onclick = () => { $('gDay').hidden = true; setSpeed(G.speed); };
  $('gDayStay').onclick = () => { $('gDay').hidden = true; };
}

// ---- views, the audience view, keys --------------------------------------------------------------------------------
function setView(v) {
  G.view = v;
  document.querySelectorAll('#gView button').forEach(b => b.setAttribute('aria-pressed', String(b.dataset.view === v)));
  $('gStage').classList.toggle('one', v !== 'both');
  for (const w of WORLDS) $(`gPane-${w}`).classList.toggle('shown', v === 'both' || v === w);
  if (MAP_OK && G.maps.twin) for (const w of WORLDS) G.maps[w].resize();
  renderFeed();
  if (!$('gBoard').hidden) renderBoard();
}
$('gView').onclick = e => { const b = e.target.closest('button[data-view]'); if (b) setView(b.dataset.view); };
$('gWatch').onclick = () => { G.watchDrives = !G.watchDrives; $('gWatch').setAttribute('aria-pressed', String(G.watchDrives)); };
$('gSpeed').onclick = e => {
  const b = e.target.closest('button[data-speed]');
  if (!b || !G.started) return;
  setSpeed(b.dataset.speed === 'max' ? 'max' : Number(b.dataset.speed));
};
$('gTruth').onchange = () => {
  G.truth = $('gTruth').checked;
  if (G.cur) for (const w of WORLDS) if (G.cur.worlds[w]) syncTruth(w, G.cur.worlds[w].truth);
  renderFeed();
  if (G.inspect) renderInspect();
};

document.addEventListener('keydown', e => {
  if (!G.open) return;
  if (e.key === 'Escape') {
    if (G.armed) arm(null);
    else if (!$('gDay').hidden) $('gDay').hidden = true;
    else if (G.inspect) { G.inspect = null; $('gInspect').hidden = true; }
    else for (const [d, b] of [['gBoard', 'gBoardBtn'], ['gSources', 'gSourcesBtn']]) if (!$(d).hidden) { $(d).hidden = true; $(b).setAttribute('aria-expanded', 'false'); $(b).focus(); }
    return;
  }
  // Space and 1 to 4 are the clock's wherever focus is on the screen, a button included: a focused button would otherwise take
  // Space as a click, so Space after clicking Storm would call a second storm. Fields keep their keys, and so does a map marker,
  // which handles its own.
  // A checkbox keeps Space, which ticks it, but has no use for digits; a text field keeps both.
  const is = sel => e.target.matches && e.target.matches(sel);
  if (e.defaultPrevented || is('select, textarea, input:not([type=checkbox]):not([type=radio])')) return;
  if (e.key === ' ') { if (is('input')) return; e.preventDefault(); if (G.started) setSpeed(G.playing ? 0 : G.speed); }
  else if (SPEED_KEYS[e.key] && G.started) setSpeed(SPEED_KEYS[e.key]);
});
// A button's Space click fires on key-up; the clock already took the key.
document.addEventListener('keyup', e => { if (G.open && e.key === ' ' && e.target.matches && e.target.matches('button:not([role=tab])')) e.preventDefault(); });

// ---- Operate as a mode (task 29) -----------------------------------------------------------------------------------
// The page calls this when the Operate tab is shown or left. A world plays only while it's watched: leaving pauses it, and coming
// back resumes at the speed it had, the rule task 28 had for leaving its screen.
async function showOps() {
  G.open = true;
  if (!G.view || !document.querySelector('#gView [aria-pressed=true]')) setView(innerWidth <= 760 ? 'twin' : 'both');
  if (!G.started) {
    $('gNotice').hidden = false;
    $('gNotice').textContent = 'No world is running. Start one from the panel: a scenario, or free play on the town designed in Design.';
    return;
  }
  if (MAP_OK) $('gNotice').hidden = true;
  await makeMaps();
  if (MAP_OK) for (const w of WORLDS) G.maps[w].resize();
  setSpeed(G.resume ? G.speed : 0);
  G.resume = false;
  drawChart();
}
function hideOps() {
  if (!G.open) return;
  G.resume = G.playing;
  setSpeed(0);
  arm(null);
  G.open = false;
}
window.opsMode = on => (on ? showOps() : hideOps());

// ---- the launcher (the Operate tab) --------------------------------------------------------------------------------
let scenarioList = [];
async function loadScenarios() {
  const r = await tool('ops_scenarios');
  scenarioList = r.scenarios;
  $('oScenario').innerHTML = '<option value="">Free play: nothing planned, you call the disasters</option>' + scenarioList.map(s => `<option value="${esc(s.id)}">${esc(s.title)}</option>`).join('');
}
$('oScenario').onchange = () => {
  const s = scenarioList.find(x => x.id === $('oScenario').value);
  $('oScenarioBlurb').textContent = s ? `${s.blurb} Seed ${s.seed}, hazards at ${s.intensity} times the sourced rates.` : 'Nothing is planned: the disasters are yours to call, and wear and weather run at the rates times the intensity.';
  if (s) { $('oSeed').value = s.seed; $('oIntensity').value = s.intensity; }
};
quietly(async () => { await loadScenarios(); $('oScenario').value = 'stormy-monday'; $('oScenario').onchange(); });

function reset() {
  setSpeed(0);
  arm(null);
  for (const w of WORLDS) {
    for (const e of G.trucks[w].values()) e.mk.remove();
    for (const e of G.jobs[w].values()) e.mk.remove();
    for (const mk of G.truthPins[w].values()) mk.remove();
    G.trucks[w].clear(); G.jobs[w].clear(); G.truthPins[w].clear(); G.status[w].clear(); G.calls[w].clear(); G.trips[w].clear();
    G.rings[w] = [];
    G.series[w] = [];
  }
  Object.assign(G, {pending: [], cur: null, since: 0, seriesSince: 0, shownT: 0, feed: [], feedKeys: new Set(), dayBase: null, inspect: null, follow: null, speed: 5});
  $('gInspect').hidden = true;
  $('gDay').hidden = true;
  $('gFeed').innerHTML = '';
}

$('oStart').onclick = () => guarded(async () => {
  const v = readAll([['seed', 'oSeed', 'Seed', 0, 2 ** 32, true], ['intensity', 'oIntensity', 'Hazards', 0, 10000, false]]);
  const args = {seed: v.seed, intensity: v.intensity};
  // The map on the Design tab, if it's designed: the region a person has just looked at is the one they mean.
  if (typeof curMap !== 'undefined' && curMap && curMap.node_sites && curMap.node_sites.length) args.region_id = `region-${curMap.map_id}`;
  if ($('oScenario').value) args.scenario = $('oScenario').value;
  let r;
  try { r = await tool('ops_start', args); } catch (e) {
    if (!/no region is designed/.test(e.message)) throw e;
    say('oStartMsg', 'No region is designed yet. Design one on the Design tab, or generate a compact town here: an urban grid of about 1,600 homes, with the Design tab\'s other settings.', 'info', '<button id="oGen">Generate a town, then start</button>');
    $('oGen').onclick = async () => { say('oStartMsg', ''); $('dFamily').value = 'UrbanGrid'; $('dHomes').value = '1600'; await $('dGo').onclick(); $('oStart').click(); };
    return;
  }
  reset();
  G.started = true;
  G.layout = await tool('ops_layout');
  const L = G.layout;
  $('gRun').textContent = `Seed ${L.seed} · hazards ×${L.intensity}${L.scenario ? ` · ${L.scenario.title}` : ' · free play'}`;
  $('oLiveInfo').innerHTML = `${esc(mapName(r.map_id))}: ${count(r.nodes, 'node', 'nodes')}, ${count(r.homes, 'home', 'homes')}, ${count(r.trucks, 'truck', 'trucks')}. ${L.scenario ? `Scenario: ${esc(L.scenario.title)}.` : 'Free play.'}`;
  $('oLive').hidden = false;
  $('oSetup').open = false;
  $('oSetupHead').textContent = 'Start again';
  if (!G.open) document.querySelector('#t-ops').click();
  await showOps();
  document.querySelector('#gSpeed button[data-speed="5"]').focus();
  await drawLayout();
  const st = await fetchState();
  applyState(G.pending.shift());
  G.shownT = st.t_s;
  toast(`${L.scenario ? esc(L.scenario.blurb) + ' ' : ''}Press ▶ or Space to start the clock.`);
}, 'oStartMsg');
})();
