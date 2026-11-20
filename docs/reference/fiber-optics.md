# Fiber optics

What the optical half of the plant is made of and how it behaves: fiber loss by wavelength, splices, connectors and reflections, how a budget is summed, cable and closure construction, analog forward and return links, digital links for DAA, hub equipment, architectures, and what optics report.

Units: wavelength nm, optical power dBm, fiber length ft, fiber loss dB per 1000 ft (kft), RF MHz and dBmV, °C. Sources quote dB/km and km; we convert with **dB/km × 0.3048 = dB/kft** and 1 km = 3280.8 ft, and say so. OMI is a fraction unless written as %.

Source notes: Remote PHY architecture, RPD status, failure modes and timing are in [daa-remote-phy.md](daa-remote-phy.md) and are cited, not repeated. The SCTE-HMS fiber node MIB is summarised in [collection-management.md](collection-management.md). Return laser loading and NPR from TR-MIDSPLIT and SCTE 183 §16 are in [plant-and-powering.md](plant-and-powering.md) §7. Vendor data sheets are examples of what products state, not norms; each is tagged [field]. No source here describes any one operator's plant.

## 1. Fiber attenuation and bands

**Bands** [ITU-T G-Sup39 §5.2 Table 5-1] [guideline; "for discussion, not specification"]: O 1260-1360 nm, E 1360-1460, S 1460-1530, C 1530-1565, L 1565-1625, U 1625-1675 (maintenance only). EDFAs work from about 1530 nm, which is what defines the C band [ITU-T G-Sup39 §5.2]. The lower limit 1260 nm is the cable cut-off [ITU-T G-Sup39 §5.2; ITU-T G.652 §8 Table 2].

**Cabled attenuation limits, single-mode** [normative unless tagged]. Checked against the PDFs except where noted.

| Source | 1310 nm | 1383 nm | 1490 nm | 1550 nm | 1625 nm | Notes |
|---|---|---|---|---|---|---|
| G.652.D max, dB/km [ITU-T G.652 §8 Table 2] | 0.40 | 0.40 (after H₂ ageing) | 0.40 | 0.30 (1530-1565) | 0.40 | 1310-1625 max 0.40 |
| same, dB/kft | 0.122 | 0.122 | 0.122 | 0.091 | 0.122 | our conversion |
| G.657.A1/A2 max [ITU-T G.657 §8 Table 1] | 0.40 | 0.40 | 0.40 | 0.30 | 0.40 | identical to G.652.D |
| G.652 typical concatenated link [ITU-T G.652 App. I Table I.1] [guideline] | 0.5 (1260-1360) | | | 0.275 (1530-1565) | 0.35 (1565-1625) | used in G.957/G.691 budgets |
| same, dB/kft | 0.152 | | | 0.084 | 0.107 | our conversion |
| SCTE "typical maximum" [SCTE 270 §25.8.1 Table 17] [guideline] | 0.33-0.35 | | | 0.19-0.20 | 0.20-0.23 | dB/km |
| same, dB/kft | 0.101-0.107 | | | 0.058-0.061 | 0.061-0.070 | our conversion |
| IEEE 10GBASE-L plant [IEEE 802.3 Cl.52 §52.14.1 Table 52-25] | 0.4, or 0.5 for TIA OSP cable | | | per §52.14.3 | | 0.5 "may not support 10 km" |
| TIA-568 max [FOA-LB, quoting TIA-568] [guideline] | 1.0 premises / 0.5 OSP | | | 1.0 / 0.5 | | read through FOA's page, not TIA |
| Vendor cable max [AFL-LEDJ p.1; AFL-ADSS p.2] [field] | 0.35 | | | 0.25 | | G.652.D/G.657.A1; = 0.107 / 0.076 dB/kft |

- 1260-1310 nm: add 0.07 dB/km (0.021 dB/kft) Rayleigh loss to the 1310 value [ITU-T G.652 Table 2 Note 2] [normative].
- Above 1625 nm, attenuation is "not well known" and rises steeply with wavelength from macro- and microbending [ITU-T G.652 Table 2 Note 1].
- Short cables (jumpers, indoor, drop) are excluded; IEC 60794-2-11 allows 1.0 dB/km (0.305 dB/kft) indoors [ITU-T G.652 Table 2 Note 1].
- Nothing read gives the 1490 or 1610 nm attenuation of a real cable, only the G.652.D bound of 0.40 dB/km (see Gaps).

**Bend loss** [ITU-T G.657 §1, §8 Tables 1-2] [normative], uncabled, mandrel method:

| Category | Minimum design radius | Max loss at 1550 nm | Max loss at 1625 nm |
|---|---|---|---|
| G.652.D | (30 mm, 100 turns) | | 0.1 dB |
| G.657.A1 | 10 mm | 0.25 dB (15 mm × 10 turns), 0.75 dB (10 mm × 1) | 1.0, 1.5 dB |
| G.657.A2 | 7.5 mm | 0.03 / 0.1 / 0.5 dB (15×10, 10×1, 7.5×1) | 0.1 / 0.2 / 1.0 dB |
| G.657.B3 | 5 mm | 0.03 / 0.08 / 0.15 dB (10, 7.5, 5 mm × 1) | 0.1 / 0.25 / 0.45 dB |

G.657.A is a subset of G.652.D and usable wherever G.652.D is specified; B3 is for runs under 1000 m (3281 ft) and need not meet G.652.D dispersion [ITU-T G.657 §1] [normative]. G.652 fibre at 15 mm can lose several dB per 10 turns at 1625 nm [ITU-T G.657 Table 1 Note 1]. Bend loss is always worse at 1625 than 1550 in every row, so a bend shows more at longer wavelengths [inferred from the tables]. SCTE 86 maps these ITU categories to IEC 60793-2-50 [SCTE 86 §6.1].

**Speed.** Group index 1.4676 at 1310 nm for "a popular" single-mode fiber gives VF 0.68 and 1.49 ns/ft one way, slower than foam coax's 1.17 ns/ft [SCTE 270 §25.1-§25.3] [guideline]. CableLabs gives 500 µs per 100 km [TR-DCA §11.9.3, via daa-remote-phy.md], which is 1.52 ns/ft [our conversion].

## 2. Splices, connectors and reflections

**Splice loss** [SCTE 181 §13.3.4 Table 10] [guideline]. Checked against the PDF.

| Method | Alignment | Typical | Maximum |
|---|---|---|---|
| Fusion, single fiber | active (core) alignment | 0.02 dB | 0.04 dB |
| Fusion, single fiber | V-groove | 0.10 dB | 0.15 dB |
| Fusion, ribbon | V-groove | 0.10 dB | 0.20 dB |
| Mechanical, single fiber | V-groove | 0.15 dB | 0.50 dB |
| Mechanical, ribbon | V-groove | 0.25 dB | 1.00 dB |

- OTDR readings of fusion splices are typically 0.03-0.2 dB; "gainers" of 0.03-0.07 dB come from mode-field mismatch, about 2 dB when splicing dissimilar fiber types; average both directions to get true loss [SCTE 181 §15.11.6] [guideline]. An acceptance rule such as "every splice under 0.1 dB" is given as an example [SCTE 181 §15.4.2.1] [field].
- TIA-568 maximum splice loss is 0.3 dB; single-mode fusion is typically under 0.05 dB [FOA-LB] [guideline].

**Connector loss.** TIA-568 allows 0.75 dB per mated pair; typical is 0.3 dB; factory-made single-mode connectors fusion-spliced on are 0.1-0.2 dB; field-polished single-mode 0.5-1.0 dB, "not recommended" [FOA-LB] [guideline]. A vendor SC/APC states 0.4 dB maximum, 65 dB minimum return loss, 0.2 dB change on remating and 0.2 dB over temperature, against TIA-568.3-D and IEC 61753-1 [CS-SCAPC] [field]. SCTE 270's worked budget uses 0.2 dB per connector and 0.05 dB per fusion splice [SCTE 270 §25.8.3] [field]. SC and LC are the connectors most used in cable; SC is square push-pull and "pull-proof" [SCTE 181 §14.2, §14.2.1] [guideline].

**Reflectance by end-face** [SCTE 181 §15.11.5] [guideline]: flat −11 to −14 dB; PC −25 to −35; super PC −35 to −45; ultra PC −50 to −57; angled (APC, 8°) better than −65 dB. A flat ferrule face reflects about 4 %, and a dirty or gapped connector up to 15 % [SCTE 181 §15.11.4.1]. OTDR event reflectance is typically −20 (large) to −50 dB (small) [SCTE 181 §15.11.4].

- APC is "the typical junction interface" of analog and digital electronics; either end-face works downstream, and cleanliness decides performance [SCTE 181 §14.3] [guideline].
- Splicing field fibers straight to equipment pigtails in a splice module housing removes polished-connector reflections, "which can cause problems with analog signals", at the cost of flexibility [SCTE 181 §12.6.2.2] [guideline].
- RIN worsens with light reflected back into the laser; an isolator at the laser output reduces it [SCTE 270 App. F] [guideline]. IIN, from scattered light reaching the detector, is sensitive to splice and fiber integrity, linewidth, link length and loss [SCTE 183 §16.10] [guideline].
- Limits: each component's optical return loss should be ≥ 40 dB, and if any is less, the cumulative round-trip ORL shall be ≥ 32 dB [SCTE 174 §7.1 Table 1] [normative for RFoG]. A node forward receiver states 40 dB minimum input ORL [CS-OM6FDRX] [field]. 10GBASE-L/E channels allow discrete reflectance below −26 dB, 10GBASE-S below −20 dB [IEEE 802.3 Cl.52 §52.14.2.2] [normative]; a 10GBASE-E channel's ORL is 21 dB [Table 52-24].
- Index-step reflection: `R = −10·log10(((n1 − n2)/(n1 + n2))²)`; 1.4676 against 1.473 gives 54.72 dB [SCTE 270 §25.6] [guideline].

## 3. How a budget is summed

All sources agree it is a plain sum in dB:

- `A = α·L + α_s·x + α_c·y`, mean fiber loss times length plus mean splice loss times splice count plus mean line-connector loss times count; it excludes equipment connectors, and a margin is to be added for repairs, extra splices, ageing and temperature [ITU-T G.652 App. I §I.1] [guideline].
- `L_total = L_connectors + L_splices + L_couplers + L_fiber + L_misc`, then `P_R = P_T − L_total` [SCTE 270 §25.8.3, §25.8.4] [guideline]. **Worked example** [field]: three connectors at 0.2 dB, one 0.05 dB fusion splice, a 50/50 coupler at 3.4 dB and 9.25 km (30,348 ft) at 0.35 dB/km (3.24 dB) total 7.29 dB, so +6 dBm launches −1.29 dBm at the receiver.
- `TLL = FL + CL + SL + CoL`, including mux and demux filter loss; `TLB = Tpwr + Rsens − TLL` [SCTE 240 §6.2.1-§6.2.6] [guideline] (sign oddity in Gaps). Connector pairs count in the headend but connections to equipment do not [SCTE 240 §6.2.2].
- Worst case, for troubleshooting: splices × max splice loss + connector pairs × max connector loss + km × max dB/km + coupler loss [SCTE 181 §15.9] [guideline].
- Link loss margin = transmitter-to-receiver dynamic range − cable plant loss; keep "as much as 3 dB" for equipment ageing and allow ±0.2-0.5 dB measurement uncertainty [FOA-LB] [guideline]. Design practice leaves a **repair margin of at least 1 dB and up to 3 dB** [SCTE 174 §14.2] [guideline].
- IEEE's 10GBASE-L budget allocates 2 dB to connections and splices at 1310 nm, and 10GBASE-E 2 dB for 30 km at 1550 nm [IEEE 802.3 Cl.52 §52.14.2.1] [normative]; worked budgets are in §6.

**Couplers.** Ideal loss is `−10·log10(fraction)`: 50/50 is 3.01 dB per leg, 90/10 is 0.46 and 10.00 dB; typical real values are 3.40/3.40 and 0.60/10.80 dB [SCTE 270 §25.8.2] [guideline]. A headend coupler-array example uses 3.2 dB per 50/50 stage [SCTE 181 §16.8 Figure 138] [field].

## 4. Cable, closures and outside plant

**Counts and tubes.** A vendor loose-tube family runs 6 to 432 fibers: 12 fibers per tube from 12 (1 tube) to 216 (18 tubes), and 24 per tube at 288 (12 × 24) and 432 (18 × 24); listed counts are 6, 12, 18, 24, 30, 36, 48, 60, 72, 84, 96, 108, 120, 132, 144, 216, 288, 432 [AFL-LEDJ p.2] [field]. Its ADSS comes in 12-432 fibers, gel-free tubes up to 216, for spans of 100-2500 ft and designs to 3500 ft [AFL-ADSS p.1] [field]. SCTE 181 says only that counts vary and the reel tag is the sure source [SCTE 181 §8.1.1] [guideline].

**Reel lengths** [AFL-LEDJ p.1, "maximum lengths"] [field]: 22,900 ft (7.0 km) for 6-144 fibers; 17,000 ft (5.2 km) for 146-216; 15,000 ft (4.6 km) for 218-288; 10,800 ft (3.3 km) for 290-432. Longer on request, on five standard non-returnable reel sizes (42 to 84 in high) [AFL-LEDJ p.2; AFL-ADSS p.2] [field].

**Types** [SCTE 181 §19.2.1] [guideline]; OSP cable shall meet ANSI/ICEA S-87-640 [SCTE 181 §19.2] [normative]:

| Type | Construction and use |
|---|---|
| Duct | Usually all-dielectric; also suits tunnels and aerial lashing |
| Direct buried and trenched | As duct plus steel armor, mainly against pocket gophers and rock |
| Aerial, lashed to strand | Usually built as duct cable, all-dielectric preferred (no grounding, lightning, induction); armored where squirrels are a risk. Squirrels cause more fiber outages than any other wildlife [§19.2.1.3.1] |
| Self-supporting | Figure-8 (integral messenger) and ADSS, no pre-installed strand [§10.3, §19.2.1.4]. ADSS dielectric suspension supports spans to 800 ft without rods, 1000 ft with [§10.3.2] |
| Microcable | Reduced tubes and strength member, air-blown into microduct; less crush and strain margin [§19.2.1.5] |
| Messenger on duct (MOD) | Strand and HDPE duct in one jacket [§8.6.1] |

ITU-T L.100 to L.111 cover cable by application (duct, buried, aerial, indoor, drop, microduct, hybrid) and L.150 to L.163 installation [SCTE 86 §10.1, §11.1] [guideline]. Air-assisted placing averages 4,000-6,000 ft per machine, with single runs to 3.6 miles achieved [SCTE 181 §9.6.1] [field]. Core ejection from old hardline can turn the aluminum shield into conduit for "typically 144" blown fibers [SCTE 181 §10.6] [field].

**Splice points and slack** [guideline]:
- Choose splice locations during the route survey; leave enough to reach the ground and a splicing vehicle plus 15-20 m (49-66 ft) more, aerial or underground, and count it when ordering reels [SCTE 181 §8.1.10, §9.1.10].
- Adjacent cables overlap by 5 m (16 ft 5 in) or more for repeated splice attempts. Aerial slack is coiled on the strand at the closure, or the splice goes into a pole-base handhole or pole cabinet; storage devices about 15 ft from the pole hold re-routing slack [SCTE 181 §8.5.3].
- Bend radii: storage devices ≥ 10 × cable diameter [SCTE 181 §8.5.4]; inside closures and housings cable ≥ 15 × diameter and fiber ≥ 38.5 mm (1.5 in) [SCTE 181 §12.1, §12.6].
- No source states a closure spacing; closures fall at reel ends, branches and node drops [inferred from SCTE 181 §8.1.10, §12.1].

**Closures** [SCTE 181 §12.1] [guideline]: protect through-splices, branch splices and node drops. Desired: at least four cables, in-line and butt entry, uncut express storage, strand hangers, #6 AWG bonding, trays matched to capacity, 5 psig flash test (sealed types); trays are usually ordered separately. Kinds: sealed aerial, free-breathing aerial, underground in cabinet or vault, and direct buried (not common in cable) [§12.1.1-§12.1.4]. Example capacities [CS-FOSC450] [field]: FOSC 450 D6 dome, 6 round ports, 36-way trays, 768 single-fusion or 3,456 mass-fusion splices, 25 mm maximum cable, 753 mm long, IEC 61300 5 m waterhead and GR-771-CORE; aerial, pedestal or underground.

**Hub termination** [SCTE 181 §12.6] [guideline]: fiber distribution units with SC, LC or MPO connector panels, or splice module housings that splice field fibers straight onto transmitter, coupler and receiver pigtails. Racks 48.7 or 59.0 cm wide. No source read gives a patch panel's port count or loss (see Gaps).

**Records** SCTE recommends [SCTE 181 §16.3, §16.5, §16.8-§16.10] [guideline]: route/cable plan, splicing plan, reel data, individual splice losses, end-to-end signatures, a coupler array diagram (every laser, coupler and fiber color from the headend), a system diagram (fibers in and out at each node and splice point) and a fiber routing/splicing matrix. That is the information `schema::fiber` holds [inferred].

## 5. Analog forward and return links

### 5.1 Modulation depth and clipping

- **OMI.** Optical: `(P_peak − P_bias)/P_bias`; electrical: `I_peak/(I_bias − I_th)`; for a complex signal, the OMI of a sine of equal average power; "OMI" alone means the whole RF load, else say per channel. Example: I_th 10 mA, I_b 30 mA, I_p 1 mA gives 0.05 [SCTE 174 §5.2] [normative definition].
- Per-carrier `m = A/I0`, with `I0 = I_B − I_TH`; RMS index of N equal carriers `µ = m·√(N/2)`. Negative peaks beyond I0 drive light to zero and clip; treat clipping as the clean signal plus a clipping-noise term. Drive is a trade between CNR and clipping noise [SCTE 270 App. F] [guideline].
- **A 1 dB change in optical loss changes RF output by 2 dB** [SCTE 270 App. F] [guideline]. An RFoG receiver below its AGC range likewise falls 2 dB per dB of optical power [SCTE 174 §9 Table 6 Note 6-1] [normative].
- Loading limits: `Σ m_j² ≤ 0.33` (composite ≤ 33 %), so equal carriers have `m_j ≤ 0.30/√N` [ITU-T J.186 §A.1.3, App. I] [normative for J.186 systems]. QAM is set 6-10 dB below analog carriers [ITU-T J.186 §A.1.3]. Common practice is 3.5 % per analog carrier and 256-QAM 6 dB lower [SCTE 174 §14.1] [guideline].
- Clipping shows as short spikes in zero span at a CSO beat and as higher BER on digital carriers; video averaging hides it [SCTE 183 §4.1.1] [guideline]. Long ingress at high level can drive the return laser into clipping and "disable the entire return path" [SCTE 181 §18.4.2] [guideline]. High-level sweep can clip a forward laser [SCTE 183 §6.2].

### 5.2 CNR from received power and modulation depth

SCTE 270 §25.7 gives four terms, adapted from Large and Farmer and the Cisco Broadband Data Book [SCTE 270 §25.7] [guideline]. Checked against the PDF. Symbols: `m` per-channel OMI, `B` noise bandwidth in Hz, `RIN` in dB/Hz, `P_R` received power in dBm, `R` responsivity in A/W, `I_r` post-amplifier equivalent input noise current density in pA/√Hz ("typ. 6 to 8"), `P_in` EDFA input in dBm, `NF` EDFA noise figure in dB.

    CNR_RIN     = 20·log10(m) − 10·log10(2B) − RIN                                    (§25.7.1)
    CNR_EDFA    = 86.2 + P_in + 20·log10(m) − NF            in 4 MHz                  (§25.7.2)
    CNR_shot    = P_R + 20·log10(m/√2) + 10·log10(R) − 10·log10(B) + 154.94           (§25.7.3)
    CNR_thermal = 2·P_R + 20·log10(m/√2) + 20·log10(R) − 10·log10(B) − 20·log10(I_r) + 180   (§25.7.4)
    CNR_link    = −10·log10(Σ 10^(−CNR_i/10))                                         (§25.7.5)

- The EDFA term is omitted on 1310 nm links, which use no optical amplifiers [§25.7]. On a multi-wavelength fiber, compute per wavelength with that wavelength's power, not the aggregate [§25.7.5].
- **Source worked example** [field]: m = 0.0358 (78 NTSC + 33 SC-QAM), B = 4 MHz, RIN −160 dB/Hz, EDFA NF 5.5 dB at +5 dBm in, P_R = 0 dBm, R = 1.0 A/W, I_r = 7 pA/√Hz: RIN 62.05, EDFA 56.78, shot 56.99, thermal 65.14, **link 52.99 dB** (we get 52.98 from the rounded terms). With a 55 dB headend and a 49.86 dB cascade, end of line is 47.33 dB [§25.7.6].
- **Slopes** [inferred from the formulas]: per dB of received power, shot CNR moves 1 dB, thermal 2 dB, RIN 0; per dB of OMI all move 2 dB until clipping. The 154.94 is `10·log10(1/2q) − 30` (our check), and 180 converts mW and pA.
- Without the EDFA and with the source's other values, our arithmetic gives 55.33 dB at 0 dBm, 52.34 at −3, 48.72 at −6 and 45.95 at −8 dBm (in 4 MHz) [inferred]. For a 6 MHz bandwidth subtract 1.76 dB [inferred].
- The same physics appears inverted in ITU-T J.186: `P_min = (e + √(e² + α(2e·I_d + N_th²)))/(α·R)` with `α = m²/(2·BW·CNR) − RIN`, linear units, N_th in A/√Hz [ITU-T J.186 App. I] [guideline]. Its example (RIN −145.8 dB/Hz, I_d 100 nA, N_th 10 pA/√Hz, R 0.8 A/W, 110 64-QAM carriers, m_j 0.0286) gives −11.0 dBm. The appendix does not state CNR_req; we reproduce −11.0 dBm only with about 34 dB in 5.057 MHz [inferred].
- SCTE 183 counts shot noise and RIN in "thermal noise" for optical links [SCTE 183 §3.1], and NPR cannot tell them from intermodulation [SCTE 183 §13.5.2] [guideline].

### 5.3 Forward transmitters

- DFB RIN is typically −150 to −170 dB/Hz [SCTE 270 App. F] [guideline].
- J.186 transmitters [normative for J.186]: 1555 ± 5 nm; system A ≥ 0 dBm out, RIN ≤ −155 dB/Hz (70-770 MHz), tolerates 65 km G.652 and +16 dBm SBS launch; system B ≥ +8 dBm, RIN ≤ −153 dB/Hz, single longitudinal mode, 120 km dispersion tolerance, +17 dBm SBS into 80 km [ITU-T J.186 Tables A.3, B.3]. External modulators are usual; a directly modulated laser needs dispersion compensation grouped by line length [ITU-T J.186 §A.3.1, §B.3.1]. Transmitters suppress SBS by frequency dithering [ITU-T J.186 §A.2.1].
- SBS sets the maximum launch: about +16 dBm for conventional HFC transmitters, up to 4 dB more with SBS suppression, about 4 dB more again for 5 km to the first split than for 20 km; only fiber before the first split counts [SCTE 174 §14.1] [guideline].
- **Example sheet** [CS-HT3562H] [field]: externally modulated 1550 nm DWDM transmitter, 11.8 ± 0.5 dBm, 43 ITU wavelengths, 46-1218 MHz, SC/APC. 190 QAM at 10.5 dBmV/ch input (8.8 dBmV/ch for 256-QAM with 30 analog). AGC holds composite ±0.25 dB over ±3 dB input. Link at 60 km: CNR 50.5 dB (52 dB at 40 km), MER > 42 dB (30 analog + 160 QAM) or > 44 dB (190 QAM), CSO 62, CTB 65 dB. It does not state OMI, RIN or the receiver power behind the link figures.

### 5.4 Forward receivers

| Parameter | OM6000 FD receiver [CS-OM6FDRX] [field] | OM4120 node [CS-OM4120] [field] | J.186 V-ONT [normative] | SCTE 174 R-ONU [normative] |
|---|---|---|---|---|
| Wavelength | 1260-1620 nm | 1260-1620 nm | 1555 ± 5 nm | 1540-1565 nm |
| Input range | −8 to +1 dBm (survives +5) | −6 to +3 dBm continuous | min ≤ −8 dBm | −6 to 0 dBm in spec; −13 to +1 without alarm |
| Noise | EIN 3.5 pA/√Hz | | | |
| RF out | ≥ 50 dBmV at 1218 MHz with −8 dBm, 0 dB pad, OMI 2.2 % | 51/33 dBmV at 1218/55 MHz actual | ≥ 80 dBµV/ch analog | +17 dBmV ± 3 at 860 MHz, OMI 3.5 % |
| Quality | MER 46.0 dB typ., source-corrected | MER 41 dB mixed (0 dBm, 3 % OMI), 44 dB all-digital; node RF only, optics excluded | | |
| Monitoring | test point 1 V/mW ± 10 %; alarm threshold −10 to 0 dBm | | | |

Citations: CS-OM6FDRX spec table and notes 1-4; CS-OM4120 forward path and notes 7, 9-10; ITU-T J.186 Table A.7; SCTE 174 Table 6. SCTE 270 uses R = 1.0 A/W in its example; J.186 assumes 0.8 A/W [SCTE 270 §25.7.3; ITU-T J.186 App. I]. The minimum for satisfactory analog C/N at an RFoG ONU is about −5 dBm; all-digital loads tolerate 3-5 dB less [SCTE 174 §14.1] [guideline].

### 5.5 Optical amplifiers and splitting

- EDFA example [CS-FA3500] [field]: 14, 17, 19, 20 and 21 dBm outputs (dual 2 × 14, 2 × 17; 2 × 21 from a 24 dBm unit); NF 4.5 dB typ., 5.0 max (gain-flattened 5.0-6.5 typ., 5.5-7.0 max), measured at 0 dBm in; input sensor range −10 to +10 dBm (0 to +20 for high-input flattened models); 1527.9-1565.0 nm; constant-current or constant-gain mode; output stability ± 0.1 dB.
- J.186 amplifier-and-splitter [normative]: input 0 dBm, output up to +21.3 dBm, NF 6.5 dB, RIN after amplification ≤ −149 dB/Hz, 16 or 32 ports; cascaded stages up to the permitted RIN degradation; up to 256 PONs from one transmitter in system B [ITU-T J.186 Table A.5, §A.1.1, §B.1.1]. Unused ports of a directly modulated system must be terminated [§A.3.1].
- Splitting one transmitter to several nodes: coupler arrays of 50/50 stages at the headend [SCTE 181 §16.8]; blast-and-split splits one high-power broadcast wavelength across subtended fibers in the field [TR-DCA §6.2.3.1] (§7). RFoG typically splits 1:32, rarely 1:64, over 0-20 km and up to 25 dB loss [SCTE 174 §8 Table 4] [normative for RFoG].

### 5.6 Return links

- Return transmitter types the node MIB names: unisolated FP, isolated FP, uncooled DFB, cooled DFB, ITU (up to 32 colors) [SCTE 38-5 `fnReturnLaserType`] [normative]. Recommended input power varies with FP vs DFB, cooling and isolation; some are rated for one carrier, others for four or more [SCTE 183 §16.4] [guideline]. Level references P1-P9, NPR and optical noise normalization are in plant-and-powering.md §7, as are the TR-MIDSPLIT DFB and digital-return NPR ranges.
- **Example node return** [CS-OM4120] [field]: analog CWDM DFB 3 ± 0.4 dBm, analog DWDM 7 ± 0.4 dBm, rated to 204 MHz; digital-return SFPs: DWDM +3 to +7, CWDM 0 to +5, 1310 nm −8 to −1 dBm. Nominal input 12, 10, 8, 5 dBmV per 6 MHz for 42, 65, 85, 204 MHz splits, 20 dBmV composite max. NPR, written "40/11 dB": analog CWDM or DWDM 40/11 (5-85 MHz) and 40/8 (5-204 MHz), over 20 or 40 km to −6 dBm at the test receiver; digital return 40/20 (2 × 85), 40/17 (1-fer), 40/11 (2-fer). We read the notation as peak NPR over a dynamic range in dB [inferred].
- **Return receiver** [SCTE 174 §16 Tables 11-12] [normative for RFoG]: input −21 to −12 dBm (extended reach −25 to −16), responsivity > 0.8 A/W at 1310 and 1610 nm, EIN < 2.5 pA/√Hz, > 10 dBmV out at OMI 17.5 % and −21 dBm, ±1 dB response. Worked: +2 dBm launch, four 64-QAM carriers and a receiver giving 28 dB C/N at −23 dBm allow 25 dB of loss [SCTE 174 §14.2] [field]. Isolation from downstream into the upstream receiver ≥ 45 dB costs ≤ 0.1 dB of 64-QAM CNR margin [SCTE 174 Table 2 Note 2-2].
- Document laser drive, bias, insertion and receiver levels at installation, since they set return headroom [SCTE 181 §18.4.2] [guideline].

## 6. Digital links for DAA

**IEEE 10GBASE-R serial PMDs** [IEEE 802.3 Cl.52 Tables 52-11 to 52-18, 52-24] [normative]. Checked against the PDF (the 2008 edition; see Gaps).

| | 10GBASE-LR | 10GBASE-ER |
|---|---|---|
| Wavelength | 1260-1355 nm (nominal 1310) | 1530-1565 nm |
| Range | 2 m to 10 km (32,808 ft) | 2 m to 30 km (98,425 ft); 40 km engineered |
| Average launch, max / min | +0.5 / −8.2 dBm | +4.0 / −4.7 dBm |
| Average receive, max / min | +0.5 / −14.4 dBm (damage: max + 1 dB) | −1.0 / −15.8 dBm; damage +4.0 dBm |
| Receiver sensitivity, OMA | −12.6 dBm; stressed −10.3 dBm | −14.1 dBm; stressed −11.3 dBm |
| RIN, OMA | ≤ −128 dB/Hz (ORL tolerance 12 dB) | ≤ −128 dB/Hz (at 21 dB return loss) |
| Power budget | 9.4 dB = 6.2 channel loss + 3.2 penalties | 15.0 dB; 10.9 channel loss; penalties 3.6 (30 km) or 4.1 (40 km) |
| Channel insertion loss, max | 6.0 dB at 1310 nm | 11.0 dB, and **at least 5 dB** |

- The 10GBASE-E channel must attenuate 5-11 dB, ideally 8 dB, with an attenuator added if needed [§52.14.3]. Links over 30 km need fiber better than B1.1/B1.3 guarantees [Table 52-18 note c].
- The cabling model is a simplex link segment: patch cord, connection, link, connection, patch cord [§52.13]. So a duplex port uses one fiber per direction [inferred].
- Vendor modules [CISCO-SFP10G Table 4] [field]: LR −8.2 to +0.5 dBm Tx, −14.4 to +0.5 Rx, 10 km; ER −4.7 to +4.0 Tx, −15.8 to −1 Rx, 40 km; **ZR** 0 to +4.0 Tx, −24 to −7 Rx, about 80 km, "not specified as part of" IEEE 10GbE. Tunable DWDM SFP+ [CISCO-DWDM-I] [field]: 100 channels on the 50 GHz grid, 1527.22-1566.72 nm, −1 to +3 dBm Tx, −24 dBm sensitivity back-to-back (BER 1e-12), overload −7 dBm, damage +5, 70 km at 20 ps/(nm·km), SFF-8472 diagnostics.
- Minimum loss: from these sheets ZR needs ≥ 11 dB and the tunable DWDM ≥ 10 dB between max launch and overload [inferred], as ER needs ≥ 5 dB by rule.
- RPD optics examples [CS-RP4111; CS-OM6RPD] [field]: "Dual 10 GbE SFP+" with path redundancy and daisy chain (the OM6000 RPD lists daisy chain as future); approved SFP+ series: 1310 nm to 40 km, 1550 nm to 80 km, CWDM in 15 wavelengths to 40 km, DWDM in 40 wavelengths to 80 km. Node power with RPD under 85 W (NC2000), 150 W (NC4000) and 140 W (OM6000), AC. Digital-return SFP links in a node sheet: 1310 nm over 10 km, CWDM 50 km at 26 dB budget, DWDM 80 km at 29 dB [CS-OM4120 note 17].

**Wavelength grids.**
- CWDM: 18 nominal wavelengths 1271, 1291 … 1611 nm at 20 nm spacing, chosen so uncooled lasers with about ±6-7 nm total variation fit wide filters, with a third of the spacing as guard band [ITU-T G.694.2 §6 Table 1, App. I] [normative]. Checked against the PDF.
- DWDM: frequencies `193.1 + n × 0.1 THz` at 100 GHz (and 0.05, 0.025, 0.0125 THz for finer grids, plus a flexible grid); `c = 2.99792458 × 10⁸ m/s` for conversion [ITU-T G.694.1 §7] [normative]. Table 1's example spans about 195.9 THz (1530 nm) to 184.5 THz (1624 nm) [ITU-T G.694.1 Table 1]. 193.1 THz is 1552.52 nm [our arithmetic; SCTE 270 §25.4 gives `λ_nm = 299,792.458/f_THz`].
- Analog DWDM typically uses 100 GHz spacing, up to 40 wavelengths per fiber; digital 25 GHz or less, 160 or more [TR-DCA §6.2.2] [guideline].

**Muxes** [field]:
- 18-channel CWDM mux/demux [FW-CMUX18]: < 3.7 dB per module, < 5.0 dB per link (mux + demux), passband ±6.5 nm, adjacent isolation > 30 dB, non-adjacent > 40 dB, return loss > 50 dB, PDL < 0.15 dB, insertion loss drift < 0.005 dB/°C, < 500 mW.
- 4-channel 100 GHz DWDM back plate [CS-HT3562H]: channel to common 1.4 dB typ., 1.6 max; express through 1.0 / 1.2 dB, connectors included; return loss ≥ 45 dB; directivity ≥ 55 dB; ≤ 21.8 dBm per port; cascades to 16 or 40 wavelengths on one fiber.
- PON pass-through filter: RFoG signal ≤ 1.0 dB, PON signal ≤ 0.8 dB, return loss ≥ 40 dB [SCTE 174 §7.2 Table 3] [normative].
- Test sets see only 1310, 1490, 1550 and 1625 nm, so other CWDM channels are tested with system transmitters or through the 1550 nm port [SCTE 240 §5.1, §6.1.4, §7.1.3] [guideline].

**What sits in the hub** [field unless noted]:
- CCAP core: a 40G Remote PHY line card with an 8 × 10G digital PIC, up to 32 RPDs per card at launch, N+1 redundancy [CISCO-cBR-RPLC]. The core may also be in a regional data center across the CIN [R-PHY §5.2.5, via daa-remote-phy.md] [normative]. A hub-shelf RPD (RPS) uses dual 10 GbE SFP+ per service group, 6 service groups per shelf, and drives analog optics [CISCO-SPHY; R-PHY §5.2.4 via daa-remote-phy.md].
- CIN switch: 48 × 1/10/25 GbE and 6 × 40/100 GbE QSFP28, PTP G.8275.1 with SyncE, boundary clock to G.8273.2 Class B, MACsec, GNSS and 1PPS/10 MHz inputs [CISCO-N9300FX3S]. The CIN can have redundant paths or load-sharing multiple paths, whose jitter differs [TR-DCA §11.9.5] [guideline].
- Timing grandmaster: GNSS receiver, G.8275.1 and G.8275.2 profiles, SyncE, up to 512 PTP clients, four GbE ports; holdover to within 1.1 µs for 4 h and 1.5 µs for 5 h on the base quartz, 1.5 µs for 2 days on the better quartz [OSA5420]. RPD timing behaviour is in daa-remote-phy.md.
- Muxes, couplers, EDFAs and analog transmitters and receivers as in §5, and fiber termination as in §4.

## 7. Architectures

- **Star from the hub** is what the documentation model assumes: fibers routed from the headend through splice points to nodes, with couplers splitting transmitters [SCTE 181 §16.8-§16.9] [guideline]. Ring architectures are mentioned as a reason for good records, without a design [SCTE 181 §16.1].
- **Blast and split** [TR-DCA §6.2.3.1] [guideline]: broadcast RF on one high-power linear 1550 nm laser, modulator and EDFA, carried far and split; each subtended fiber also gets one demultiplexed narrowcast wavelength from directly modulated DFBs, so each node's fiber carries two wavelengths whose intensities add at the detector. Gains: reach, low fiber counts, shared EDFA repeaters. Costs: complexity, node splits need new fiber, hard to scale SNR or narrowcast count.
- **Where DAA pays**: links beyond 25 km (82,000 ft) need a mid-span EDFA, high fiber counts or both; replacing blast-and-split; hubs within 8 km (26,250 ft) gain less [TR-DCA §6.2.3]. A 50 km service group is the worked scenario [TR-DCA §6.1.1] [field].
- **Fiber deep** moves to smaller service groups and N+0, multiplying service groups [TR-DCA §6.1.2] [guideline]; see docsis40.md.
- **Fibers per node.** No source states a count. What they do state: an analog node holds forward receivers and return transmitters, with return wavelengths in CWDM or DWDM [CS-OM4120] [field]; forward DWDM puts up to 40 wavelengths on one fiber [CS-HT3562H] [field]; an RPD has two 10 GbE SFP+ ports [CS-RP4111] [field]. With one fiber per direction per duplex port (§6), an analog node takes two fibers without WDM, and an RPD takes two per active port, four with path redundancy, fewer with CWDM, DWDM or a daisy chain [inferred].
- **Protection**: a node may have two optical receivers fed from paths A and B with an A/B switch (`forcePathA`, `preferPathA` …), reported in the MIB [SCTE 38-5 `fnABSwitchTable`] [normative]; RPDs offer path redundancy and daisy chain, and a daisy chain makes each RPD a three-port switch [CS-RP4111; R-PHY §12.2 via daa-remote-phy.md].

## 8. What optics report

- **Node (HMS)**: `fnOpticalReceiverPower` and `fnReturnLaserOpticalPower` in 0.1 mW, laser current mA, laser temperature 1 °C (−40..80), wavelength 0.01 nm, `fnOpticalReceiverState`, `fnReturnLaserControl` on/off, `fnOpticalAmpPresent` [SCTE 38-5] [normative]; see collection-management.md. At 0.1 mW resolution a −10 dBm receiver reads 1 and anything below about −13 dBm reads 0, so this MIB cannot resolve faint links [inferred].
- **Headend optics (HMS)** [normative]: transmitter `heOpTxLaserOutputPower` (0.1 dBm), `heOpTxLaserBiasCurrent` (mA), `heOpTxLaserTemp` (0.1 °C), `heOpTxLaserWavelength` (0.01 nm), `heOpTxInputRFPower` (RF composite, 0.1 dBm), `heOpTxInputRFPadLevel` (0.1 dB), AGC and CW/modulated modes [SCTE 85-1]. Receiver `heOpRxInputPower` (0.1 dBm), `heOpRxInputWavelengthControl` (0.01 nm), `heOpRxOutputPower` (RF, 0.1 dBm), pad, constant power or gain [SCTE 85-2]. Amplifier `heOpAmpInputPower`, `heOpAmpOutputPower`, `heOpAmpLaserOutputPower` (0.1 dBm), pump bias and TEC current (mA), constant power or gain, gain per wavelength [SCTE 85-3]. J.186 lists the alarms: RF input, optical output, AGC, power supply; amplifier input, pump, output [ITU-T J.186 Tables A.4, A.6].
- **Pluggables, SFF-8472** [SFF-8472 §9.1-§9.4, §9.8] [normative]: address A2h, bytes 96-111 real-time, 16-bit fields, internally or externally calibrated. Temperature 1/256 °C, ±3 °C; supply 100 µV, ±3 %; Tx bias 2 µA, to 131 mA, ±10 %; **Tx power and Rx power 0.1 µW**, 0 to 6.5535 mW (about −40 to +8.2 dBm), each ±3 dB; Rx may be average or OMA (A0h byte 92 bit 3). DWDM modules add laser temperature (relative ±0.2 °C) and TEC current (0.1 mA). High and low alarm and warning thresholds for each in bytes 0-39; flags in 112-113 and 116-117. RPDs expose `SfpPlusStatus` per Ethernet port, defined in R-OSSI [R-PHY Table 66, via daa-remote-phy.md].
- Maintenance practice: compare receiver DC or RF test points with the reference at installation to spot added loss; test dark fibers twice a year and inspect closures yearly [SCTE 181 §18.5.1, §18.6] [guideline]. Causes of added loss: dirty or degraded splices and connectors, vibration or stress, poor termination, bends, tight tie wraps on pigtails; causes of reflectance: degraded connectors and mechanical splices [SCTE 181 §18.5.1, §18.5.1.1] [guideline].

**Construction cost** [FBA-2025 §2] [field; US fiber builders of all kinds, Sept-Oct 2025, 38 states, self-reported]: median $18.00/ft underground (interquartile $11.50-25.00) and $8.00/ft aerial ($4.90-11.50), labor and materials, labor 72 % and 64 %. Underground: new conduit $18.50, existing conduit $15.00, direct burial $10.32/ft; trenching $19.00, plowing $11.88/ft; combinations $25.00. Aerial: strand and lash $7.20/ft, ADSS $8.23/ft. Rural to urban: underground $11.88-20.38, aerial $5.47-9.00/ft. Engineering, make-ready and permitting are on top of deployment cost.

## Implications for hfc-twin

All [inferred].

**Library classes (task 21 §2).**
- `FiberCable`: `loss_db_per_kft` keyed by wavelength band, `fiber_count`, `fibers_per_tube`, `construction` (loose tube, ribbon, ADSS, figure-8, armored, microcable), `max_reel_len_ft`. A `Generic` entry can cite G.652.D maxima (0.122 dB/kft at 1310/1490/1625, 0.091 at 1550); a `Datasheet` entry can cite AFL's 0.107/0.076. Bend loss should be a fault parameter, not a cable field, since it is a property of how the cable was handled.
- `FiberConnector`: `insertion_loss_db`, `return_loss_db`, `polish` (UPC, APC). SC/APC 0.4 dB and 65 dB is sourced; generic UPC can take SCTE 181's −50 to −57 dB band. `SpliceClosure`: capacity in single splices, tray count, ports. `PatchPanel`: ports only; loss sits in its connectors.
- `OpticalPassive`: splitter `ratio` with loss per leg (ideal plus SCTE 270's excess), mux `grid` (CWDM, DWDM 100 GHz), `channels`, `insertion_loss_db`, `isolation_db`.
- `OpticalTx`: `wavelength_nm`, `output_dbm`, `rin_db_per_hz`, `omi_per_channel` (or composite with channel count), `laser_kind` (FP/DFB, cooled, isolated), `clip_omi`. `OpticalRx`: `input_min_dbm`, `input_max_dbm`, `responsivity_a_per_w`, `ein_pa_per_rthz`, `rf_out_dbmv_at` a stated power and OMI. `OpticalAmp`: `output_dbm`, `nf_db`, `input_range_dbm`. `Transceiver`: `tx_min/max_dbm`, `rx_sens_dbm`, `rx_overload_dbm`, `reach_ft`, `wavelength_nm`, `min_loss_db`. Fields no sheet states (OMI, RIN, responsivity on node sheets) stay `Generic` even on sourced parts.

**Physics (task 22).**
- Replace `optics_cnr_db` = 48 with the SCTE 270 four-term sum from the link's received power, per wavelength. The model is then a pure function of plant: launch power minus the hop sum gives P_R; OMI and RIN come from the transmitter part; B is the subcarrier or channel bandwidth. The SCTE 270 example (52.99 dB) is the oracle. A 1310 nm link with the same parts gives 55.33 dB at 0 dBm and 48.72 dB at −6 dBm in 4 MHz, so today's 48 dB matches a received power near −6 dBm under the source's example parameters.
- The receiver's RF output must follow 2 dB per optical dB inside its AGC-free range, then flat with AGC, so a dirty connector changes both CNR and node output level unless AGC holds it. Our current `cnr_loss_db` fault has no level change; the computed version would.
- Clipping: composite OMI from loading (`m·√(N/2)`) against the transmitter's clip point gives a clipping-noise term; the J.186 0.33 bound on `Σ m²` is a sourced design check.
- `us_optics_snr_db` = 40 likewise becomes the same formula on the return link, or the node sheet's NPR at the operating input. Return laser input total power ties to plant-and-powering §7's P2.
- Digital links: margin = launch − Σ hops − sensitivity, and also launch − Σ hops ≤ overload. A link below sensitivity or above overload is down, which drives the RPD `CinLinkFault` derivation in daa-remote-phy.md rather than RF degradation.
- Bend and dirt are wavelength-dependent: per G.657 a bend costs more at 1625 than at 1550, so a fault can carry `loss_db_at_1550` and scale up for longer wavelengths only if a source gives the slope (see Gaps).

**Observations.** A node's receive power should be emitted quantized to 0.1 mW for an HMS adapter, and 0.1 µW with ±3 dB accuracy for SFF-8472. The sim owns the true value; adapters model the quantization.

**Design rules (task 23).** Fiber counts from the AFL family, 12 per tube; reel length caps splice spacing; slack 49-66 ft per splice point plus 16 ft overlap; repair margin 1-3 dB (FOA: 3 dB); IEEE 2 dB connection-and-splice allowance; 10GBASE-ER 5 dB minimum loss; closure capacity by count; costs from FBA medians, tagged as general US fiber construction.

## Gaps

- **Corning sheets unreachable.** Corning blocked downloads, so SMF-28 Ultra attenuation at 1490 and 1625 nm and Corning cable reel lengths are unread. No cable sheet read gives 1490, 1610 or 1625 nm cabled loss; only G.652.D's 0.40 dB/km bound covers them.
- **TIA-568 and ICEA S-87-640 not read**; their values come through FOA's page, whose single-mode bracket reads "[1/0.5]", which we take as premises/OSP. ITU-T L-series cable recommendations not read.
- **IEEE 802.3 read from the 2008 Section Four** (third printing 2010) in a copy hosted outside IEEE; the current edition via IEEE GET was not retrieved. Clause 52 has been stable, but check before citing as current.
- **No fibers-per-node, closure-spacing, ring-design or slack-loop-count rule** in any source. Bidi (single-fiber) SFP+ optics were not found in the sources read.
- **Analog link inputs missing from sheets**: no node or transmitter sheet states OMI, RIN or responsivity together, so the SCTE 270 formula needs `Generic` RIN and responsivity. NPR versus received power for analog return lasers is not given beyond the OM4120 single points.
- **EDFA formula constant** 86.2 is the source's; we did not derive it. J.186's example needs CNR_req ≈ 34 dB, which it does not state.
- **No CMTS or vCore sheet states source MER**; the cBR-8 line card sheet gives ports and RPD count only. No vCore product was read. R-OSSI (`SfpPlusStatus` attributes) and R-PHY's optical interface text were not re-read.
- **Microchip timing sheets blocked**; the grandmaster example is a 2015 OSA 5420 sheet via a distributor.
- **No patch panel sheet**: port counts and panel loss unsourced. Closure figures come from a product page, not its PDF; the D6 page lists six trays in its title and zero in its table.
- **Costs**: FBA covers US fiber builders, mostly FTTH; no cable-operator node-fiber cost. FCC cost model (CostQuest) and NTIA or state cost studies not read.
- **Temperature**: no source gives fiber loss versus temperature or EDFA and laser drift for the sim's environment.
- **Source oddities**:
  - SCTE 240 §6.2.6 writes `TLB = Tpwr + Rsens − TLL`; with sensitivity in negative dBm this should read `Tpwr − Rsens − TLL`.
  - SCTE 240 says CWDM runs 1270-1610 nm; G.694.2 (2003) moved the grid to 1271-1611 nm.
  - SCTE 181 §15.8's link loss example is an image (Figure 119) and did not extract.
  - SFF-8472's µ signs were lost in extraction (read as "100 V", "2 A", "0.1 W"); the stated ranges confirm µV, µA and µW.
  - OM6000 receiver's EIN prints as "3.5 dB pA/H1/2"; read as 3.5 pA/√Hz.
  - OM4120's NPR "40/11 dB" notation is not defined in the sheet.
