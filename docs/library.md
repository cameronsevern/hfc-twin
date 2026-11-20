# Component library provenance

`fixtures/library/generic.json` is every part tagged `Generic`: typical values, unverified. `fixtures/library/sourced.json` adds parts read from public data
sheets and tagged `Datasheet`, and keeps generics for each class no public sheet covered. The schema has one confidence per part, so this file records the fields inside a
`Datasheet` part that the sheet doesn't actually state.

| Part | Source | Fields from the sheet | Fields not from it |
|---|---|---|---|
| `cable-commscope-p3-625` | CommScope 5226303 sheet, rev. 2020-01-17, on a Pima County site | Attenuation (a, b fitted to its table, 5–1218 MHz), NVP 87 %, loop 1.1 Ω/kft | `k_temp` 0.002/°C, from SCTE 183 §13.1 |
| `amp-commscope-ble120` | BLE120 data sheet, rev. D, 2-2023 | Gain 38 dB, NF 9.0 dB at 1218 MHz (7.0 at the low edge), return NF 6.0 dB, return gain 27 dB, return loss 16 dB, 38–90 VAC, 24.4 W at 90 V (option X), output 55 dBmV, splits, band-edge delay | See below |
| `tap-teleste-3poc427` | 3POC427 data sheet, 2018-05-29 | Maxima: insertion ≤1.0 and ≤1.5 dB, isolation ≥35 dB, return loss 16 dB at the worst band | `power_passing` false is our reading: it's an indoor, F-connector part and the sheet states no current. Excluded from outdoor designs for that reason |
| `psu-alpha-xm3-915hp` | XM3-HP data sheet, 09/2019 | 63/89 VAC output, 15 A, 1350 VA | None |

**BLE120 fields that are ours, not the sheet's:**

- `cin_ref_db` 50.5 at `ref_out_dbmv` 49. The sheet's all-digital figure is CCN 50 dB at 49 dBmV, 11 dBmV in. CCN includes thermal noise, so we removed about 59 dB of it with §4.2.
- `ctb_ref_db` 86 and `cso_ref_db` 84. The sheet gives 74 and 78 at 55 dBmV on a mixed load. These are moved to 49 dBmV with §4.4's slopes, which are approximate. Nothing in the MER uses them.
- `min_in_dbmv` 11 is the sheet's all-digital reference input, not a stated minimum.
- `agc_range_db` 3 isn't stated. The sheet says ALC exists and falls back to gain hold when the pilot is lost.
- `group_delay_ns`: the sheet gives a maximum over the last 1 MHz at each edge. The table reads that as the rise over that MHz, with 0 in the passband. Absolute delay isn't stated.

**Gaps, still `Generic`:** outdoor hardline taps, splitters and couplers, drop cable, every node and RPD, and modems. Searches found no sheet for Antronix or PCT taps, and CommScope's Series 6 sheets
returned 404. Harmonic gates its RPD documents, as `docs/DESIGN.md` expects. The 1.2 GHz sheets we did read suggest the generic amp draws too much at 60 W against about 24 W, and is too clean, with CIN 58 dB at 52 dBmV
against 47.5 dB at that level. So the generic fixture likely overstates healthy RxMER on deep cascades, and step 12's sensitivity report should show it. The fixture's values stay as they are because `docs/physics.md`'s oracles are computed on them.

## Version 2 parts (task 21)

`generic.json` and `sourced.json` gained one or more entries for every class version 2 added, all appended after the
version 1 entries, none of which changed. The generic entries are `Generic`: version 1's RF numbers where a part is a
modular copy of a version 1 generic, the ceilings `docs/reference/plant-and-powering.md` fits for `.750` hardline and series 11
drop, G.652.D's maximum loss for fiber [`docs/reference/fiber-optics.md` §1], and typical values otherwise. The frozen
version 1 designer never picks one: it places 4-port through taps with no version 2 field (`design::optimize::version_1_tap`).

Parts read from public sheets for version 2, in `sourced.json` only. Sources are listed in `docs/reference/README.md`.

| Part | Source | Fields from the sheet | Fields not from it |
|---|---|---|---|
| `fiber-afl-ledj-48` | AFL-LEDJ | 48 fibers, 12 per tube, loose tube, G.652.D, 0.35 and 0.25 dB/km at 1310 and 1550 nm (0.1067 and 0.0762 dB/kft, our conversion) | Loss at any other wavelength |
| `fconn-commscope-sc-apc` | CS-SCAPC, a product page | SC, APC, insertion loss 0.4 dB maximum, return loss ≥ 65 dB | A typical loss: the field holds the maximum |
| `closure-commscope-fosc450-d6` | CS-FOSC450, a product page | 768 single-fusion splices | `trays` 6 is the page's title; its table says 0 |
| `mux-fiberworks-cmux18` | FW-CMUX18 | 18 channels, 1271-1611 nm, insertion loss < 3.7 dB per module, adjacent isolation > 30 dB, return loss > 50 dB | The channel list is the G.694.2 grid the sheet names, written out by us |
| `sfp-cisco-10g-lr` | CISCO-SFP10G | 1310 nm band, launch −8.2 to +0.5 dBm, sensitivity −14.4 dBm, overload +0.5 dBm | `rate_gbps` 10 is the 10GBASE name, not a stated line rate |
| `sfp-cisco-10g-er` | CISCO-SFP10G | 1550 nm band, launch −4.7 to +4.0 dBm, sensitivity −15.8 dBm, overload −1 dBm | As above |
| `edfa-commscope-fa3517` | CS-FA3500 | 17 dBm output model, NF 5.0 dB maximum (4.5 typical), input −10 to +10 dBm, 1527.9-1565 nm | None |
| `ftx-commscope-ht3562h` | CS-HT3562H | Externally modulated, 11.8 dBm, 46-1218 MHz, 10.5 dBmV per channel input at 190 QAM | `wavelength_nm` 1550 (the sheet gives 43 ITU channels), `rin_db_per_hz` −160 and `omi_pct` 3.58 (SCTE 270 §25.7's worked example, not the sheet) |
| `frx-commscope-om6000-fd` | CS-OM6FDRX | Input −8 to +1 dBm, EIN 3.5 pA/√Hz, ≥ 50 dBmV out at 1218 MHz from −8 dBm | `responsivity_a_per_w` 0.85 (not stated), `rated_lo_mhz` 54 (ours) |
| `switch-cisco-n93180yc-fx3s` | CISCO-N9300FX3S | 48 ports at 1/10/25 Gbit/s | None |
| `timing-oscilloquartz-osa5420` | OSA5420 | 4 GbE ports | None |
| `gb-antronix-a7bcp` | ANX-A7BCP | Insertion loss maxima by band to 1800 MHz, return loss ≥ 18 dB | None. Its 0.40 dB at 1002-1800 MHz is four times SCTE 129's limit (`docs/reference/design-practice.md` §9) |
| `dropamp-antronix-mrah1-15-g2` | ANX-MRA | One output, 14.8 dB at 258-1218 MHz, NF < 3.5 dB, passive return < 1.8 dB, return loss 18 dB, 5-204/258-1218 split | Output capability and band-edge group delay as a table: the sheet gives delay limits per band, not points |
| `filter-antronix-glf1002` | ANX-GLF1002 | Low-pass, 1.6 dB over 5-1002 MHz, ≥ 40 dB at 1125-1525 MHz, return loss 18 dB in the passband | `stop_return_loss_db` 1.0 (ours: a filter reflects what it stops); the rated band's 1525 MHz top is the stop band's edge. It qualifies on the high-split profile though it cuts 1125-1218 MHz, which `qualifies` can't see |
| `splitter-abs-1802h` | ABS-180X | Leg loss, isolation and return loss by band to 1800 MHz | `power_passing` false is our reading of an indoor part |
| `drop-abs-t11-072m` | ABS-T11 | Velocity 85 %, loop 16.5 Ω/kft, attenuation table to 1800 MHz | `atten_a`, `atten_b`: plant-and-powering.md §2's free fit to SCTE 74's series 11 ceilings, which this table matches point for point; `k_temp` from SCTE 183 §13.1 |
| `dc-commscope-ssp12q` | CS-SSPQ | 12 dB tap leg, return loss by band, 15 A on all ports | `excess_loss_db` 1.2: the through leg is 1.1 to 2.2 dB typical across the band, which one number can't hold. We took 601-750 MHz's 1.5 dB less the ideal 0.28 dB |
| `leq-commscope-ffe8-120-42` | CS-FFE120 | Forward loss by frequency at 54-1218 MHz, return loss 15 dB, 42/54 split | `value_db` 8 is the model's name, not a measured difference; `power_passing` from its 12 A rating |
| `pad-commscope-npb-6` | CS-NPB | 6 dB, return loss ≥ 22 dB to 1218 MHz | `insertion_loss_db` 0: the sheet states flatness, ±0.1 dB, not loss beyond the value. `family` is ours |
| `battery-alphacell-3-5hp` | ALPHACELL-HP | 12 V, 104 Ah at the 20 hour rate to 1.75 V per cell | `v_end_discharge` 10.5 V is 6 cells at that 1.75 V, our arithmetic. The supply's manual uses 10.2 V |

**Read, and not made into entries:** the hardline tap sheets (CS-FFTQ, TX-OTTZ, TEL-4RTP, ANX-MHT) and the amp and node sheets
(CS-BLE180, CS-MB120, CS-MB180, TX-DBT1800, TEL-ICON4000, CISCO-HGBT, CISCO-HGD, CS-NC2000, CS-NC4000S4, VEC-EN9000,
HAR-RIPPLE, HAR-SHELL1). Their numbers are in `docs/reference/design-practice.md` §2, §4 and §6. A tap entry needs insertion and
tap loss tables at the frequencies the sim reads, and an active entry a dozen fields none of these sheets states together (CTB, CSO,
CIN at a reference output, AGC range, minimum voltage), so either would be mostly ours. Task 23, which places them, is where to
decide what an entry made from them should carry.

**Classes still only `Generic`:** `PatchPanel`, `Cmts` (no sheet states ports and source MER together), `Connector`,
`Cpe`, `Structure`, `Strand`, `Conduit`, the analog return transmitter and the hub return receiver as separate parts, the
tap conditioner and cable simulator plug-ins, the drop F connector, and taps of 2 and 8 ports.

## Parts added in task 22

Seven `Generic` entries, in both libraries, each inserted after the part it varies. No existing entry changed.

| Part | What it is | Where its numbers come from |
|---|---|---|
| `osplit-generic-1x8` | A 1×8 hub coupler, 10.2 dB | Three 50/50 stages at SCTE 270 §25.8.2's typical 3.40 dB each, our arithmetic. The as-built fixture's forward transmitter reaches its node through it, which brings 10 dBm inside the node receiver's −8 to +1 dBm (`docs/physics.md` §16) |
| `rtx-generic-1310-npr` | A 1310 nm return DFB | OMI 6.9 % at 12 dBmV per 6 MHz and RIN −150.5 dB/Hz, fitted by least squares to TR-MIDSPLIT §6.1.1.1's 35.5 dB at 7 dBmV and 52.5 dB at 26 dBmV, with an 80 MHz load at −6 dBm received (both approx, §16). Output 3 dBm is `rtx-generic-1310`'s; `clip_total_in_dbmv` 27 is the top of the source's usable range |
| `rrx-generic-hfc` | A hub return receiver for an HFC node's laser | `rrx-generic` is SCTE 174's RFoG receiver, rated −21 to −12 dBm, which a node's 3 dBm laser overloads on any short link. This one takes −15 to +3 dBm, from memory; responsivity and noise are `rrx-generic`'s. Its RF output isn't read by the sim (§16) |
| `modem-generic-d30-clean`, `-d30-noisy`, `-d31-clean`, `-d31-noisy` | Two more modems per generation, spread around `modem-generic-d30` and `-d31` | Ceilings 41.5 and 38 dB (D3.0), 46.5 and 42 dB (D3.1, the clean one near the lab modem `docs/physics.md` §4.5 fits at 46.79), noise figures 8 and 12 dB, return loss 22 and 12 dB, each stating ±1 dB on its ceiling and noise figure, which §8's randomization draws within. From memory, around the existing generics' 40/44 dB, 10 dB and 18 dB |

## Parts added in task 23

All `Generic`, appended to both libraries after the entries before them, none of which changed. The designer's rules files name
them; the task's decisions (`docs/tasks/23-as-built-designer.md`) list the first set with where each number comes from.

- **For the designer's stages:** roots, an RPD and two amps that state their source MER, noise-figure, CIN and reference composite
  tables; a CMTS stating a source MER; `ftx-generic-1310-full`; 4-port faceplate taps for every value; tap conditioners to 12 dB and
  line equalizers of 6, 8 and 12 dB; fiber cable of 6 to 144 fibers; `panel-generic-144`; `cable-generic-500` and a `.500` pin and
  splice; and, for the legacy mode, a 750 MHz node, bridger and line extender and 1 GHz 4-port taps.
- **Long reach** (the closing fixes): `sfp-generic-10g-er` and `sfp-generic-10g-zr`, with the vendor figures
  `docs/reference/fiber-optics.md` §6 gives for 10GBASE-ER and a ZR module; `ftx-generic-1550` and `ftx-generic-1550-full`, an
  externally modulated 1550 nm forward transmitter at the example sheet's output in §5, at a standard and a full-load index; and
  `rtx-generic-1550`, a 1550 nm DWDM return transmitter at the 7 dBm §5 quotes.
- **Premises at the standards' ceilings** (the closing fixes): `gb-generic-scte129`, a ground block at SCTE 129's 0.05 dB to 1218 MHz
  and 30 dB return loss, and `splitter-generic-4way-home-scte153`, an in-home 4-way at SCTE 153's class 2 ceiling for a 1218 MHz part,
  8.8 dB at 600 to 1002 MHz and 8.9 to 1218 [`docs/reference/design-practice.md` §9]. The note gives no class 2 figure below 600 MHz,
  so the 55 MHz point is class 3's 7.8 dB. The rules' premises templates name these two.

**Two older entries are over those ceilings, and stay.** `gb-generic` reads 0.2 dB at 55 MHz and 0.5 dB at 1000, ten times SCTE 129's
0.05 dB. `splitter-generic-4way-home` reads 7.8 and 10.0 dB, which is SCTE 153's class 3 ceiling, for a 1794 MHz part, on an entry
rated to 1218 MHz, whose class 2 ceiling is 8.8 dB. Both came in with task 21 from memory. They stay because the as-built fixture's
oracles (`fixtures/one_street_as_built/`, `docs/physics.md` §13) are worked on them, and an entry doesn't change once something rests on
it. A home's loss on them is 0.45 dB (ground block) and 1.2 dB (4-way) more at 1 GHz than on the new two. `splitter-generic-8way-home`
reads SCTE 153's class 3 ceiling too; the note gives no class 2 figure for an 8-way, so there's nothing to set beside it yet.
