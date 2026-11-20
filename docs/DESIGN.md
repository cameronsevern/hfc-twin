# HFC Digital Twin — Design One-Pager

Nov 2, 2026 · Cameron Severn · exported from the living design doc; see docs/DESIGN.md history for changes

A one-stop shop for building and maintaining modern HFC networks: plant records, design and migration, monitoring, diagnosis and work orders, all reading one plant graph. A physics simulation of real plant and real components sits underneath, so every function shares one model of the network. Diagnosis is the inverse of the simulation. Everything runs on simulated data for now; real plant, real telemetry and real work orders plug in at three defined seams.

## Vision

**The problem is fragmentation.** At a Tier 1 operator, each function has its own system, built in isolation by different teams, and their data doesn't work together by default. Design doesn't know what monitoring sees, monitoring doesn't know what was built, and no one can follow a problem from a modem to a part to a fix to proof that it worked.

**The destination is one system for all of it.** One data model, the plant graph, that every function reads, so the functions agree because they share their source.

**The way there is one function at a time.** It runs alongside what an operator already has, read-only, then takes over functions in the order that hurts most. Nobody can switch off working systems all at once for an unproven product, and it doesn't need them to.

**First target: a Tier 1 operator midway through a DAA and high-split rollout.** The first version doesn't need to be turnkey. It needs to convince an operator's data science and PNM engineers that one model of the plant can do all of this across a mixed estate.

**It runs on simulated data until real data is available,** which won't be soon. So nothing yet says whether the sim matches real plant, and we say so plainly. We check it against CableLabs' published results, report which of our estimates actually change an answer, and never present a synthetic result as a field result.

**The demo is the whole lifecycle on a simulated region:** design a node and generate its parts list, mark it built, watch it run, inject a fault, diagnose it, issue the work order, confirm that telemetry shows the fix worked, then migrate the node from low-split I-CCAP to high-split RPD. No other tool can show that loop on one model.

**The order:** prove the loop on one street (v0), show the whole lifecycle at region scale and check it against published results (v1), run it read-only on real plant when data is available (v2), then take over functions in production (v3). Small and rural ISPs share the core and come afterward.

## Problem

Tier 1 operators have plenty of data and many systems, but no shared model. Each team's tool holds its own view of the plant, so PNM data rarely reaches the whole estate with one method, plant records drift from what monitoring sees, and turning data into "replace the tap at 4th and Main" still takes scarce specialists. A mid-rollout estate makes it harder: two splits, two node architectures and several CMTS vendors at once.

Small and rural cable ISPs have the opposite problem: no design tool and no PNM analytics. Plant design software (Bentley Coax, Lode Data) is priced and built for Tier 1 operators. DOCSIS 3.1 modems report rich PNM data, D3.0 modems much less, and interpreting either takes a specialist the small operator doesn't have.

No existing product combines plant records, design, a physics simulation and PNM diagnosis in one loop. That loop is the product.

## Product and user

| Function | What it does | Starts at |
|---|---|---|
| Plant records | Inventory and as-built state; each element is marked designed, built or imported | Step 1 |
| Design and migration | Layouts, parts lists, node splits, low-split to high-split planning | Steps 8, 10 |
| Monitoring | PNM and plant-active telemetry, in one schema across vendors | Step 4 |
| Diagnosis | Which component, where, how confident, and why, with simulated evidence beside the data | Step 5 |
| Work orders | The fix from an operator-owned policy table, dispatch, and proof from telemetry that it worked | Steps 5, 11 |

**Out of scope:** billing, provisioning, customer care and CMTS configuration. The twin reads from those systems and exchanges work orders with ticketing, but doesn't replace them.

**Generated plant is never presented as the operator's plant.** A twin that localizes real faults has to be built from real records, because a generated plant can't truthfully say "replace the tap at 4th and Main". Every element records whether it was designed, built or imported, and every dataset whether it is synthetic or field (`CLAUDE.md` invariant 11).

**Primary user: a Tier 1 operator's engineers.** The design center is a long-lived estate where mergers have left several CMTS vendors side by side, running low-split with high-split arriving. It would deploy into the operator's own cloud account with SSO and a security review. The default experience exposes engineering detail: level-budget tables, per-subcarrier plots, raw telemetry, and the reasoning behind every diagnosis. Data science needs the model evidence too: data contract, held-out evaluation, calibration.

**The design assumes a DAA (Remote PHY) rollout in progress.** That makes the RPD a first-class plant active (optical levels, RF output, temperature via the vCore), and moves CMTS-side PNM capture to the RPD, where feature availability depends on both the RPD and the Core. Modem PNM tests stay on the modem. During the rollout, legacy I-CCAP and DAA segments coexist. A twin that models both and compares each node before and after migration is useful during the rollout rather than after.

**Migration comparison shows what the physics can compute:** achievable modulation and capacity per modem in both directions, transmit headroom as upstream rises to 204 MHz, which parts must change, and which modems lose service. Any change in noise from removing analog optics stays labeled a what-if until real measurements calibrate it.

**Later customer: small and rural ISPs.** The one-person network team, wanting opinionated defaults and no level-budget tables unless asked. Same core, different UX and deployment. Their plant is probably low-split at 1 GHz or below with many D3.0 modems, which the first profiles already model.

**Tier 1 requirements.** What has to be true before a Tier 1 operator can run it in production. The v1 and v2 work builds toward these, but only v3 finishes them:

| Area | Requirement |
| --- | --- |
| Scale | Millions of modems, thousands of nodes, dozens of CMTSs. Twin sharded by CMTS and service group; incremental recompute is the scaling model, not an optimization |
| Multi-headend | Many headends and hubs, each with its own CMTS mix; plant imported per region, diagnosis and triage roll up across all of them |
| Deployment | The product deploys as Terraform into a customer-supplied cloud account and VPC, AWS first. The application depends on four primitives only: Postgres, S3-API object storage through an abstraction (opendal), a Postgres-backed queue, and containers on Kubernetes (or Docker Compose on one VM for development and demos). Inference runs in-container; SSO is generic OIDC. Cloud-specific code lives only in Terraform, so GCP or Azure is a new Terraform module with no application change. Telemetry reaches their VPC over their own network and never leaves their account |
| Access and audit | SSO (SAML/OIDC), role-based access, audit log of every query and recommendation, least-privilege SNMP and API credentials. Status reads are read-only, but triggering PNM tests needs SNMP write to the test objects |
| Security review | Vendor security questionnaire, dependency and container scanning, documented data flows; this is the gating item for management-plane access |
| Integrations | Runs alongside existing systems until each function is taken over: work orders out to their ticketing system; plant import from their GIS; MCP server as the general-purpose query surface for their own agents and tools |
| Data governance | Retention policies per data class, pseudonymized subscriber identity by default, CPNI-aligned access controls |
| Reliability | High availability for the collection layer so a product outage never drops telemetry; backfill on reconnect |
| Validation | Calibration report against their real measurements per CMTS vendor; shadow mode of the diagnoser against their existing process before any recommendation reaches a tech |

## Architecture

```
Map + headend ──┐
Component lib ──┼──► Plant model ─► Optimizer ─► Physics sim ─► Telemetry ─► Diagnosis ─► Work order
+ profile    ───┘        ▲                          (RF, AC,      (PNM        (inverse       (map pin
                         │                           faults)       schema)     of sim)        + fix)
            Real plant import (v2)    Real CMTS feed (emulated v1, v2)  Work orders (v2)
                                   ════ MCP server: plant graph, telemetry, faults, sim controls, optimizer ════
                                            Operator's own Claude session, or any MCP client
```

Everything between the map and the work order is deterministic Rust, except the ML diagnoser, which is trained offline and runs as a pure function at inference. In v2 the three dashed boxes bring in the real plant, real telemetry, and real work-order history; nothing downstream of them changes. The MCP server is the only AI surface; the operator brings their own agent.

## Architecture profiles

The simulator and optimizer are parameterized by an **architecture profile**, never hard-coded to one DOCSIS generation. A profile is a data file that selects among implemented mechanisms; adding one adds no code unless it needs a new mechanism. The first FDX profile does, for its scheduler, echo cancellation and condition-dependent telemetry. Later ones are data.

| Profile | Downstream / upstream | Max cascade | Node class | Notes |
| --- | --- | --- | --- | --- |
| D3.1 low-split, I-CCAP | 5–42 MHz up, 54–1002 MHz down | N+6 | 1 GHz analog | First profile: the legacy state. Mixed D3.0 and D3.1 modems. The 1002 MHz edge is our assumption, to confirm with an operator |
| D3.1 high-split, RPD | 5–204 MHz up, 258–1218 MHz down | N+6 | 1.2 GHz RPD | First profile: the migration target |
| D3.1 mid-split | 5–85 MHz up, 108–1218 MHz down | N+6 | 1.2 GHz | Data file only, for other operators' plant such as Comcast's |
| D4.0 ESD | One of five splits up to 5–85 + 108–684 MHz up, 834–1794 MHz down | N+3 (our design choice) | 1.8 GHz, or 1.2 GHz if downstream stops there | 1.8 GHz amps and taps only for downstream above about 1.2 GHz; amps must match the split regardless |
| D4.0 FDX | 108–684 MHz shared (classic) or 258–834 MHz (shifted), down up to 1794 MHz | N+0 simplest, N+x with FDX amps | FDX node | Echo-cancellation model; FDX amps exist (SCTE 296) |

A profile defines: band plan, channel plan, OFDM/OFDMA parameters, cascade limit, homes-passed limit, target levels and tilt, and the plant-side capability set. Effective capability is the profile's set intersected with each modem's, because generations mix on one node. Split and node type can change separately, so any combination is a data file, and D4.0 assumes DAA only. An RPD removes the analog optical noise term from the RF path, and its digital fiber side is otherwise a fault source (RPD offline, link down). It also differs in capability set and upstream set point. A hub-shelf RPD with linear optics is out of scope for v1. Switching profiles on an existing design re-runs the optimizer and flags components that no longer qualify.

## Component library

Real, purchasable parts with spec-sheet data. Each entry is versioned, records its rated band and, for actives, its duplex type, and carries its source document link. Whether a part qualifies for a profile is computed from those, never stored.

| Class | Examples | Spec fields captured |
| --- | --- | --- |
| Node | CommScope/ATX/Harmonic 1.2 and 1.8 GHz nodes | Output level and tilt, return path, port count, power draw; for DAA nodes, RPD model and firmware, which set the available PNM features |
| Amplifier | Cisco GainMaker, Technetix DBx, ATX GigaXtend | Gain, tilt, noise figure, CTB/CSO, max output, power draw |
| Tap | Antronix, PCT, Corning Gilbert | Port count, tap value (4–29 dB), insertion loss by frequency, through loss |
| Hardline coax | CommScope P3 .500/.625/.750, Times Fiber | Attenuation dB/100 ft by frequency, temperature coefficient, loop resistance |
| Drop coax | RG6, RG11 | Attenuation by frequency, max drop length |
| Passives | Splitters, directional couplers, power inserters | Loss, isolation, current rating |
| Power supply | Alpha XM3 class | Output voltage, current, battery runtime |

Policy: specs come from public spec sheets and data sheets, not scraped manuals. Where a vendor publishes only partner-gated data, the library records the gap and uses a generic entry marked as such. Operators can add their own parts from an invoice.

## Physics simulation

The sim computes what a real plant would measure, not an abstraction of it. Rust core, deterministic given a seed.

**Forward path:** per-frequency level budget from node to each tap port. Cable loss by length, frequency and temperature; tap insertion and through loss; amp gain, tilt and AGC. Output is level and tilt at every port, which must land inside the profile's window.

**Noise and distortion:** cascaded noise figure to CNR; CTB/CSO from amp specs and loading; thermal noise floor. Converted to per-subcarrier MER at each modem, which is the number PNM actually reports.

**Return path:** modem transmit power needed to hit the CMTS target; noise funneling across the node's service group; ingress sources injected as faults.

**AC powering:** current draw per active device, voltage drop along coax by loop resistance, power supply loading. A design is invalid if any amp sees less than its minimum input voltage.

**Design optimizer:** part of the product, for new builds, node splits and migration, and also the source of training layouts. Its output is a set of elements marked designed, with a parts list derived from them. Classical, not AI. Routes along OSM ways (Steiner-tree heuristic), places taps to reach every home within max drop length, inserts amps when the level budget is exhausted, splits nodes at the profile's homes-passed and cascade limits. Objective: minimize actives and cable with all constraints met. The user adjusts; the optimizer re-solves.

## Reactivity

Everything downstream of the plant model is a pure derivation of it. Design, sim, telemetry and diagnosis form a dependency graph, not a batch pipeline: a design change recomputes only the service groups it touches.

- **Incremental recompute.** Sim and telemetry run per service group, keyed by a hash of that group's plant, profile, component library entries, environment and fault set. Changing node 14 recomputes node 14, not the region.
- **Deterministic seeding.** Telemetry and injected faults are seeded per service group, so a design change keeps the same faults in the same places and shows how diagnosis changes, not a new random world.
- **Design controls are first-class inputs:** profile per node (split and node type), homes-passed-per-node target, cascade limit, aerial or underground per street, and manual overrides (pin a node, force a split). The optimizer re-solves around pinned items.

**Demo:** one headend serving a region of roughly 2,000–10,000 homes passed (20–80 nodes), half low-split I-CCAP and half high-split RPD. The presenter changes the network type and design parameters on the map and the downstream layers update in place, including the pre- and post-migration comparison.

**Impairment workbench.** Impairments are physical parameters, not scripted signatures: a corroded connector is a return-loss value at a position along a cable, and the sim derives reflection delay, pre-equalization coefficients and RxMER ripple from it. The user clicks a span, tap, amp or drop on the map, picks a fault class, sets its parameters (severity, position, onset, time profile such as worsening with rainfall), and watches the telemetry and the diagnoser's ranked answer respond across the affected service group. It serves as the demo and a tech training sandbox. The training-data generator is the same parameter sweep without the UI (built at step 4): sweeping many generated layouts keeps the ML agnostic to any one plant.

## Telemetry and fault model

Synthetic telemetry is generated per modem from the sim, in the same schema a real CMTS exposes through CableLabs PNM, SNMP and IPDR. Which measurements exist depends on the modem generation and the capability set in the profile, so the twin never emits data a real deployment could not. A D3.0 modem has SC-QAM pre-equalization and little else, which matters on low-split plant.

**Measurements:** downstream RxMER per subcarrier, full-band capture spectrum, pre-equalization coefficients, FEC codeword counts (correctable/uncorrectable), transmit and receive power, partial-service state, T3/T4 timeouts, upstream SNR per channel, node and amp status where the hardware supports it.

**Telemetry comes from the hardware, not from an operator's data lake.** Going direct keeps fidelity uncapped by someone else's pipeline and sidesteps internal data-ownership disputes. Three sources, all on the same feature contract:

- **CMTS:** SNMP for registration and channel state, IPDR streaming for bulk counters, CMTS-side PNM (upstream RxMER, spectrum capture).
- **Cable modems:** DOCS-PNM-MIB over direct SNMP to the modem, where per-subcarrier data originates (full-band capture, RxMER per subcarrier, pre-equalization). The modem itself uploads result files as a TFTP client to an endpoint the product hosts. This needs a network path to and from the modem subnets, which a CMTS-only deployment doesn't provide; it then keeps CMTS tables and CMTS-side tests only.
- **Plant actives:** node and amp transponders (SCTE-HMS or DOCSIS-based) for voltage, current, temperature and levels at the device, which localizes powering and gain faults without inference.

**No real hardware is available yet, so a device emulator stands in.** The sim drives an emulated modem and CMTS that answer SNMP and upload real PNM file formats over TFTP, and v1's one SNMP and TFTP adapter reads them. Telemetry then travels the same path as it would from a real device, so connecting real gear is an integration, not a rewrite. v2 adds an API-based vCore adapter for DAA when access arrives, a legacy I-CCAP adapter such as cBR-8 or E6000, and further breadth against the contract. A collector-feed adapter is available as a fallback for operators that will not grant management-plane access, but it is not the primary path.

**Polling is twin-driven.** At Tier 1 scale, blanket PNM collection is a scheduling problem and a CMTS load problem. The adapter layer enforces budgets and rate limits per CMTS and per modem (one test and one upload at a time, four stored files per modem), and the diagnoser chooses which modem to poll next as the measurement that most reduces its uncertainty (active sensing). Targeted polling is cheaper on the CMTS than blanket collection, which is itself a selling point.

**Fault classes injected into the sim:**

| Fault | Physical parameters | Physical effect | PNM signature |
| --- | --- | --- | --- |
| Water ingress in hardline | Span, position, affected length, loss slope vs frequency, rainfall coupling, impedance variation | Frequency-dependent loss and impedance changes along the wet length, worsens with rain | Non-periodic full-band-capture waves and low RxMER, correlates with weather, cluster downstream of one span, upstream often clean |
| Corroded or loose connector | Position along cable, return loss | Impedance mismatch, micro-reflection | Pre-eq coefficient energy at one delay; group delay ripple |
| Suckout | Center frequency, notch depth and width | Notch at one frequency, with group delay; causes include grounding, craft and manufacture | Localized RxMER dip at the same subcarriers across neighbors |
| Frequency-selective loss | Location, shape (band-stop, high-pass, low-pass), band edges, depth, stop-band return loss | Traps, filters, pullout, under-rated parts | Missing or sloped band with a sharp edge below one point; echoes near the edge |
| Amp failure or drift | Gain delta, tilt delta, compression onset, onset profile | Gain loss, excess tilt or compression | Step change in level for every modem downstream; nothing upstream of it |
| Upstream ingress | Entry point, frequency, power, time-of-day profile, burst rate and duration | Noise on return path | Elevated upstream noise across the service group, time-of-day pattern; localized by switching legs, not from modem data |
| Downstream ingress | Entry point, frequency, bandwidth, power, time-of-day profile | Over-the-air signal such as LTE coupling in through a shield break | RxMER dip inside the interfering band, only on modems downstream of the entry point |
| Power supply fault | Output voltage sag, powering zone | Voltage drop, amp brownout | Intermittent partial service across one powering zone |
| Bad drop | Drop loss, single home | Loss on one home only | Single-modem symptoms, neighbors clean |

**Diagnosis is the inverse of the sim.** The sim maps fault parameters to telemetry; diagnosis asks which fault parameters best explain the telemetry observed. One inference problem answers three questions, in order: **who** is affected (which modems, how many, how severe), **what** the cause is (which component, where, which parameters), and **how** to fix it (the work order, from the policy table).

- **Prior:** plant topology. Faults live on components, and a fault on a span affects everything downstream of it. This is what a rules engine would encode, expressed as structure rather than rules.
- **Likelihood:** the sim itself. A candidate fault is scored by simulating it and comparing to what was observed.
- **Amortized inference:** a model trained on sim samples (across many generated layouts) proposes candidate fault parameters directly from telemetry features, so the sim only has to verify the top few. Which model is decided at model selection, below; the architecture does not depend on it.
- **Fine-tuning:** real work orders with telemetry-verified outcomes adjust the proposal model; the sim and topology prior do not change.

Output is a posterior over candidate faults: for each, the component, its parameters, confidence, affected-customer count, a map pin, and the explanation "simulated telemetry under this hypothesis, beside what we saw." Compound faults follow from the method, and uncertainty is a posterior, though it still has to be measured for calibration. The same inference drives prioritization and triage across the plant, with operator-supplied weights (service mix per node, crew availability) kept separate from the learned part.

**Proposal model architecture.** The plant is a graph with telemetry on the leaves and faults on the edges, so the natural model is a graph network rather than a per-modem classifier:

1. **Subcarrier encoder** per modem: a small 1D transformer or CNN over the subcarrier axis (normalized RxMER margin, full-band capture, pre-eq taps from the feature layer) produces a modem embedding, replacing hand-built spectral features.
2. **Graph network** over the plant (modems, taps, amps, spans, node): attention-based message passing propagates modem embeddings up the tree and plant context back down. Sibling deviation becomes something the model computes, and localization is learned rather than ruled.
3. **Fault heads** per component: fault class and physical parameters, so compound faults and direct localization are outputs, not post-processing.
4. **Temporal layer** (later): a transformer over time per component for onset detection and weather-coupled trends.

GNNs are inductive over graph structure, which is the layout-agnostic property built in, and the sim supplies the unlimited labeled graphs that make a deep model viable where labeled data would otherwise be the limit. This is neural posterior estimation for simulation-based inference.

**Model selection** is a stage, not an assumption. The no-ML diagnoser from step 5 is the floor every candidate must beat. Candidates: GBDT on hand features (shadow-mode comparator), GBDT on encoder embeddings, GNN as above, and GNN plus temporal layer. All are trained on the same sim-generated data and evaluated on held-out layout families the model has never seen (held out by density, cascade depth, profile, aerial vs underground, legacy vs DAA, never by random sample), on localization accuracy, parameter error, calibration, compound-fault recall, and inference latency per service group. The winner ships as an exported model that runs in-process from Rust, with Python only for training (`CLAUDE.md` invariant 6); the baselines stay as comparators. Training randomizes across the uncertainty in our `approx` values, and a sensitivity report says which of them change a diagnosis, which is also the list of what to measure first once data exists. Selection is only meaningful once the sim passes its validation suite (`docs/physics.md` §12). In v2 the first real plant is the final held-out set, and its residuals against the sim-trained model are the headline validation number.

## What AI does and does not do

**Does not:** place nodes, taps or amps; compute RF or power; decide the fix; write the work order, which comes from a template over the policy table. Those are deterministic and must be reproducible. Topology validation uses the sim as its likelihood, so it is statistical and explainable, not a learned model.

**Does:** propose fault hypotheses for the sim to verify (amortized inference trained on sim samples, fine-tuned on work orders), and prioritize and triage across the plant. Outside the runtime, an offline import tool may use an LLM to map free-text tickets into the cause ontology (v2), and its versioned output is the input. The operator's own agent can explain and rephrase anything through MCP.

**No bundled chatbot.** The product exposes the twin as an **MCP server**: tools for plant graph queries, telemetry queries, fault history, sim controls (inject fault, re-run, what-if), and the optimizer. The operator points their own Claude session (or any MCP client) at it. The product's own web UI is an MCP client too: there is no parallel REST API, so anything the UI can do an operator's agent can do. This is cheaper than a built-in agent, keeps the user's existing AI workflow, and makes the product AI-native without owning a model.

The tool is itself built with AI; that is part of the pitch but not a product feature.

## Data contract and label strategy

Models are insulated from data change by a contract, and from label drift by learning only what is physically stable. Retraining is a pipeline, not a project.

**Observation and feature contracts**

- There are two layers. The **observation schema** is what the sim emits per modem, per active and per service group, in absolute units, and adapters map vendor data into it. It has one owner, the sim, and cannot drift from it.
- The **feature layer** is a pure function of observation, plant and profile, built at step 4, and it's all that models read. Features are relative, not absolute: MER margin against the profile target, deviation from siblings on the same tap or amp, pre-eq reflection delay in feet, codeword error rate per hour, level drift against design level. Arrays stay arrays, normalized the same way (RxMER margin per subcarrier, not raw RxMER), which is what the spectrum encoder reads. They hold across profiles and hardware generations.
- Raw CMTS fields map in through per-vendor adapters with contract tests. A vendor or firmware change is an adapter fix, not a model change.
- Capability sets differ by modem and CMTS, so the proposal model handles missing features natively (GBDT) and trains with feature dropout across profiles.
- The schema is versioned; every model records the schema it was trained on; new models run in shadow mode before promotion.

**Label strategy**

| Question | Source of truth | Learned? | Why |
| --- | --- | --- | --- |
| Who is affected | Telemetry + topology | No | Computed; no labels needed |
| What the cause is | Inverse of the sim (physics) | Yes | Physical phenomena do not change with practice or vendor |
| How to fix it | Operator-owned policy table: cause × component → action, parts, crew | No | Repair practice drifts; a table edit beats a retrain |

- Work orders are normalized at import into the cause ontology (an LLM maps free text to cause, component, action). The mapping is versioned and raw ticket text is kept forever, so an ontology change means re-running the mapper and retraining, in minutes.
- Telemetry-verified outcomes (symptoms cleared after work at this location) are the primary label; human-written tickets are weak supervision.
- Sim labels are the anchor. Real labels fine-tune a sim-trained model rather than replace it. Divergence between the sim-only and fine-tuned models is the label-drift signal.
- Calibration is tracked per cause class so drift is localized to one slice.
- A plant change (node split, new amps) re-featurizes automatically because the plant model is the source of truth.

## Build order

The order follows the dependency graph: each step is built on a hand-made input so that the step producing that input for real can come later. Five rules drive it: schemas before the code that reads them; the sim before anything that consumes it; diagnosis without ML before diagnosis with it; one adapter before many; validation before model selection.

| Step | Builds | Needs | Why here |
| --- | --- | --- | --- |
| 1 | Schemas: plant graph with record status and parts list, architecture profile, component library, environment, observation, fault parameters, work order, policy table. Low-split I-CCAP and high-split RPD profiles | Nothing | Everything downstream reads these. Versioned, so a change is a migration, but a wrong shape is expensive |
| 2 | Plant graph crate with one hand-built street (node, two amps, taps, drops) | 1 | Source of truth for all derivation; a hand-built plant lets the sim be validated without an optimizer or OSM |
| 3 | Physics sim: forward path, noise, return path, AC power, on both first profiles | 2 | The likelihood, the telemetry generator and the training-data source in one; the most load-bearing component, so validate against known level budgets first, measure the per-service-group latency budget here, and follow `docs/physics.md` §11 so later profiles don't force a rewrite |
| 4 | Fault parameterization, modem state, telemetry generator, feature layer and parameter sweep | 3 | Turns the sim into the data factory; all training data depends on it |
| 5 | Diagnosis with topology prior and sim verification, no ML; work-order output via policy table | 4 | Proves the inverse-of-sim loop end to end with zero training data; becomes the floor every model must beat |
| 6 | MCP server, then a minimal map UI as its client | 5 | API before UI forces the tool surface to be designed, and the product is demoable from here. End of v0 |
| 7 | Device emulator and adapter: sim-driven SNMP and TFTP in real PNM file formats, an adapter that reads them, modem binding and pseudonymization; validation suite against published results | 1, 4 | Shapes the adapter interface before breadth and proves telemetry can travel the real path, with no hardware. Gives the sim its only outside check before anything is trained on it |
| 8 | OSM ingest, layout generator, design optimizer, parts lists and plant records | 2, 3 | Design is part of the product, and it supplies layout diversity for steps 10 to 13 |
| 9 | Component library breadth | 8 | Data entry is only valuable once the optimizer places real parts |
| 10 | DAA depth and migration planning: RPD fault classes and observations, per-node comparison between profiles, parts that must change | 4, 5, 9 | Why the twin is useful during a rollout |
| 11 | Work order lifecycle: dispatch, simulated repair, and verification from telemetry that symptoms cleared | 5, 7 | Closes the loop that no separate system can |
| 12 | Training data generation at scale; model selection (GBDT, GNN) on held-out layout families; sensitivity report | 4, 7, 8 | Needs many diverse layouts and a sim that passes validation; this is why ML comes this late |
| 13 | Reactivity: incremental recompute, impairment workbench, the lifecycle demo at region scale; full map UI | 6, 8, 10, 11, 12 | The demo surface; built on a proven core rather than driving it. End of v1 |

v2 and later steps get specs when v1 is close, since a pilot will change them.

## Milestones

The numeric targets are proposals, to be reviewed once step 3 has measured real behavior.

**v0 — walking skeleton**

- [x] One street, one node, one fault class (connector or reflection), end to end: hand-built plant in, work order out, queried through MCP, shown on a minimal map
- [x] Works on both first profiles, with D3.0 and D3.1 modems on one tap
- [x] Fixes the feature, telemetry and work-order schemas before any breadth work
- [x] **Bar:** every `docs/physics.md` oracle passes; byte-identical reruns; the faulty component ranks first in at least 90% of seeds and positions tried on the fixture

**v1 — the whole lifecycle on a simulated region, for an operator's data science and PNM engineers**

- [x] Device emulator and one SNMP and TFTP adapter; pseudonymized at ingest; validation suite against published results. CM test OIDs in `pnm_wire::oids` are placeholders
- [ ] OSM ingest and layout generator for a chosen region; optimizer producing valid designs with parts lists (server-side; WASM for interactive what-ifs); plant records. **Partly:** the generator, optimizer, parts lists and records are done, and a what-if is fast enough without WASM. OSM ingest now runs on real extracts and any drawn area (Task 14), and no demo region is chosen. Since steps 21 to 24 a second designer, `design::v2`, designs plant as a field build has it, on rules as data: premises, fittings, structures and fiber, greenfield or a legacy plant upgraded in place, with a cost per home (Task 23). Since step 25 it designs any real area drawn on the page with no issue on any node it builds: 30 towns and twelve fresh areas, both profiles, apartment blocks and cut-off neighbourhoods included, all synthetic plant
- [ ] Component library for both first profiles, populated from spec sheets. **Partly:** 24 parts have datasheets (`fixtures/library/sourced.json`). The other 196, outdoor taps, drops, nodes, RPDs, modems, premises parts, plug-ins and fiber among them, are `Generic`
- [x] RPD fault classes and per-node migration planning between low-split I-CCAP and high-split RPD. Plans report what a migrated layout fails; moving amps to fix it isn't automated
- [x] Rust sim: forward path, noise, return path, AC power; domain randomization and compound faults; since steps 21 and 22, premises, fittings, plug-in alignment, connector and splice reflections, ambient pickup, and fiber optics from launch to receiver
- [x] Telemetry generator in PNM schema; nine fault classes as physical parameters, and since step 24 in-home wiring, a plug-in set wrong, and three on the fiber (cut, loss, transmitter drift); weather coupling; impairment workbench on the map
- [x] Diagnosis as inverse of the sim: topology prior, sim as likelihood, proposal model selected from GBDT and GNN candidates on held-out layouts; policy table for fixes; work-order output with posterior confidence; sensitivity report (`docs/model-selection.md`). Since step 24 on as-built plant too: candidates at fittings and in homes, the scope rules, `diagnosis::fiber` at the region, and a model for each kind of plant, the GBDT for both by the same rule
- [x] Work order lifecycle, with verification from telemetry
- [x] Map UI (MapLibre) with design, fault, and telemetry layers
- [x] MCP server over the twin, with subscriber identity gated behind explicit authorization
- [x] Incremental recompute per service group, running from Docker Compose
- [ ] **Bar:** the §12 validation suite passes; on held-out layout families, the faulty component in the top 3 at least 80% of the time, beating the no-ML floor; calibration error under 5%; the lifecycle demo runs live on a region of 20–80 nodes. **Not fully met**, all on synthetic plant:
  - Validation: passes except NMTER at −10 dBc, which misses its tolerance by about 0.1 dB (`docs/validation.md`).
  - Top 3: met on all three held-out families (85 %, 90 %, 81 %), beating the floor. Rural's and UrbanGrid's intervals reach below 80 %.
  - Calibration: **missed on all three** (5.4 %, 7.5 %, 8.1 %).
  - Demo: met through the tools on 32 nodes, and through the page on 16. A diagnosis with the model takes about 13 s on a node.
  - Since step 26 a designed region also runs day to day: hazards as physical faults at sourced rates, households that call, a
    call centre and trucks on the streets, dispatched from calls or with the twin, side by side on one seed. All synthetic.
    Since step 28 it plays as a game screen: both worlds on synced maps, trucks driving their routes, calls ringing, the
    sky from the world's environment, disasters from a toolbar, and scenarios written ahead as the user's own actions.
    Since step 29 the whole page is laid out that way: one top bar with the modes, a tool strip, the map as the stage and a side
    panel. Operate is a mode, and Design walks three numbered steps from a place to a running town.
  - On as-built plant (step 24), held-out maps: top 3 **74.0 %** for the GBDT and 75.1 % for the GNN through the hybrid, **under 80 %**, beating the floor by 16.9 points; calibration **missed** (10.5 %, 8.7 %). The model alone reaches 80.3 % and 88.8 %: here the sim's verifying costs top 3.

**v2 — read-only pilot on real plant, when data is available**

- [ ] Import plant from their GIS, or from shapefile, KMZ or CSV; a minimal as-built capture where records fall short
- [ ] Terraform deployment into the operator's cloud account, read-only and shadow mode only
- [ ] An API-based vCore adapter; one legacy I-CCAP adapter such as cBR-8 or E6000; plant-active transponders (SCTE-HMS); per-CMTS and per-modem polling budgets
- [ ] One inference engine for three jobs: diagnosis, sim calibration against real measurements (with a residuals report), and topology validation (which plant structure best explains correlated telemetry; disagreements with records are flagged, never silently trusted)
- [ ] Shadow-mode diagnoser against the operator's existing process
- [ ] Tech feedback loop: what was found and done, feeding labels; import work-order history and fine-tune the proposal model on telemetry-verified outcomes
- [ ] CPNI review
- [ ] **Bar:** first calibration on real data, with levels within 2 dB and RxMER within 3 dB of measurements; the rest agreed with the operator before the pilot starts

**v3 — production, one function at a time**

- [ ] Take over functions in the order the operator chooses, starting where the pain is worst
- [ ] Multi-headend and multi-region twin; sharding by CMTS and service group
- [ ] SSO, RBAC, audit logging
- [ ] Security review package: questionnaire, scanning, data-flow documentation
- [ ] Adapter breadth: Casa, Vecima and the remaining vendors; collector-feed fallback
- [ ] Ticketing and GIS integrations
- [ ] D4.0 ESD and FDX profiles, as operators' plans require
- [ ] Field-tech mobile view of the work order

**Later — small and rural ISPs**

- [ ] Opinionated UX with a hosted stack per tenant; address-list and parcel upload as the homes-passed source; free pilot with one operator

## Risks and open questions

| Risk | Why it matters | Mitigation |
| --- | --- | --- |
| Plant records are as-designed, not as-built | Diagnosis localizes by topology; wrong topology gives the wrong cause | Topology validation is the same inference engine as diagnosis (v2); flag disagreements rather than trust records. The twin now designs and diagnoses plant as built, with premises, fittings and fiber (steps 21 to 24), on synthetic plant only. Inside a home it can't yet place a fault: a fault in one home is put in that home 16 to 20 % of the time, against a bar of 80 % |
| The sim is unvalidated against real plant | No real data is coming soon, so every result, and any model trained on it, rests on the sim's assumptions. This is the first thing a data scientist will ask about | Validation suite against published results; randomize across `approx` values and report which change an answer; say so plainly; first real data calibrates in v2, and fine-tuned vs sim-only divergence is the signal from then on |
| Subscriber PII and CPNI | Modem MACs tie to names and addresses | Pseudonymize at ingest; twin keyed by plant location; MCP gates identity behind authorization |
| Recommendation liability | Every recommendation is a truck roll | Show confidence and the no-ML diagnoser's answer side by side; shadow mode before anything reaches a tech; tech feedback loop closes it |
| Adapter interface wrong | Every adapter would be rewritten | One real SNMP adapter shapes the interface in v1, a vCore adapter tests it from the API side once access arrives; contract tests |
| Fault physics rests on estimates | Tap and amp values, optical noise, CIN slope and the FEC notch model have no datasheet or spec figure, so telemetry and migration gain are only as good as those guesses | All marked approx or `Generic` in `docs/physics.md`; the sensitivity report ranks them; replace from datasheets first, then real data. Step 24 reran it on as-built plant with step 22's approx values that a part, an input or the plant holds; every estimate moves far more answers there than on version 1 (hardline and drop loss 61 %, active return loss 58 %, tap return loss 37 %, modem implementation MER 27 %), and a ruled world takes top 3 from about 75 % to as low as a third, so these four are what to measure first. Task 22's optical estimates moved under 2 % |
| Scope of a one-stop shop | Every function added competes with depth, and breadth without proof looks like a slide | One plant graph keeps functions cheap to add; each function gets only what the lifecycle demo needs in v1; billing, provisioning and CMTS config stay out |
| Incumbent systems and owners | A pitch to replace systems threatens the people who run them, and nobody can switch everything off at once | Run alongside, read-only; take over one function at a time; describe the problem as fragmentation, not bad work by any team |
| Spec-sheet access | Some vendors gate detailed specs, vCore APIs and RPD PNM documentation behind partner portals | Generic entries flagged as such; operators add parts from invoices; the vCore adapter waits for access through a pilot |
| Selling before validating | Early claims could outrun the evidence | Show the validation suite and sensitivity report; label estimates as what-ifs; never present synthetic results as field results (`CLAUDE.md` invariant 11) |
| Solo-dev scope | v1 is large: an optimizer, plant records, migration planning, model selection and a full UI. This plan keeps it believable only by cutting depth | v0 first. If time runs short, a GBDT ships before a GNN, the region shrinks, and the full UI gives way to the lifecycle demo's path through it |
| OSM data quality | Missing buildings make poor layouts and designs | Parcel data for the chosen region and address-list upload |
| Management-plane access | Direct-to-hardware needs a path to modem subnets and SNMP write to PNM test objects, which a Tier 1 security team must approve; this gates any deployment | Deploy into the operator's own cloud account from day one, least-privilege SNMP, TFTP endpoint inside their VPC; CMTS-only and collector-feed fallbacks when access is refused |

**Open questions**

For an operator, in rough priority:

- [ ] Which systems would this replace, which do the most damage, and which function should it take over first?
- [ ] Who owns those systems and PNM analytics today, and what would they need to see?
- [ ] What are the low-split and high-split plant details: top downstream edge, cascade depth, node sizes, and whether split and node type change together?
- [ ] What would make a read-only shadow pilot possible later, and who has to approve it?
- [ ] Can vCore API access be arranged, and which RPD PNM features are available?

For us:

- [ ] Which Front Range town or area is the demo region? Needs a mix of aerial, underground and rural fringe.
