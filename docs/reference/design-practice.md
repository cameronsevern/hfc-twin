# HFC design practice

How a coax plant is laid out and specified in the field: levels at the tap and the home, tap and passive families, amplifiers by role, cable by role, nodes and segmentation, powering, structures, premises wiring, upgrades in place and construction cost. The physics of the parts is in plant-and-powering.md and `docs/physics.md`; this note is about which parts a designer picks and where they go.

Units: MHz, ft, dBmV (75 Ω), °C. Sources quote inches for clearances, depths and loop sizes and °F for temperatures; we give feet or °C with the source's figure in brackets. Levels are per 6 MHz unless stated. "Analog-referenced" (or "virtual") levels are 6 dB above the digital level actually carried [VOGEL2019 §2].

Tags: a vendor data sheet describes one product, so its numbers are **[field]**: real and checkable, not a rule. A vendor installation manual's recommendations are **[guideline]**. Data sheets are cited by table name, since most have no numbered sections. Several publicly hosted ATX sheets carry "Confidential and Proprietary" footers and are not used.

## 1. Levels at the tap and the home

**How a design states levels.** Design maps give each amplifier's input and output levels at an upper and a lower reference carrier, with the pad and equalizer values the station takes [SCTE 180 §13.2.2.3, §13.3.1.1] [guideline]. The technician sets the upper carrier with the pad and the lower with the equalizer, then checks against the map. Downstream taps are nominally set to one level, within tap-value steps [SCTE 183 §16.4] [guideline] (plant-and-powering.md §6). An amp's AGC is specified for a preceding span whose coax loss at 1794 MHz equals the amp's operational gain [SCTE 279 §7.4.4] [normative], so a span is designed as one gain's worth of loss at the top frequency [inferred].

**Device-level targets.**
- A device input of 0 dBmV analog-referenced is typical, with digital signals 6 dB or more below; it may be lowered at higher frequencies at the cost of modulation order there [VOGEL2019 §2] [field].
- The CM window and SCTE 40 terminal levels are in plant-and-powering.md §6 and §10. For analog, the FCC asked for ≥ +3 dBmV at the end of a 100 ft drop [SCTE 183 §1].
- No public document states a tap-port design level as a number, nor its tolerance, nor a limit on tilt at the port (see Gaps).

**A worked tap run** [VOGEL2019 §3 Figure 7] [field], which the paper calls "typical", on a legacy 5-42/54-1002 MHz plant:

| Item | Value |
|---|---|
| Source | Node or last amp, 54 dBmV analog-referenced at 1002 MHz |
| Feeder | PIII 625 (QR 540), six 4-port taps, drawn 150 ft apart |
| Tap values, first to last | 29, 26, 23, 20, 14, 8 dB |
| Behind each port | 100 ft series 6 drop, a 4-way split, 50 ft series 6 in the home |
| Device level, first to last | 4.4, 3.1, 1.7, 0.1, 0.6, 1.1 dBmV, against a 0 dBmV target |

The run steps down 3 dB per tap, then 6 dB at the end, and ends on the lowest 4-port value [inferred from the figure]: the "step down toward the end of the line" the realism review called practice.

**Premises allowance.** Removing one split level (8 to 4, or 4 to 2) saves about 4 dB, and a gateway that ends coax at the entry removes 10+ dB of in-home loss [VOGEL2019 §2] [field]. VOGEL2019's premises, priced from the SCTE ceilings [inferred]:

| MHz | 100 ft series 6 [SCTE 74] | 4-way, class [SCTE 153] | 50 ft series 6 | Ground block [SCTE 129] | Total, maximum |
|---|---|---|---|---|---|
| 1002 | 6.56 | 8.6 (1) | 3.28 | 0.05 | 18.5 dB |
| 1218 | 7.26 | 8.9 (2) | 3.63 | 0.05 | 19.8 dB |
| 1794 | 8.96 | 14.0 (3) | 4.48 | 0.10 | 27.5 dB |

A home run with no splitter (the D4.0 case, §9 below) is 9.1 dB at 1794 MHz on 100 ft of series 6 [inferred]. Vendor 1.8 GHz 4-ways measure 9.2-10.0 dB at 1218-1800 MHz, far under the 14.0 dB class 3 ceiling [ABS-180X; ANX-CMC5000 IL tables] [field].

**End of line.**
- Every tap family has a terminating value, the lowest for each port count, with no output port (§2) [CS-FFTQ; TX-OTTZ; TEL-4RTP; ANX-MHT] [field]. A self-terminating tap is a splitter with no coupler [PNMP §3] [guideline] (plant-and-powering.md §4).
- Near the end of the line, operators asked for unconditioned taps: a higher level, even negatively tilted, beats a flatter, lower one [ATX-ANW1544] [field].
- Isolation is specified with all ports terminated, and an unterminated low-value port reflects [PNMP §5.2.1.2] [field] (plant-and-powering.md §4). No source gives a minimum end-of-line level beyond the device target.
- Upstream, low-value taps at the end widen the modem transmit range; avoiding them on new builds is one remedy [SCTE 183 §16.4] [guideline] (plant-and-powering.md §7.5).

## 2. Taps

**Standard** [SCTE 264] [normative]:
- 2-, 4- and 8-way taps (§1.2); a housing with 5/8-24 KS ports plus a field-replaceable faceplate (§5.2). The housing is specified to 3000 MHz and the faceplate to ≥ 1794 MHz (§1.2, §7.1, §7.2).
- With the faceplate off, a make-before-break bypass carries ≥ 15 A with < 7 ms of AC interruption (§6.1). Faceplate ≥ 12 A continuous; housing and bypass ≥ 15 A; at 30-89 VAC quasi-square, −40 to +60 °C (§6.2). A power-extracting F port passes ≥ 2 A (§6.3).
- Direction is set by a reversible plug-in coupler, or by fixed left-to-right and right-to-left faceplates (§6.5). Plug-ins may be offered for conditioning: cable equalizers and cable simulators for tilt (§6.7).
- Published: maximum and typical insertion loss and tap loss at each split's band edges from 5 to 1794 MHz (§7.2 Table 2), and minimum return loss, output-to-tap and tap-to-tap isolation (§7.2.3-§7.2.5). Passband within ±0.5 dB of a linear trend (§7.2.6); shielding ≥ 120 / 110 / 100 dB to 1002 / 1218 / 1794 MHz (§7.4).

**Families on the market**, values in dB, T = terminating, read from each sheet's values table [field]:

| Sheet | Band, MHz | 2-port | 4-port | 8-port | Current |
|---|---|---|---|---|---|
| CS-FFTQ (1.2 GHz) | 5-1218 | 4T, 7, 10, 12, 14, 17, 20, 23, 26, 29 | 7T, 10, 14, 15.5, 17, 20, 23, 26, 29 | 10T, 14, 17, 20, 23, 26, 29 | 12 A |
| TX-OTTZ (1.8 GHz) | 5-1800 | 4T, 8, 11, 14, 17 … 29 | 8T, 11 … 29 | 11T, 14 … 29 | 15 A |
| TEL-4RTP (1.8 GHz) | 5-1800 | 4T, 8 … 29, 32, 35 | 8T … 35 | 11T … 35 | 15 A |
| ANX-MHT (2 GHz) | 5-2000 | 4T, 8 … 29 | 8T … 29 | 11T … 29 | 15 A |

Steps are 3 dB, apart from CS-FFTQ's older 7/10/12/15.5 values.

**Insertion loss by value.** Lower values lose more through, and more at the top of the band [inferred from all four sheets]. TX-OTTZ-2, typical in-to-out loss in dB [TX-OTTZ IL table] [field]:

| Value | 8 | 11 | 14 | 17 | 20 | 23 | 26 | 29 |
|---|---|---|---|---|---|---|---|---|
| 5 MHz | 3.0 | 2.0 | 1.0 | 0.8 | 0.5 | 0.6 | 0.4 | 0.3 |
| 1218 MHz | 4.3 | 2.8 | 1.8 | 1.7 | 1.3 | 1.0 | 0.9 | 0.9 |
| 1800 MHz | 4.9 | 3.8 | 2.4 | 2.3 | 1.6 | 1.5 | 1.2 | 1.1 |

The same value loses more in 4- and 8-port faceplates: at 1800 MHz, OTTZ-4-11 is 4.7 dB and OTTZ-8-14 is 5.0 dB [TX-OTTZ 4- and 8-way tables] [field]. At 1218 MHz, CS-FFTQ runs from 4.0/5.2 dB typical/maximum (2-port 7) to 1.5/2.0 dB (29) [CS-FFTQ IL table] [field].

**Tap loss is not the nominal value** [field]. Low values in 4- and 8-port faceplates read high: OTTZ-4-11 measures 13.0 dB and OTTZ-8-14 15.5 dB [TX-OTTZ], and FFT8-14Q's nominal is 14.5 dB [CS-FFTQ]. Low values also rise 1-2 dB above 1218 MHz: OTTZ-2-8 is 8.8 dB low and 10.0 dB at 1218-1800 [TX-OTTZ]. Tolerances are ±1.0 dB, or ±1.5 dB above 1218 MHz (Technetix), and ±1.0 / ±1.5 / ±2.0 dB by band (Teleste) [TX-OTTZ; TEL-4RTP].

**Isolation, return loss, hum** [field]. Output-to-tap isolation is 16-25 dB (OTTZ) and 20-41 dB (4RTP), higher for high values; tap-to-tap 16-26 dB [TX-OTTZ; TEL-4RTP]. Return loss is 12-18 dB, falling to 14 dB above 1218 MHz on 4RTP; one Antronix port reads 6 dB at 5-12 MHz (MHT2220) [ANX-MHT]. Hum ≥ 60 dB at 10 A [TX-OTTZ; CS-FFTQ]. All of this fits the 15-18 dB new-tap return loss in plant-and-powering.md §4.

**Plug-ins at the tap.** The plug-in sits after the coupler and before the port splitter, so it acts on all ports and not on the through path [CS-TQ; CS-FFTQ block diagram] [field]. Each vendor states its value at a different frequency, so a value means nothing without its family [inferred]:

| Family | Values, dB | What the value is |
|---|---|---|
| CS T-EQ (1.2 GHz) | 2-22, 2 dB steps | Nominal at 1003 MHz; T-EQ-10 loses 9.2 dB at 5 MHz, 1.1 dB at 1218 MHz |
| CS T-CS (1.2 GHz) | 2-12 even, 15, 18, 21 | Loss at 1218 MHz; T-CS-10 is 0.1 dB at 5 MHz, 11.0 dB at 1218 MHz |
| CS T-RPA/42, /85 | 2-18 | Return-only attenuation over 5-42 or 5-85 MHz |
| ANX CEH / CSH (2 GHz) | 2-16 even / 3, 6, 9, 12 | Equalization or simulation at 1794 MHz; a jumper ships in the slot |
| TX TXPI-EQZ / CSZ (1.8 GHz) | 2 … 22 / 2 … 21 | The 5 to 1825 MHz loss difference [inferred from the table] |

Sources: [CS-TQ; ANX-MHT E-Option; TX-TXPI] [field].

## 3. Line passives and how branches are made

**Standard** [SCTE 265] [normative]:
- Hardline splitters (2- or 3-way), directional couplers, equalizers and power inserters; housing plus faceplate, to ≥ 1794 MHz in a 3000 MHz housing (§1.2, §8.1, §8.2). 3-way splitters are balanced, or unbalanced as two high legs and one low (§7.9 Table 2).
- ≥ 15 A on every port, except 2-port inline equalizers in a tap housing (≥ 12 A) and **power inserters: ≥ 20 A on the AC input and ≥ 15 A on each RF/AC port** (§7.2). AC passes between all ports by default and is removable per port with a finger-replaceable shunt (§7.3).
- Published: maximum and typical insertion loss, minimum return loss and isolation at the Table 3 points; passband within ±0.5 dB of the device's slope (§8.2, §8.2.1).

**Products**, dB at the top of each band, typical/maximum [field]:

| Sheet | Couplers | Coupler through loss | 2-way leg loss | Power inserter IL |
|---|---|---|---|---|
| CS-SSPQ (1218 MHz) | 7, 9, 12, 16 | 3.4/4.2 (7) … 1.9/2.8 (16) | 5.0/6.3 | 0.9/1.6 |
| TX-LH100 (1800 MHz) | 8, 12, 16 | 4.1/4.5 (8) … 2.7/3.1 (16) | 5.4/6.0 | 1.4/1.8 |
| TX-XG (1825 MHz) | 8, 10, 12, 16 | 4.2 (8) … 2.6 (16), typical | 5.8 | 0.7 |
| TEL-4XP (1800 MHz) | 8, 12, 16 | 4.5 (8) … 3.0 (16), maximum | 6.5 | 0.8-2.2 |

CS-SSPQ's only 3-way is unbalanced (3/6/6); TX-LH100's unbalanced 3-way loses 9.6/10.6 dB on two legs and 5.4/6.0 dB on the third at 1800 MHz. Inserters: CS-SSPQ is 20 A common and 15 A per leg; TX-LH100's AC port is 20 A; TEL-4XP states only 15 A, short of SCTE 265. Line equalizer CS-FFE120 ships with 8 or 12 dB of forward EQ (FFE-8: 8.5 dB at 54 MHz, 1.6 dB at 1003 MHz), takes a JXP return pad (0-26 dB) and passes 12 A [CS-FFE120] [field].

**Branching at an active.** A bridger station splits its own output internally [SCTE 183 §16.4 Figure 16-13] (plant-and-powering.md §5). Current amps set the extra outputs with a plug-in [field]:

| Amp family | Driven outputs | What makes the third (or fourth) output |
|---|---|---|
| CS MB120 / MB180 mini-bridger | 2; Port 2 is main | SP120 splitter (equal Ports 3 and 4, ≤ 5.0 dB each, 17 dB isolation), or a keyed DC120/8, /10, /12 coupler (through 3.3 / 2.7 / 2.2 dB, tapped 9.0 / 10.5 / 12.5 dB maximum) |
| Cisco GainMaker HGD | 2 (main, aux) | Signal director: jumper, 2-way splitter, DC-8 or DC-12 |
| TX DBT-1800-C2 | 2 (MAIN, AUX) | Director: DC 8 or DC 12 on AUX1 or AUX2, jumper, or splitter |
| Cisco HGBT, TX DBT-1800-C3 | 3, equal high level | None needed |
| TEL ICON4300 | 3 | Each output on, −6 dB or off in software |

Sources: [CS-SP120; CS-MB120; CISCO-HGD; CISCO-HGBT; TX-DBT1800-C2; TX-DBT1800-C3; TEL-ICON4000]. Away from an active, an unequal branch takes a coupler and an equal one a splitter [inferred]; no source states a rule for choosing.

## 4. Amplifiers by role

**Types** [SCTE 279 §6] [normative]:

| Type | Definition |
|---|---|
| Multiport (system amp, bridger, trunk/bridger) | High gain; ≥ 2 outputs driven by independent power amplifiers; strand or pedestal; line powered |
| Line extender | High gain; 1 or 2 outputs from a single power amplifier |
| Booster | 1 output, smaller, lower gain and power; between legacy sites with very long spacing; can sit in a tap or passive pedestal (§12.1) |
| Compact | 1-3 outputs, ports down, cabinet mounted; bridger or line-extender configurations |

By §6.1, a mini-bridger with two driven outputs is a multiport amp [inferred from CS-MB180].

**1.8 GHz requirements** [SCTE 279] [normative unless tagged]:
- Mid, high, UHS-396 and UHS-492 splits shall be supported; sub, euro and UHS-684 should be (§7.1.1). Diplexers are field-changeable plug-ins and may be remotely switchable (§7.1.2).
- Gain is not specified, but 48-50 dB at 1794 MHz downstream and 30-32 dB at 684 MHz upstream were expected for high-gain amps; boosters 10-13 dB and 4-7 dB, with 0, 2, 4 or 6 dB input attenuation (§7.2.2, §7.3.2, §10.2.2, §10.3.2, §10.4.1) [guideline].
- **No plug-in pads, equalizers or cable simulators**: all three, and internal slope, are electronic in ≤ 0.5 dB steps (§7.2.3, §7.4.1, §7.5.1). One universal interstage slot holds a jumper (§7.4.2). AGC is in plant-and-powering.md §5; test points are −20 dB (§7.2.7).
- Housings at most 18 × 10 × 10 in (multiport) and 12 × 10 × 10 in (line extender) (§9.1.1).
- Through current ≥ 15 A steady, 20 A peak for 2 h (§14.4); low-voltage cut-off with ≥ 3 V hysteresis (§14.3); current and power tables in 5 V steps (§14.5, §14.6); power factor should exceed 0.95 (§14.9.2). SCTE 150 is the older line-extender sheet template, with AC current at 87 and 60 VAC and a rated low-voltage limit [SCTE 150 §5.0].

**Examples**, power at 90 VAC [field]:

| Sheet | Role | Downstream gain, tilt | NF, dB | Return gain | Power |
|---|---|---|---|---|---|
| CS-BLE180 | Line extender, 1 output | 43 dB at 1794; default tilt 12.9 dB, 102-1794 | 9 / 11 | 27-32 dB by split | 31 W, 0.7 A |
| CS-MB180 | Mini-bridger, 2 driven | 50 dB; default tilt 21.9 dB | 9 / 11 | 27-32 dB | 45 W, 0.9 A |
| TX-DBT1800-C2 | Dual system amp | 49 dB at 1794; mainboard tilt 9 dB | 10 | 28 (204), 32 (684) | 42.8 W, 0.5 A |
| TX-DBT1800-CC1 | Line extender | 46 dB at 1794 | 10 | 25-32 dB | 27.0 W, 0.40 A |
| TEL-ICON4000 (4300) | 3-output system amp | 3 × 50 dB nominal; interstage slope 11-26 dB | 10 | ≤ 30 dB | 57 W |
| CS-BLE120 (1.2 GHz) | Line extender | 38 dB; internal slope 10.4 dB (42 split) | 7.0 / 9.0 | 24 or 27 dB | 24.4 W, 0.51 A |
| CISCO-HGBT (1.2 GHz) | 3-output | ≥ 46 dB; internal tilt 19.0 dB | 8.5 / 8.0 | 21.6-27.2 dB | 49.0 W, 0.66 A |

TX-DBT1800-C2 sets everything electronically in 0.5 dB steps: forward input attenuation and EQ 0-20 dB each, interstage attenuation and EQ 0-15 dB each; reverse input attenuation 0-15 dB, interstage attenuation and EQ 0-15 dB each [TX-DBT1800-C2] [field].

**Reference output levels** show what a design's output levels look like [field]. CISCO-HGBT: 40.2 / 43.3 / 47.8 / 50.8 / 54.7 / 58.0 dBmV at 54 / 258 / 550 / 750 / 1002 / 1218 MHz, a 17.8 dB linear tilt over 54-1218 MHz, and 35 dBmV return output. CS-BLE120 all-digital: 49 / 39.2 / 32 dBmV actual at 1218 / 550 / 54 MHz from an input of 11 / 5.9 / 4.4 dBmV. Nodes are in §6.

**Plug-in families on legacy and 1.2 GHz stations:**
- **Pads** [SCTE 288 §7 Tables 1-2] [normative]: 0-5 dB in 0.5 dB steps, 6-20 dB in 1 dB steps, and fine values 0.25-4.25 dB; rated 5-1250 MHz; return loss 30 dB below 600 MHz, falling to 22-25 dB at 1000-1218 MHz on higher values; tilt within ±0.15-0.25 dB. Vendors: CS NPB 0-26 dB in 1 dB steps plus a 75 Ω terminator [CS-NPB]; GainMaker 0-20 dB in 1 dB steps, as a forward input pad, an AGC pad and four return pads [CISCO-HGBT] [field].
- **Equalizers and simulators** [field]: CS CE-120 (1-21 dB) equalizes PIII-500; its value is the tilt over 54-1218 MHz, with 0.7 dB left at 1218 MHz, and a CS-120 simulator's value is its loss at 1218 MHz [CS-CE120]. GainMaker cable EQs are 0-30 dB in 1.5 dB steps valued "at 1002 MHz", and its inverse EQs 1.5-21 dB in 1.5 dB steps [CISCO-HGBT].
- **Return equalizers** [field]: CS SRE 0-20 dB in 2 dB steps, made per split [CS-SRE] (0-10 dB on BLE120); GainMaker 0-12 dB in 1 dB steps.
- The GainMaker AGC pad is the main-port design output at the pilot frequency minus 34 dB [CISCO-HGBT] [field]. A station's complement is an input pad, an input EQ or simulator, an AGC pad, a return pad per input and a return output pad and EQ [same], matching SCTE 183's station diagram (plant-and-powering.md §5).

## 5. Cable by role, and drops

**Hardline.** Sizes and losses are in plant-and-powering.md §1 [SCTE 15].
- Products [field]: CommScope P3 .500, .565, .625, .700, .750, .840, .875 and QR .540, .715, .860 [CS-COAX-QRG]; Times Fiber T10 .412-1.000 and TX10 .565, .703, .840, 1.160 [TFC-CABLE-BOOK].
- Roles [field]: CommScope lists P3 and QR for "trunk, distribution" with no size mapping [CS-COAX-QRG]. A Times Fiber tension table labels T10 .500 and .625 "feeder" and .750 and .875 "trunk" [TFC-CABLE-BOOK TN 1038 Table 3]. VOGEL2019's tap run is .625, and one upgrade option makes its first section .875 [VOGEL2019 §3]. No source names an "express" size or role; "feeder" here is the tapped cable [inferred].
- Non-foam or poorly performing cable may need replacing above 1 GHz [SCTE 300 §7.4 e; TR-TAR §6.4.1] [guideline].
- P3 .500 / .625 / .750 / .875: static bend radius 6.0 / 7.0 / 8.0 / 9.0 in (0.50-0.75 ft), maximum pull 300 / 475 / 675 / 875 lbf [CS-TD-MANUAL] [guideline].

**Drops.**
- **Size by length:** series 6 "for runs of 150 feet or less", series 11 "for runs of over 150 feet" [CS-DROP-MANUAL selection matrices] [guideline]. Series 6 (no series 59) and series 11 on longer drops, at least tri-shield, messengered aerial and flooded buried [SCTE 300 §7.12] [guideline].
- At 1794 MHz, 1.8 GHz-rated messengered drop loses 8.85 dB/100 ft (series 6) and 5.99 dB/100 ft (series 11) [ABS-T6; ABS-T11] [field], inside the SCTE 74 ceilings (plant-and-powering.md §2).
- **Aerial span**, heavy / medium / light NESC loading, assuming a 21 ft attachment, 7 ft sag and 14 ft clearance: series 6 75 / 100 / 145 ft bare and 90 / 120 / 155 ft messengered; series 11 75 / 100 / 130 ft bare and 130 / 175 / 205 ft messengered [TFC-CABLE-BOOK TN 2015] [field]. Spans over 75 ft typically use messengered drop [TFC-CABLE-BOOK TN 1039-A]; RUS caps an aerial service wire span at 250 ft [RUS 1753F-801] [guideline].
- **Buried:** ≥ 1.0 ft (12 in), or 1.5 ft (18 in) for NEC 830 cable outside conduit [CS-DROP-MANUAL]; 1.0 ft in soil and 3.0 ft at ditches [RUS 1753F-153] [guideline].
- No source states a typical drop length. VOGEL2019 models 100 ft [field], the FCC analog rule's length.

## 6. Nodes, size and segmentation

**Real node areas**, five legacy N+2 and N+3 areas from a 2016 vendor study [ULM2016 Table 3] [field]:

| Node | A | B | C | D | E | Average |
|---|---|---|---|---|---|---|
| Coax miles | 4.2 | 6.2 | 3.5 | 2.5 | 1.9 | 3.7 |
| % aerial | 20 | 77 | 97 | 87 | 91 | 70 |
| Actives | 21 | 30 | 21 | 19 | 14 | 21 |
| Actives per mile | 5.0 | 4.9 | 5.9 | 7.6 | 7.4 | 5.7 |
| Homes passed | 153 | 352 | 398 | 469 | 520 | 378 |
| Homes passed per mile | 37 | 57 | 112 | 187 | 274 | 104 |

**Node C** [ULM2016 case study] [field]. Two 15 A supplies power the whole area; its 398 homes are about 200 subscribers at 50 % take. Segmenting in place from 1×1 to 4×4 averages 100 homes per segment, "often unbalanced between the RF legs". Fiber deep N+0 replaces the node and its 21 amps with 6 nodes, which may need 64 dBmV outputs, and leaves taps 195-1,448 ft from fiber (1,007 ft average). Fiber to the last active instead puts 21 nodes at the old amp sites, with serving groups of 30-60 homes.

**Splits and trend.** VOGEL2019 splits a 1,000-home node into four of 250 [VOGEL2019 §1] [field]. Service groups are trending smaller and toward N+0 [TR-DCA §6.1.2] [guideline]. D4.0 assumes DAA rather than traditional nodes [SCTE 300 §6.1]; some plans keep chains of up to six amps, where AGC is recommended [SCTE 300 §7.6.2] [guideline].

**Node ports and segmentation** [field]:

| Sheet | RF ports | Segmentation (DS × US) | Output per port | Power |
|---|---|---|---|---|
| CS-NC2000 (1.2 GHz) | 2 driven; a 50 % coupler adds a third | 1×1, 1×2, 2×2 | 54 dBmV QAM at 1218, 22 dB linear slope | 10 A per port, 15 A combined |
| CS-NC4000S4 + RD1424 RPD | 4 | 1×1, 1×2, 2×2, 2×4 | 50 dBmV at 1218, 17 dB tilt | 150 W AC typical; 15 A bypass |
| VEC-EN9000 (1.8 GHz RPD) | 4 RF/power + 2 power-only | 2×2 or 2×4 | TCP 70 dBmV; tilt 15-21 dB over 108-1218 | 180 W maximum; 15 A |
| HAR-RIPPLE (Ripple+) | 4 | 1×1 … 2×4 | 58 dBmV QAM at 1218, 21 dB tilt | 130 W typical |
| HAR-SHELL1 (compact) | 3, one or two high-level | 1×1, 1×2 | 53 dBmV at 1218; an equal split of port 2 costs 4 dB | 52-65 W |

CS-NC2000 sets level and slope with plug-in pads (0-22 dB) and EQs (5-22 dB in 1 dB steps). With an RPD the NC4000S4 expects return inputs of 12 / 10 / 8 / 5 dBmV per 6 MHz on 42 / 65 / 85 / 204 MHz splits [CS-RD1424]. No source says how many node ports are used in practice or how homes are balanced across them.

## 7. Powering

**Where things go.** The design fixes supply locations and each active's power-director setting, and the as-built map follows any change [SCTE 180 §13.2.1.1] [guideline]. Pole-mounted supplies sit ≥ 10 ft above ground, and not at a fire-alarm, police or traffic box or a foreign meter [SCTE 180 §8.3] [guideline]. Electric utilities restrict sites further: one bars CATV supplies from poles carrying transformers, regulators, capacitors, reclosers or sectionalizers [WPS-JU10]; another bars communication supplies from its poles and from within 6 ft of one [CLARK-JU2]; a third puts ground pedestals and handholes ≥ 4 ft from a pole base, 6-10 ft preferred [TPU-C-OH-1060] [guideline]. No source gives a rule for placing inserters relative to the load.

**Current limits along the path:**

| Element | Rating | Source |
|---|---|---|
| Tap faceplate; housing and bypass | ≥ 12 A; ≥ 15 A | SCTE 264 §6.2 [normative] |
| Tap power-extracting F port | ≥ 2 A | SCTE 264 §6.3 [normative] |
| Line passive; inline EQ in a tap housing | ≥ 15 A; ≥ 12 A | SCTE 265 §7.2 [normative] |
| Power inserter | ≥ 20 A AC in, ≥ 15 A per RF/AC port | SCTE 265 §7.2 [normative] |
| Amplifier through current | ≥ 15 A; 20 A peak for 2 h | SCTE 279 §14.4 [normative] |
| Node | 10 A per port, 15 A combined (NC2000); 15 A (NC4000, EN9000) | CS-NC2000; VEC-EN9000 [field] |
| Hardline | Ampacity far above these | plant-and-powering.md §3 |

**Supplies and batteries.** Voltages, supply types and constant-power loads are in plant-and-powering.md §12.1. One current family [ALPHA-XM31HP model table] [field]:

| Model | 903 | 905 | 908 | 910 | 915 | 918 |
|---|---|---|---|---|---|---|
| Output, VAC | 60/89 | 60/89 | 63/89 | 63/89 | 63/89 | 63/89 |
| Rated current, A | 3 | 5 | 8 | 10 | 15 | 18 |
| VA | 270 | 450 | 720 | 900 | 1350 | 1620 |
| Battery bus, VDC | 12 | 12 | 36 | 36 | 36 | 36 |

- Quasi-square output, current-limited below 150 % of rating, up to 94 % efficient on line [ALPHA-XM31HP]; default end of discharge 10.2 V per battery, 30.6 V per string [ALPHA-XM3HP-TM] [field]. Technetix TXUPS: 900 VA at 15 / 10 A and 1350 VA at 22 / 15 A, at 63 / 89 VAC [TX-TXUPS] [field].
- Standby comes from three 12 V AGM units per 36 V string, 104-114 Ah at the 20 h rate. At 90 VAC one string runs 204-229 min at 10 A and 100-119 min at 18 A; six batteries at 10 A run 450-487 min [ALPHACELL-HP] [field]. SCTE 183's 90 minutes (plant-and-powering.md §12.2) is one case, not a constant [inferred].

**Design load: the sources disagree.**
- 80 % of rating [SCTE 183 §16.12] [guideline] (plant-and-powering.md §12.1).
- 90-95 % at 25 °C for constant loads; 105-115 % for months, 115-125 % for 30 min, 125-150 % for 10 min, 150-180 % for 10 s, fold-back below 20 A above 180 % [SCTE 287 §7 Table 1] [guideline]. Efficiency drops sharply below 30-40 % of rating, and below 5 % the transformer is unstable [SCTE 287 §7]. The common device is a 15 A supply, often lightly loaded [SCTE 287 §6] [field].
- Near 100 %, since below 75 % efficiency "will fall off dramatically", citing a CableLabs publication [SCTE 180 §13.2.1.1] [guideline].

**What a supply carries.** Node C's 21 actives and 398 homes run on two 15 A supplies (§6) [ULM2016] [field]. At 90 VAC a line extender draws 24-31 W, a mini-bridger or dual 39-45 W and a 3-output amp 49-57 W (§4 table); nodes draw 61 W DC (NC4000S4) and 130-180 W with an RPD [CS-NC4000S4; HAR-RIPPLE; VEC-EN9000] [field]. Minimum input is 45 V (BLE180, MB180) and 40-42 V (DBT-1800), and GainMaker's lockout selects 30, 40 or 50 VAC [CS-BLE180; TX-DBT1800-C2; CISCO-HGBT] [field].

## 8. Structures

**Aerial spans.** No source gives spans for cable plant; these are telephone sources (see Gaps).
- Cost-model pole spacing: 250, 200, 175 and 150 ft, from the sparsest density zones (under 100 lines per square mile) to the densest (2,550 and up) [FCC 99-304 ¶218] [field].
- Maximum span, cable lashed to strand, heavy / medium / light NESC district: 6M strand 400 / 600 / 725 ft at 0.4 lb/ft and 250 / 375 / 425 ft at 1.0 lb/ft; 10M 425 / 600 / 700 ft at 1.0 lb/ft [RUS 1751F-635 Tables 4-6] [guideline]. Spans are set by 14 ft ground clearance at 15.6 °C (60 °F) [RUS 1751F-630].
- Special construction where a span is over 1.5× the average of the five adjacent spans [RUS 1751F-630]. A span lengthened to avoid a driveway stays within 15 % of average, and in towns block length, driveways and property lines set the span [RUS 1751F-626] [guideline].

**Strand, lashing and sag** [SCTE 180] [guideline]:
- Strand 1/4 in or larger, hardware at least Class A galvanized (§8.4), on the same side of the pole as the power neutral and telephone (§8.6). Stringing tension for a 250 ft span at −17.8 / 4.4 / 26.7 °C (0 / 40 / 80 °F): 1/4 in 900 / 700 / 500 lbf, 3/8 in (10M) 2675 / 2300 / 1925 lbf (§8.6 Table 4).
- Suspension clamps take up to 10° of tangent pull and curved clamps 10-20°; beyond 20° the pole is dead-ended and guyed (§8.4, §8.6).
- Cables over .750 are double lashed with a wrap at least every 1.2 ft (14 in); tails of ≥ 6 ft are left at amp, coupler and splitter sites; final sag is 1.5-2.0 % of span at 21.1 °C (70 °F) where telephone is absent or low, otherwise proportional to power (§8.11.5). Overlash only on strand checked for the added load (§8.11.6).

**Expansion loops** on aerial hardline:
- One loop at each pole, on the input and output of every active, and at every tap; two where the span exceeds 150 ft, at street and rail crossings, and in spans with little sag [CS-TD-MANUAL] [guideline]. Times Fiber also loops at every pole and adds one on spans over 200 ft [TFC-CABLE-BOOK TN 1038] [guideline].
- A 1.0 ft (12 in) flat-bottom loop for .625 and smaller and 1.25 ft (15 in) for larger cable [CS-TD-MANUAL]; SCTE 180 also asks 15 in for larger cable (§8.11.7). Loops are ≥ 0.5 ft (6 in) deep, use only 2-3 in of extra cable, take the large size when mixed with large cable, and are formed with a mechanical bender [CS-TD-MANUAL; SCTE 180 §8.11.7] [guideline].
- Loop life depends on sag and depth: a 6 in loop on .500 cable moving 1 in lasts about 88 years, and at 3 in depth .750 cable drops from 55 years to under 3 [TFC-CABLE-BOOK TN 1049] [field].

**Clearances** at final sag [SCTE 180 §8.3] [guideline]: 15.5 ft over roads, 15 ft over driveways and alleys, 10 ft pedestrian-only, 23.5 ft over rail; 1.0 ft (12 in) from telephone; 5.0 ft (60 in) below primary and 3.3 ft (40 in) below secondary; 2.5 ft (30 in) from secondary at midspan. Utility standards add a 3.3 ft (40 in) safety zone; 1.0 ft (12 in) between communication attachments at the pole and 0.5 ft (6 in) minimum in the span, designed to 12 in; 1 ft of pole space per attacher, with CATV in the upper communication position; and third-party space at 23-27 ft above grade on a 45 ft pole [WPS-JU10; CLARK-JU2] [guideline].

**Underground** [guideline]:
- Cover over buried cable 2.0 ft (24 in) in soil, 3.0 ft (36 in) at ditch crossings, 0.5 ft (6 in) in rock, 1.0 ft (12 in) at premises [RUS 1753F-150]; conduit ≥ 2.0 ft (24 in) [RUS 1753F-151]. These are telephone and fiber specs. Conduit is 2-6 in, in sections up to 1,500 ft, typically 600-1,000 ft [RUS 1751F-643].
- Pedestals house taps, couplers, splitters, line extenders, amps and nodes [SCTE 180 §10.1.1]. They stand in the right-of-way or easement, ≥ 1.5 ft (18 in) from side and rear property-line corners, facing the street (§10.7). Vaults reach 4 ft deep (§10.1.2).
- Enclosure load tiers [SCTE 77 Table 2] [normative]: Tier 5, 5,000 lb design, sidewalk; Tier 8, 8,000 lb, sidewalk with non-deliberate traffic; Tier 15, 15,000 lb, driveways and parking; Tier 22, 22,500 lb, heavy non-deliberate traffic.

## 9. Premises

**Ground block** [SCTE 129] [normative]: rated 5-3000 MHz (§6.1); insertion loss ≤ 0.05 dB to 1218 MHz, ≤ 0.10 dB to 1794 and ≤ 0.20 dB to 3000; return loss ≥ 30 dB to 1218 MHz and ≥ 25 dB to 1794 (§6.2 Table 4); #6-#14 AWG bond wire at < 50 mΩ (§5.4, §6.5). One 1.8 GHz block allows 0.40 dB at 1002-1800 MHz [ANX-A7BCP] [field], four times the standard's limit.

**Splitters** [SCTE 153] [normative]. Classes 3 / 2 / 1 are rated to 1794 / 1218 / 1002 MHz (§7.1). Class 3 maximum insertion loss, dB (§7.3 Table 3):

| MHz | 2-way | 3-way balanced | 3-way unbalanced, high / low | 4-way | 8-way |
|---|---|---|---|---|---|
| 5-400 | 4.2 | 6.1 | 4.3 / 7.8 | 7.8 | 11.3 |
| 600-1002 | 4.5 | 7.0 | 4.6 / 8.8 | 10.0 | 12.5 |
| 1002-1218 | 4.8 | 8.0 | 4.8 / 9.2 | 12.0 | 14.3 |
| 1218-1794 | 6.5 | 9.1 | 6.7 / 11.0 | 14.0 | 16.0 |

Class 3 port isolation is ≥ 20 dB at 5-10 MHz, 28 at 10-65, 25 at 65-870, 22 at 870-1002, 20 at 1002-1218 and 18 at 1218-1794 (§7.5 Table 7); return loss ≥ 18 dB, 16 dB at 1218-1794 MHz (§7.4). Drop couplers of 6-30 dB hold ±1.5 dB on the tap leg (§7.3 Tables 1-2). Each port is labelled with its top-frequency loss rounded up to the next 0.5 dB (§6.4). MoCA splitters pass signal between output ports above 1125 MHz on purpose: port-to-port loss there is at most 25-30 dB, and input return loss only 5 dB [ANX-MMC2000; HOLLAND-CAT] [field].

**Drop amplifiers** [SCTE 161] [normative]:
- Mounted in a NID or indoors and never cascaded (§1.2). Class 2 is rated to 1218 MHz; 1794 MHz is future work (§1.5, §9.1).
- Gain within +2/−1 dB of the label; without return gain, return-path loss ≤ 2 dB plus SCTE 153 splitting loss on multi-port units (§9.3). Return loss ≥ 18 dB and port isolation ≥ 20 dB (§9.4, §9.5).
- Rated output ≥ 20 − 10·log10(n) dBmV per carrier per port, never below +10 dBmV; return output ≥ +50 dBmV (§9.6). Forward NF ≤ 8 dB; return NF ≤ 8 / 12 / 16 / 20 dB for 1 / 2 / 4 / 8 ports (§9.9).
- Products [field]: Antronix MRA gives 14.5 / 11.5 / 7.5 dB for 1 / 2 / 4 outputs at 52-1002 MHz, NF < 3 dB, passive return, and 14.8 dB at 258-1218 MHz on its high-split model [ANX-MRA]; FRA1 gives 10 dB return gain at NF 5.5 dB [ANX-FRA]; unity-gain 4- and 8-port amps with active return exist [ABS-IPA3000]. No 1.8 GHz drop amp was found.

**Filters.**
- Traps and filters are lowpass, highpass, notch or bandstop, with rejection agreed between buyer and maker, passband group delay < 20 ns and return loss ≥ 16 dB [SCTE 151 §5.2, §7.4-§7.6] [normative].
- A MoCA PoE filter is a lowpass that rolls off near 1 GHz, with a transition to about 1125 MHz and ≥ 40 dB of stop band [SCTE 235 §8.4] [guideline]. It isolates one MoCA home from another and goes at the tap spigot, the ground block or the input of the root splitter or amp (Guideline 6); with a multi-output drop amp, at the amp input [EBE-POE-NOTE] [field].
- PoE products pass 5-1002 MHz with 1.0-2.5 dB of loss and reject 1125-1525 MHz (some to 2000) by 35-70 dB [ANX-GLF1002; ABS-LPF1002; PPC-SNLP] [field]. No 1.8 GHz-compatible PoE filter was found.
- Split changes use highpass filters; one passes 258-1800 MHz at 1.5 dB and rejects 5-204 MHz by ≥ 45 dB [TEL-4HPF] [field].

**Wiring and devices.**
- Home-run wiring is recommended and loop-through avoided [SCTE 235 §7.1-§7.2 Guidelines 1-2] [guideline].
- SCTE 235's worked home: a bonding block with a MoCA PoE filter feeds an 8-way splitter serving a MoCA-capable D3.1 eMTA, a MoCA DVR and three MoCA set-tops, unused ports terminated in 75 Ω [SCTE 235 §11.2 Figure 36] [field].
- Traditional wiring is tap, bonding block, then a house splitter, with one leg to the modem and the other to one or more QAM video devices; secondary DOCSIS devices for voice or security also appear [SCTE 300 §7.11] [guideline].
- For D4.0 only the modem stays on the drop: other outlets terminated, splitters removed, no drop amp or filter left, and MoCA removed because it overlaps FDD. A splitter that must stay is 1.8 GHz-rated, with the gateway on its own leg [SCTE 300 §7.11; TR-TAR §6.5.2, §6.7] [guideline].
- SCTE 300 Table 3 lists what blocks D4.0 by band: splitters, every kind of home amp, data-only filters, PoE filters (above 860 MHz), return-conditioning filters, in-line attenuators and F-81 barrels (above 1218 MHz) [guideline].
- In-home cable is series 6 [SCTE 300 §7.12]; VOGEL2019 models 50 ft of it behind a 4-way [field]. No source gives the share of homes in each wiring case.
- **Apartment blocks.** Service runs from the tap to the building much as a residential drop does, to an interconnect box inside or
  outside it; since the signal "could be split literally dozens of times", a line amplifier goes in before the crossconnect, and a
  system designer decides whether and where one is needed. From the crossconnect each apartment has its own cable, and the riser
  needs room for one per apartment; the run "could be several hundred feet" [CS-DROP-MANUAL §10] [field]. A building of two or more units
  falls under commercial codes [same]. The twin's groups of 4 or 8 units, each fed from its own tap port to a ground block, a
  drop amp where the level calls for one (10 dB with a passive return, or 14.5 dB with 10 dB of active return), a 4- or 8-way splitter
  and 100 ft home runs, or a unity-gain 4- or 8-port amp with active return in place of amp and splitter, are [inferred] and
  `Generic` (task 25):
  one feed and one amplifier for a whole building would need a gain and a splitter tree no part here states.
- In-line attenuators and return-conditioning filters are devices found in homes, since SCTE 300 Table 3 has to list them [guideline]. No
  source here gives their values, or says when one is fitted. The twin's 3, 6 and 10 dB flat in-line attenuators, fitted only where no other
  template keeps a modem inside its level window and above its transmit floor, are [inferred] and `Generic` (task 25).

## 10. Upgrades in place

**Kept.**
- **Amp sites.** SCTE 279 amps are for drop-in upgrades at legacy amp sites, with boosters only where spacing is very long; the expected 48-50 dB gain is meant to minimize re-spacing [SCTE 279 §1.2, §7.2.2] [guideline].
- **Amp housings.** Vendors sell 1.8 GHz modules and kits for legacy 1 and 1.2 GHz housings [CS-BLE180; CS-MB180; TX-DBT1800-C2; TEL-ICON4000]; the 1.2 GHz BLE120 targets "≥ 750 MHz spacing" and fits only some older housings [CS-BLE120] [field].
- **Tap housings.** Faceplate-only parts fit existing housings: CS-FFTQ back to "F" series housings, TX-XFOMZ for legacy Motorola housings (12 A, 14 dB in/out return loss) [field]. SCTE 264's 3 GHz housings are meant to take a later faceplate swap [SCTE 264 §1.2, §7.1] [normative]. Conventional taps are split-agnostic, and for FDX to 1.2 GHz the existing 1.2 GHz taps should serve [TR-TAR §6.4.2] [guideline].
- **Strand, hardline and drops**, unless old or damaged: replace non-foam or poorly performing coax, and drops on home visits where needed [TR-TAR §6.4.1, §6.7; SCTE 300 §7.4 e, §7.12] [guideline].

**Replaced** [TR-TAR §6.4.2-§6.4.5, §6.6] [guideline]:
- Every amplifier, for FDX and FDD alike.
- For FDD to 1.8 GHz, all taps and passives: at least the tap plates, and some whole taps, since even taps rated higher may not perform.
- Conditioned taps' plug-ins, for the new split; check with the maker that a faceplate swap is supported and whether center-conductor seizure parts need replacing.
- Network filters, usually removed. Power supplies upgraded and plant repowered; SCTE 300 asks for a review of the line power configuration [SCTE 300 §7.6.2]. QAM video reduced to recover spectrum.

**When the split moves**, amp diplexers are plug-ins changed in the field or switched remotely [SCTE 279 §7.1.2] [normative], and conditioned taps' plug-ins change too [TR-TAR §6.4.3]. After a field diplexer change, return loss may be up to 2 dB lower [SCTE 279 §13.1 Table 10] [normative]. A business-as-usual refresh replaces RF modules with 5-85/103-1003 MHz modules, assumes the taps work to 1 GHz, and segments the node in place [ULM2016] [field].

**Level outcome** [VOGEL2019 §4 Figure 9] [field]. On the Figure 7 run, a 1.8 GHz upgrade swaps taps and node/amp modules, holds legacy levels to 1002 MHz and steps down to 50 dBmV analog-referenced above. The last device then reads −22.3 dBmV at 1794 MHz against a 0 dBmV target. A gateway brings it to −10.8 dBmV; adding higher output and an .875 first section gives −9.6 dBmV. Upgraded plant stays marginal at the top of the band unless the premises and the run change too [inferred].

**Cost lines.** Capital: tap upgrades, amp upgrades or removal, and node, optical and vCMTS upgrades. Expense: impairment removal, drop and home-wiring replacement, removing home amps, splitters and filters, and replacing set-tops [SCTE 300 §7.2] [guideline].

## 11. Construction cost

No public source prices coax construction or HFC parts per foot or per unit (see Gaps). Fiber construction on the same structures is priced [field]:

| Source and basis | Aerial | Buried or underground |
|---|---|---|
| FBA-2025 §2: 2025 survey medians, $/ft, labor and materials, excluding make-ready, permits and engineering | 8.00 (IQR 4.90-11.50); strand-and-lash 7.20 | Underground 18.00 (IQR 11.50-25.00); direct buried 10.32; new conduit 18.50 |
| FBA-2025 Fig 2.7: by homes per mile (5-10 / 11-50 / > 50) | 5.47 / 7.10 / 9.00 | 15.00 / 17.00 / 20.38 |
| FCC-CACM-2013 slide 20: illustrative, inputs not adopted, $/ft with structure | 7.20 at every density | Buried 4.61 / 8.80 / 10.84; underground 23.74 / 28.35 / 33.37 (rural / suburban / urban) |

Labor is 64 % of aerial and 72 % of underground cost, and a drop costs $338-400 aerial and $556-675 underground per connection [FBA-2025 §2] [field]. Where two-way cable plant already exists, upgrading it for service up to 50 Mbps cost roughly $165 per home passed in 2010 [OBI-TP1 ch. 4] [field]. One 2016 vendor study puts enablement capital per home passed at $40 for a D3.1 CMTS upgrade alone, $145 with the plant at 1002/85 MHz, a node split and a little fiber, and $338 for 10G EPON FTTH [ULM2016 Figure 10, reproducing an earlier vendor paper] [field].

## Implications for hfc-twin

All [inferred].

**`DesignRules`, tap-port target.** No source states a number. Hold the rule as a device target plus a premises template's loss, at the lowest and highest design frequencies, and derive the port level. With VOGEL2019's 0 dBmV analog-referenced device (−6 dBmV digital) and the §1 allowance, the port target is about +12 dBmV digital at 1002 MHz; the inputs are `Sourced`, the derived rule `Generic`. Tolerance should be at least half a tap step (1.5 dB) plus the sheets' tap-loss tolerance (±1.0-1.5 dB).

**Tap family.** Use 2/4/8-port families in 3 dB steps with the lowest value terminating: TX-OTTZ or TEL-4RTP at 1.8 GHz, CS-FFTQ at 1.2 GHz. The library must carry tap loss and insertion loss by frequency, not the nominal value: an 8-port "14" measures 15.5 dB. The "highest tap first" search (realism review D1) gives way to working out from the active (task 23 `feeder`), ending on a terminating tap.

**Plug-ins.** `PlugIn.family` must carry the reference frequency of `value_db`: CE-120 is tilt over 54-1218 MHz, T-EQ is at 1003 MHz, CEH at 1794 MHz, TXPI the 5-1825 MHz difference. On 1.8 GHz actives the family is electronic, in 0.5 dB steps over the TX-DBT1800-C2 ranges.

**Amp by role.** `ActiveSpec.outputs` models a main output plus a director: an equal split at about 5 dB per leg, or an 8, 10 or 12 dB coupler. That replaces D4's two-way splitters at an amp. Line extender 43-46 dB and multiport 49-50 dB at 1794 MHz are `Sourced` [SCTE 279; CS-BLE180; TX-DBT1800]. Spans follow SCTE 279 §7.4.4: one gain's worth of loss at the top frequency.

**Cable by role and drops.** Feeder .500-.625 and trunk .750-.875 rest on one vendor table [TFC-CABLE-BOOK]: `Sourced` but weak. "Express" stays `Generic`. Series 6 to 150 ft and series 11 beyond is `Sourced` [CS-DROP-MANUAL], as is the 250 ft aerial service span [RUS 1753F-801].

**Nodes.** The legacy rules file can take ULM2016's N+3 nodes of 150-520 homes at 5-7.6 actives per mile as `Sourced` [field] ranges, not rules. N+0 serving groups of 30-60 homes [ULM2016] and 2- and 4-port nodes segmenting from 1×1 to 2×4 (§6) are `Sourced`. Port balancing has no source.

**Powering.** Keep 80 % [SCTE 183], as task 23 already says, and record SCTE 287's 90-95 % as the alternative. Inserters 20 A in and 15 A per port, taps 12 A, amps 15 A and node ports 10 A are sourced. Supply ratings come from the ALPHA-XM31HP table; derive standby from ALPHACELL-HP runtimes, not a fixed 90 minutes.

**Structures.** Pole spacing by density (150-250 ft) [FCC 99-304] and maximum span by strand [RUS 1751F-635] are `Sourced`, from telephone plant. The loop rule is `Sourced` [CS-TD-MANUAL]: one per pole, active and tap, two where a span exceeds 150 ft; it feeds the parts list and a loop-crack fault, not RF. Pedestal setback (1.5 ft) [SCTE 180] and burial depth (2.0 ft, telephone) [RUS 1753F-150] are `Sourced`; pedestal spacing is not.

**Premises templates.** Sourced templates are SCTE 235's MoCA home, SCTE 300's traditional splitter home (VOGEL2019's 4-way and 50 ft fits inside it) and the D4.0 home run; their shares are `Generic`. Task 22 should compute the MoCA splitter's deliberate port-to-port leakage above 1125 MHz and the PoE filter's stop band. A PoE filter left in a D4.0 home blocks FDD downstream above 1 GHz [SCTE 300 Table 3], a fault-like premises state.

**Cost.** Use FBA-2025 per-foot medians for aerial and underground structures, marked as fiber construction. Coax material and part prices stay `Generic`.

**Legacy, then upgraded (task 23).** The upgrade keeps sites, housings, strand and hardline; swaps amp modules, diplexers, tap faceplates and conditioners; and removes premises splitters, amps and filters only for D4.0 homes. VOGEL2019's −10 to −22 dBmV at 1794 MHz are the marginal top-of-band levels the review wants a generated estate to show.

**Library.** Flag ANX-A7BCP against SCTE 129 and TEL-4XP's inserter rating against SCTE 265. Record which SCTE 264 Table 2 points each tap sheet omits; none publishes them all.

## Gaps

- **Tap-port level:** no public document states a design level, its tolerance, a tilt limit at the port, or a minimum end-of-line level. VOGEL2019's device target is the nearest.
- **Premises shares:** no source gives the share of homes with splitters, MoCA, drop amps or filters, devices per home, or a typical in-home cable length.
- **Cable by role:** no "express" role or size rule, no amp spacing by role; booster spacing only in words [SCTE 279 §1.2].
- **Nodes:** no homes or actives per node by architecture beyond ULM2016's five examples; no port-use or port-balancing rule.
- **Powering:** no placement rule for inserters, no siting rule for supplies, no count of supplies per node. Per-active minimum voltage appears only as data-sheet input ranges; SCTE 279 leaves cut-off values to the maker.
- **Structures:** spacing and spans come only from telephone sources (FCC cost model, RUS); the RUS tables need cable weight per foot, which no coax source here gives; no coax hardline burial depth and no pedestal spacing.
- **Cost:** no public coax construction cost per foot and no per-part HFC prices; NTIA or state broadband-office guidance was not found or not read.
- **1.8 GHz parts not found:** a drop amp, a MoCA PoE filter that passes FDD, and a plug-in pad or equalizer family (1.8 GHz amps are electronic).
- **Not obtained:** the CommScope/Vistance 1.8 GHz tap sheet, PPC hardline taps, Antronix app note AN-1007. Cisco's live pages are gone; its sheets were read from archived copies.
- **Source oddities:**
  - SCTE 153 writes its isolation and return-loss minima as "shall not exceed", and its class 3 4-way at 600-1002 MHz (10.0 dB) is above class 2's 8.8 dB.
  - CS-CE120 says "2 through 20" and "1 through 10" but tabulates CE-120-21 and CS-120 values up to 20.
  - CommScope's loop-size break is "< 625" / "> 700" in the text and "≤ .625" in the appendix.
  - TX-DBT1800-C3's current table (1.7 A at 90 V for 55.7 W) does not match its watts; the C2 table does. TX-DBT1800-C2's text calls it "triple balance".
  - SCTE 287's loading guidance conflicts with SCTE 183's 80 % and SCTE 180's "near 100 %" citation; none of the three is chosen here.
