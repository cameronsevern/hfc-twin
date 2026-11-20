# DOCSIS 4.0: ESD (FDD) and FDX as they differ from 3.1

Scope: band plans, channels, levels, transmit limits, FDX mechanics, plant, PNM, MAC symptoms,
coexistence and migration for DOCSIS 4.0, and what they demand of schemas and architecture built earlier.

Assumes [phy-docsis31.md](phy-docsis31.md); only the differences are here. "ESD" in `docs/DESIGN.md` is
called FDD in the specifications. Levels are dBmV in the stated bandwidth; note 6 vs 1.6 vs 6.4 MHz.

## 1. Frame

- Two alternatives, both on the 3.1 OFDM/OFDMA PHY: FDD extends upstream to 684 MHz and downstream to
  1794 MHz; FDX shares a band between directions [normative] [PHY4 §5.1]. OFDM numerology, PNM and
  PHY-MAC convergence are pointers back to 3.1 [PHY4 §7.2.3, §7.3.3, §8, §9].
- A distributed architecture is assumed for both [normative] [PHY4 §7.2, §7.3]; deployment on traditional
  HFC nodes is not recommended [guideline] [SCTE 300 §6.1]; TR-TAR assumes DAA and digital optics
  [TR-TAR §1.4.0, §1.4.2].
- Optional Annex A takes downstream to 3000 MHz: at least 12 resources of 192 MHz (2304 MHz); upstream is
  unchanged [normative where adopted] [PHY4 Annex A]. Not distilled further here.

Device classes that can share a plant [PHY4 §3, §7.2; MULPI4 §12.1.2, §14]:

| Class | What it is | FDX band behavior |
|---|---|---|
| D3.0, D3.1 | Legacy, diplexed by MDD TLV 21.1–21.3 | Non-L D3.1 use of FDX spectrum is undefined; receives non-FDX channels |
| FDX-L | D3.1 CM with a software upgrade | One direction per sub-band: high-split: TX up to 204, RX from 258; mid-split: RX only. RBA-SW only |
| FDD CM | Purpose-built; at least 2 of 5 diplexer settings | None |
| FDD-XL | FDD CM with limited FDX support | Like FDX-L: TX below its US edge, RX above its DS edge |
| FDX CM | No diplexer between sub-bands, echo canceller | Either direction on any sub-band, RBA-HW or RBA-SW |
| Unified | FDX or FDD, one at a time; DS to 1794 | Mode chosen from MDD TLV 17.5, 21, 22 |

An FDX CM changes between 3.1 mode and FDX mode only by reboot [PHY4 §7.2]; an FDD CM may reboot when
changing mode or split [PHY4 §7.3]. It scans for a primary downstream at 804–1218 MHz, then reads the plant
type from the MDD [normative] [MULPI4 §12.2.2.1].

## 2. Band plans

### 2.1 FDD (ESD) splits

| Split | US upper | DS lower | Applies to | SCTE 279 amp option |
|---|---|---|---|---|
| Sub / extended sub | ≤42 | 50–54 | Node only | should |
| Euro | 65 | 87.5 | Node only | should |
| Mid | 85 | 108 | Node only | shall |
| High | 204 | 258 | Node, CM | shall |
| UHS-300 | 300 | 300–372 | Node, CM | may |
| UHS-396 | 396 | 396–492 | Node, CM | shall |
| UHS-492 | 492 | 492–606 | Node, CM | shall |
| UHS-684 | 684 | 684–834 | Node, CM | should |

[normative] [PHY4 Table 29, Table 31]; amp column [SCTE 279 §7.1.1], which lists Euro's DS edge as 85, not 87.5.

- The DS lower edge is a **range** for UHS: the US edge is fixed and vendors may narrow the transition band
  [PHY4 §7.3.2]. The top of the range (372, 492, 606, 834) is the original static edge. SCTE 279 lists
  those static values for amps, noting downstream may start lower [SCTE 279 Table 1].
- On UHS a D4.0 CM's upstream is **two transmit channel sets**: legacy TCS in 5–85 and TCS_FDD (also
  TCS_EXT) in 108–split, with 85–108 unusable [normative] [PHY4 §7.3.2.3, Table 4; MULPI4 §13.1.2]. A D3.1
  CM still uses 5–204 as one TCS, and the 108–204 channel belongs to both: legacy TCS for D3.1, TCS_FDD for
  D4.0 [MULPI4 §13.1.3]. The node sees a continuous 5–684 [MULPI4 §13.1.1].
- High split is the special case: an FDD CM uses one TCS, 5–204, under 3.1 fidelity rules [PHY4 §7.3.2.3,
  §7.3.4.12].
- Extended US channels are 96 MHz on a fixed grid from 108: 108–204, 204–300, up to 588–684 [normative]
  [PHY4 §7.3.2.5.5; MULPI4 §13.1.2]. Allocations: 192, 288, 384 or 576 MHz, skipping 480.
- Occupied downstream is 1536 MHz for the node: 258–1794 contiguous, or with holes (108–1794 with a 150 MHz
  gap is the given example) [PHY4 §7.3.2]. Below 258 the node is in Legacy Transition Mode and should carry only
  video SC-QAM and non-DOCSIS signals, because FDD CMs need not receive below 258 [PHY4 §5.2.1.1, §7.3.2].
- FDD also covers mid- and high-split band plans with DS extended to 1794 [MULPI4 §5.2.14, §13.1.1].

### 2.2 FDX plans

| | CFDX (classic) | SFDX (shifted) |
|---|---|---|
| Legacy upstream | 5–85 | 5–204 |
| Unused gap | 85–108 | 204–258 |
| FDX band | 108–684 | 258–834 |
| FDX transition band (CM unspecified) | 684–804 | 834–984 |
| Allocated tops (96/192/288/384/576 MHz) | 204, 300, 396, 492, 684 | 354, 450, 546, 642, 834 |
| Likely adopters | Mid-split fiber-deep operators | High-split operators heading to 1794 |

[normative] [PHY4 §5.2.1.1, §7.2.2, §7.2.2.5.5, §7.2.2.7]; adopters [PHY4 §7.2.4.14.2.1, §7.2.5.9.1].

- A node and a CM must each support at least one plan and may support both [normative] [PHY4 §7.2.2].
  Plan, sub-band count (1–3), width (96 or 192) and DS upper edge (1218, 1794, 3000) are signaled in MDD
  TLV 22.2–22.5 [MULPI4 §6.4.28.1.21].
- Sub-band layouts: 1, 2 or 3 of 96 MHz (1 US + 1 DS each), or 2 or 3 of 192 MHz (2 US of 96 + 1 DS each)
  [normative] [PHY4 §7.2.2.5.5]. Allocated spectrum always starts at the plan's lower edge and holds only
  FDX channels. Above it, within the band, the node may place SC-QAM or non-FDX OFDM, which an FDX CM in FDX
  mode need not receive, and no US channel exists there [PHY4 §7.2.2.6].
- DS upper edge is 1218 or 1794 for FDX [PHY4 §5.2.1.1, Table 3]; 1794 applies to Unified devices and to RPD/RMD
  profiles measured at Interface C [PHY4 §7.2.5.10.1].
- Per sub-band, US and DS share subcarrier spacing and CP; these may differ between sub-bands. A subcarrier
  excluded downstream is excluded upstream, and the US channels' active edges stay inside the DS channel's
  [normative] [PHY4 §7.2.2.5.5].
- Static FDX: a single RBA for all CMs (all-upstream use), no sounding, one TG. The example runs 108–204 upstream
  in an amplified **N+X** mid-split plant using high-split amps, leaving 204–258 for amp roll-off; neighbors may
  need band-stop filters [field] [MULPI4 App XIV]. `FdxBandCfg.Direction` configures a sub-band as upstream,
  downstream or dynamic per node [CCAP-OSSI4 §6.5.4.13].

### 2.3 Selectable and switchable splits

CM diplexer configurability is vendor-specific: switch, software filter via OSSI, or other [PHY4 §7.3.2.3].
Plant state reaches CMs in MDD: TLV 21.1–21.3 for pre-4.0 CMs, overrides 21.4–21.6 for FDD CMs on UHS, TLV 22 for
FDX; the CMTS sets 21.1–21.3 to the highest legal values not above the overrides [normative]
[MULPI4 §6.4.28.1.20]. Amp diplexers are field-swappable plug-ins and may be remotely switchable between a pair of
installed filter sets, so a start split can be activated upward later [SCTE 279 §7.1.2]. DOCSIS places no
requirement on amp diplexer settings, and amps limit usable spectrum [MULPI4 §13.1.1]. Allocation can grow
incrementally [PHY4 §7.2, §7.3].

## 3. Channels and OFDM parameters

| Item | FDD CM | FDD node | FDX CM | FDX node |
|---|---|---|---|---|
| DS OFDM (≤192 MHz) | 5 | 6 | 5, 3 FDX-capable | 6, 3 in FDX band |
| DS SC-QAM | 32 | 32 | 32 | 32 |
| US OFDMA | 7 for UHS-684 (3/4/5 for 300/396/492) | 8 | ≥7 CFDX, ≥8 SFDX; 6 FDX | 8; 6 FDX |
| Legacy-band OFDMA | ≥1 below 85 | | 1 in 5–85 (CFDX), 2 in 5–204 (SFDX) | 2 |
| US SC-QAM | 4 (8 optional), in 5–85 | same | 4 (8 SHOULD) | same |

[normative] [PHY4 Table 7, Table 30, Table 32, §7.2.2, §7.3.2]. A CM transmits on all channels at once. No
SC-QAM upstream in the FDX or extended band [PHY4 §7.2.4.12.2, §7.3.4.12.2].

- FDX DS channels are 96 or 192 MHz; FDX US channels 96 occupied, up to 95 modulated; minimum encompassed 86 MHz per
  sub-band [PHY4 §7.2.2.1, §7.2.2.2, Table 13]. FDD US: 95 max, minimum occupied 6.4 MHz (25 kHz) or 10 MHz (50 kHz)
  [PHY4 Table 38].
- US: 25 kHz/4K FFT (3800 active) or 50 kHz/2K FFT (1900 active), 102.4 MHz sampling, 40/20 µs; BPSK to 4096-QAM.
  DS: 25 kHz/8K or 50 kHz/4K, QPSK to 4096-QAM; minimum contiguous 22 MHz [PHY4 Table 13, Table 38, Table 44].
- CP: FDX band 96, 128, 256, 384, 512 samples (0.9375–5 µs) [PHY4 Table 13]. FDD 96 to 640 samples, 0.9375–6.25 µs
  [PHY4 Table 38]. Rolloff (window) to 224 samples.
- **Boosted pilots are not supported in FDX or FDD extended channels** [PHY4 Table 13, §7.3.4.12.4]. Maximum scheduled
  minislots do not apply [PHY4 §7.2.4.12.3, §7.3.4.12.3].
- Minimum grant bandwidth: a CM bursting in the band must use at least the minimum, met with any mix of probes, ranging,
  OUDP testing and data [MULPI4 §12.1.4, §13.1.5]. Side effect: no contention request or (FDD CM) broadcast ranging
  regions on extended channels, so upstream entry there is unicast for D4.0 CMs [normative] [MULPI4 §13.1.5].
  Under-grant hold in FDX: UGHU = 6 users, UGHB = allocated spectrum / 6 [PHY4 §7.2.4.12.7.1].
- Unlike FDD, FDX channels are not a reliable path for broadcast MAC messages: SYNC, MDD, DPD, RBA and DPR go on the primary
  (non-FDX) DS, and UCD/MAP/DCD may not use an FDX DS [normative] [MULPI4 §12.1.3].

## 4. Levels and tilt

**FDX node, CFDX to 1218** [normative] [PHY4 §7.2.5.9.1, measured at Interface D]:
reference PSD 37 dBmV/6 MHz at 111 MHz, 58 at 1215, linear 21 dB (about 1.89 dB/100 MHz). That totals 73.8 dBmV TCP, beyond
current hybrids given FDX node loss from a coupler replacing the diplexer. The node must reach 72 TCP (should reach 73.8), with
one down-step on a channel boundary at 1002–1122 MHz:

    P_i = 37 + 21·(i−1)/184 − Step(i)        i = 1..185 (6 MHz equivalents)
    TCP = 50.78412 + 10·log10[ (10^(fD/528.571) − 10^(fstart/528.571))
                              + 10^(−D/10)·(10^(fstop/528.571) − 10^(fD/528.571)) ]

Drop for 72 TCP: 3.5 dB at 1002, 3.9 at 1026, 5.0 at 1062, 9.8 at 1110; for 71: 6.5 at 1002, 9.5 at 1044 [PHY4 Table 17]. Fidelity
relaxes 0.8 dB per dB of step. Config range 70–72 TCP, stability ±1 dB, channel accuracy ±3 dB [PHY4 §7.2.5.9.1.1,
Table 19]. Oracle checked by hand [inferred]: D = 0 over 108–1218 gives 73.79; D = 3.9 at 1026 gives 72.0.

**SFDX reference DS PSD** (defines the receiver test, not a transmit mandate): 35.6 dBmV/6 MHz at 261, 45.7 at 981, linear; TCP 60.0
over 258–834, 62.4 over 258–984; content above is operator-chosen [PHY4 §7.2.4.14.2.1]. Fidelity to 1794 or with SFDX follows the
FDD rules at Interface C [PHY4 §7.2.5.10.1].

**FDD node.** Normative point is Interface C, before the launch amp [PHY4 §7.3.5.9]:
channel reference ≥ 20 dBmV/6 MHz, per-channel adjust to −10 dBc, steps only on channel boundaries [PHY4 Table 42, App III]. MER ≥ 48 dB in
any 192 MHz channel; in-band −50/−47/−45 dBr below 600, 600–1002, above 1002; 1 dB MER relief per dB of step [PHY4 Table 41].
Interface D is informative: TCP at least 70 expected [PHY4 App III]; target 66–70 TCP with CCN ≥ 49 dB to 1.8 GHz, 75 with CCN ≥ 46 dB to
3 GHz [guideline] [PHY4 §A.5.8].

**TCP integral.** Linear equalizer: `TCP = 10·log10 ∫ 10^((m·f+b)/10) df` with PSD per MHz, plus a cable-equalizer form with PSD linear in √f.
Options to manage TCP: tilt, gaps, one or several step-downs [PHY4 App IV]. Carrying the 21 dB/1104 MHz slope to 1794 without a step
gives about 85 dBmV [inferred], which is why step-downs are not optional in practice.

**Reference amp profiles** (characterization, not a mandated design):

| Device | Span | Tilt | Step | Source |
|---|---|---|---|---|
| 1.8 GHz high-gain amp | 258–1794 | 19 dB linear (virtual line) | 0 or 6 dB at 1026 | [SCTE 279 Table 2] |
| 1.8 GHz booster | 258–1794 | 15 dB | 0 or 6 dB at 1026 | [SCTE 279 Table 7] |
| FDX amp | 108–1218 | 16 dB (13 dB to 1002) | none | [SCTE 296 §8.1.8.3] |

Amp slope has normally been cable-shaped, not straight-line [SCTE 296 §8.1.3]. Common test bands are per-band (legacy vs extended), and
performance must be published as CCN vs TCP [normative] [SCTE 279 §7.2.8; SCTE 296 §8.1.8].

**CM input, FDD** [normative] [PHY4 Table 44]: total < 40 dBmV over 108–1794; −24 to +21 dBmV/24 MHz, the same −30 to +15 per 6 MHz; a
lowest-24-MHz-relative cap per MHz of X − 10·log10(24) + 12. FDX inherits 3.1 limits except the edges [PHY4 Table 21].

CM CNR for 10⁻⁶ PER depends on what the plant carries [normative] [PHY4 Table 25, Table 45, Table 46]:

| QAM | Plant ≤1218: CNR ≤1002 / 1002–1218 (min P6AVG) | Plant to 1794: CNR (min P6AVG) | FDX ext-ACI / self-ACI at 0 dBmV |
|---|---|---|---|
| 4096 | 41 / 41.5 (−6) | 41 (0); 44 (−6) | N/A / N/A |
| 2048 | 37 / 37.5 (−9) | 37 (−6); 40 (−9) | 40 / N/A |
| 1024 | 34 (−12) | 36 (−12) | 35 / 42 |
| 512 | 30.5 (−12) | 33 (−15) | 31.5 / 33 |
| 256 | 27 (−15) | 30 (−18) | 28.5 / 29 |
| 64 | 21 (−15) | 26 (−24) | 22.5 / 23 |
| 16 | 15 (−15) | 20 (−27) | 16.5 / 17 |

FDX columns [PHY4 Table 22, Table 23]. At 3 dBmV/6 MHz self-ACI, 2048 reaches 44.5 and 1024 improves to 36. Plant RF assumptions: DS CCN ≥ 35 dB, group delay
≤ 113 ns/24 MHz (may exceed within 30 MHz of the lower edge), up to 1.74 dB/6 MHz; US C/(I+N) ≥ 25 dB, ≤ 163 ns/24 MHz, 2.78 dB/6 MHz, 14 dB seasonal
gain swing [PHY4 Table 3, Table 4]. Micro-reflection bounds are unchanged in form.

## 5. CM transmit limits

Table per band; all PSD at Interface F, dBmV/1.6 MHz at 108.8 and 683.2 (258.8 and 833.2 for SFDX) [normative] [PHY4 §7.2.4.12, §7.3.4.12, Table 33]:

| Mode | Ref PSD low/high | Max in band (dBmV) | Modulated | Legacy band total | Min per-channel |
|---|---|---|---|---|---|
| CFDX | 33 / 43 | 64.5 | 570 MHz | 55 in 5–85 | −15 dB |
| SFDX | 32.25 / 42.25 | 63.75 | 570 MHz | 59 in 5–204 | −15 dB |
| UHS-684 | 33 / 43 | 64.5 | 570 MHz | 55 in 5–85 | −15 dB |
| UHS-492 | 36 / 46 | 63.6 | 380 MHz | 55 in 5–85 | −18 dB |
| UHS-396 | 36 / 46 | 61.3 | 285 MHz | 55 in 5–85 | −18 dB |
| UHS-300 | 36 / 46 | 58.6 | 190 MHz | 55 in 5–85 | −18 dB |
| High split | 3.1 rules | 65 TCP over 5–204 | | | |

Overall TCP ≥ 65 dBmV for FDX and UHS-684 [PHY4 §7.2.4.12, §7.3.4.12.1]; higher is permitted. Hand integration of the lines reproduces 64.5 and 63.75 [inferred].

- The reference line is `Pref(f)`: 10 dB per 360 slots of 1.6 MHz, a straight line in frequency, **not** a flat limit. It sets the fixed slope a D4.0 CM's
  fidelity is specified on. Alternate tilts for fidelity: 8 dB (34.3/42.3) and 12 dB with 1 dB relief (31.6/43.6); FDD allows them for UHS-684 only [PHY4 Table 8, Table 33].
- **Reported and commanded power in extended channels is relative to Pref, in dB, at virtual Interface F′** where Pref looks flat [PHY4 §7.2.4.12.2, §7.3.4.12.2]. Only
  the legacy TCS keeps the 3.1 absolute form. Within one 96 MHz channel Pref changes 1.67 dB [inferred].
- Plimit: 1.5 dB above Pref for channels wholly below 300 (CFDX) or 450 (SFDX), else 1 dB; per subcarrier 3/2.5/2 dB by band [PHY4 §7.2.4.12.2]. FDD: 1.5 / 1 dB [PHY4 §7.3.4.12.2].
- Window: `P1.6low_multi = max(Pref − Pload_min_set − 10, Pref − 15)`, normally a 10 dB DRW covering ±2 dB plant tilt deviation from 8–12 dB [PHY4 §7.2.4.12.4]. Violation sets a bit in the RNG-REQ SID.
- Per subcarrier: `Pt_sc_i = P1.6r_n + PreEq_i − 10·log10(32 or 64)`; probes add Probedelta [PHY4 §7.2.4.12.5]. Accuracy ±2 dB with pre-eq off.
- The two TCSs are independent: separate grants, power, and reconfiguration; one inactive frame on a channel, ≥20 µs global quiet, and global time before ±3 dB cumulative [PHY4 §7.2.4.12.6]. An RBA change needs no reconfiguration.
- Upstream MER ≥ 42 dB at top of band with pre-eq, ≥ 47 at UGHB grant; 5 dB relief at lowest frequency [PHY4 Table 11].

**Headroom.** Two figures: TCP headroom and per-channel minimum headroom; with tilt and several channels the channel limit bites first. Knobs: lower the receive set point, shrink occupied bandwidth, fix attenuation. Failure is FEC errors, lower bit
loading or a dropped channel into partial service [guideline] [SCTE 300 §7.14]. Check that the CM limit still reaches the node at target at every frequency [TR-TAR §5.1, §6].

## 6. Upstream receive

FDD RPD/RMD, Interface C, dBmV/6.4 MHz set point [normative] [PHY4 Table 39, Table 40]: −4 to 25 up to 64-QAM, 0 to 25 for 128–1024, 7 to 25 for 2048, 10 to 25 for 4096; burst range −9/+3 (to 256) or −3/+3; TCP may run 3 dB above and any 6.4 MHz 8 dB above. CNR for
10⁻⁶ PER: QPSK 11, 16-QAM 17, 64 23, 256 29, 1024 35.5, 2048 39, 4096 43. BaseTargetRxPower is per 1.6 MHz in R-PHY terms.

FDX node: no Interface C; measured at D with echoes present [PHY4 §7.2.4.14.1]: QPSK 12.5, 16-QAM 18.5, 64 25.5, 256 32, 512 36, **1024 44**, set points 0–7 dBmV/6.4 MHz; **2048 and 4096 are N/A** in the FDX band [PHY4 Table 16]. Test echo model: peak return loss
10.4 dB at 108 rising to 18.7 at 684; average 17.6 to 22.1 per 96 MHz channel [PHY4 Table 14, Table 15; §7.2.4.14.2].

## 7. FDX mechanics

**Duplex.** Node transmits and receives on the same spectrum at once via echo cancellation and self-interference cancellation; a CM is FDD per sub-band, either up or down at any moment [PHY4 §7.2; MULPI4 §5.2.13].

**Groups.** IG: CMs whose CM-to-CM co-channel interference exceeds a design limit, per sub-band; limit may vary by frequency and is vendor-specific [MULPI4 §12.3]. TG: IGs merged for traffic balance; each has one RBA (direction per sub-band), and TG 1–255 [MULPI4 §5.2.13, §6.4.51]. Only the TG is reported; the CMTS is the source of truth [PNM-HFC §VIII.2.1.4.2].

**RBA.** RBA-SW (type 61) for changes slower than every few seconds, RBA-HW (type 62) for faster; FDX-L and FDD-XL hear only SW, with `undefined` for fast sub-bands [MULPI4 §6.4.51, §11.9.1]. Refresh ≤2 s, timeout 3 refreshes, start gap 600 µs/2 ms, 8 future RBAs stored [MULPI4 §6.4.51, Annex B]. Direction switch: CM switch times `t-switch-du`, `t-switch-ud`, then `t-ds-reacquisition` from 1–4 away/recovery pairs; US re-ranges if away > T4/2 [MULPI4 §12.5.1]. CMs with an expired RBA freeze loops and still answer unicast grants [MULPI4 §11.9].

**Sounding** [PHY4 §7.2.6, §7.2.8; MULPI4 §12.3].

| Test → measurer | Method |
|---|---|
| FDX → FDX | OUDP or CWT |
| FDX → FDX-L, FDD-XL | CWT |
| FDX-L, FDD-XL → FDX | OUDP |

- CWT: tones offset by α·Δf (about 174 Hz in the typical case, so they count as variance), Kboost 0–10 dB in ¼ dB, n ≤ 255 per sub-band, N ≤ 765 per CM, 5.12 ms linear ramp [inferred from 128·8192 at 204.8 MHz], tones up to 17 dB above average subcarrier at the measurer; 500 ms DS recovery [PHY4 §7.2.6.2; MULPI4 §12.3.8.2]. Derived MER: `−10·log10(10^(−MER_CW/10) − 10^(−MER_pre/10)) − Kboost` [PHY4 App II].
- OUDP: full-channel pseudo-random bursts; measurer counts all subcarriers over M ≤ 1024 symbols with ZBL downstream; DPR protects other CMs; T 50 µs–5 ms guard. Uncorrectables are expected and PNM error counters do not update during the window [PHY4 §7.2.6.1, §7.2.8.1].
- Full mesh sets the baseline; partial refines; initial requires no traffic on that sub-band; periodic pairs opposite-RBA TGs (TG1 (1,0,0) tests against TG2, TG3) [MULPI4 §12.3.2–§12.3.5]. Pre-EC sounding sets IG only, not DS profiles [MULPI4 §12.2.1].
- Triggered RxMER, three triggers (OUDP, ECT probe, time), all subcarriers, u6.2 reporting, accuracy above 5 dB (non-PLC) or 15 dB (PLC) [PHY4 §7.2.8].

**CM echo cancellation** [PHY4 §7.2.7; MULPI4 §12.4]. Self-interference has two parts: **ALI** (in-band leakage from the CM's upstream into another sub-band's downstream, must be cancelled precisely) and **ACI** (stronger, adjacent, overloads the receiver, needs only rough cancellation). Reflection from beyond the first tap is too small to matter. Other CMs are not cancelled [PHY4 §7.2.7]. The test model is 75–150 ft Series 6 + 10 ft, 25 dB combined return loss [PHY4 §7.2.5.12.3].

EC is proprietary; only training support is specified [PHY4 §7.2.7.3]. Trained per **RBA sub-band direction set**: foreground (ECT probes at regular power; with ZBL it cancels both, without ZBL only ACI) or background (low-level signal in DS sub-bands, ALI only). Initial and periodic phases, expiry timers on both ends; on expiry the CMTS stops user traffic for that set only [MULPI4 §12.4, §12.4.4]. An FDX CM can hold up to 8 sets [CM-OSSI4 §F.2.3.14]. All-same-direction sets may report N/A.

**Node EC** is vendor-specific [MULPI4 §5.2.13]. Rough size [inferred]: at 684 MHz, 47.9 dBmV/6 MHz launch minus 18.7 dB return loss gives 29 dBmV of echo; for 1024-QAM at 7 dBmV/6.4 MHz needing 44 dB, residual must be −37, so about 66 dB of cancellation (64 at 108 MHz). Not from any source; bandwidths ignored.

**Entry sequence** [PHY4 Figure 24; MULPI4 §12.2.1]: register on non-FDX channels; add, range and probe all FDX US; add FDX DS; sound; assign IG, TG, RBA; EC training; steady state.

## 8. Plant requirements

**Node.** Launch amp raises Interface C to D; FDX node RL > 10 dB; FDD 14/13/12 dB active by band [PHY4 Table 18, Table 41]. FDX PHY tests assume one active leg, others terminated [PHY4 §7.2.4.14.2].

**FDD amps** (DS/US) [SCTE 279]: passband DS shall 108–1794, should reach down to 54 or 85; US should 5–684, shall 10–492 [§7.2.1, §7.3.1]. Gain anticipated: DS 48–50 dB at 1794 for high-gain, 10–13 dB booster; US 30–32 dB at 684, 4–7 dB booster [§7.2.2, §7.3.2, §10]. Flatness ±0.5 to 1218, ±0.75 to 1794; multi-frequency agile AGC with pilot-loss protection [§7.2.4, §7.4.4]. Upstream quasi-AGC or thermal [§7.5.3]. Return loss to ≥12 dB at 1280–1794 [§13.1]. Ingress switch recommended [§7.5.4]. Scope excludes EC and launch amps, and calls out boosters for long spacing [§1.2].

**FDX amps** [SCTE 296]: EC amps for N+X [§1.2, §6.1]. DS 102–1218, gain ≥47 dB multiport, ≥38 dB line extender at 1218 [§8.1.1, §8.1.2]. US 5–85 and 108–684, > 34 dB at 684 with +5 to +13 dBmV/6.4 MHz input, SNR > 37 LE, > 34 multiport [§8.2.1, §8.2.2]. RL ≥13–18 dB by band [§11.1]. Ports are **North and South** because direction no longer labels them [§5.2]. Transponder required, should be DOCSIS 3.0+ [§9.2]. Covers CFDX to 1.2 GHz only; 1.8 GHz is in progress [§1.2, §1.5]. Diplexers are incompatible with FDX, so plant is N+0 or uses diplexer-free or EC amps; N+0 was the original assumption, and FDX amps have since opened N+X [SCTE 300 §9.8.1.1; PNM-HFC §VIII.2.1.3.6.1].

**Power.** ≥15 A through current, 20–25 A peak; low-voltage cut-off hysteresis ≥3 V (279), ≥2 V (296); power and current tables in 5 V steps; D4.0 amps expected hotter [SCTE 279 §14.3–§14.6; SCTE 296 §12.3–§12.6; PNM-HFC §VIII.2.1.5]. Review supply configuration [SCTE 300 §7.6.2].

**Taps and passives.** Conventional taps are split-agnostic; RF bandwidth and return loss (especially above 1 GHz) are the concern. For FDX to 1.2 GHz, existing 1.2 GHz taps should work; for FDD to 1.8 GHz, faceplate or full replacement is likely, and some rated parts underperform [guideline] [TR-TAR §6.4.2]. Conditioned taps need new plug-ins [TR-TAR §6.4.3]. A legacy feeder example rolls off sharply near 1.3 GHz, driven by passives [field] [PNM-HFC §VIII.2.1.3.1]. Return loss, frequency response and port-to-port isolation across the full band are critical for both alternatives [guideline] [SCTE 300 §9.8.2]. Remove in-line filters and equalizers [TR-TAR §6.4.4; SCTE 300 §9].

**Cable.** Loss above 1 GHz and CM power limits together push cable size and condition; some coax will need replacing; check non-foam dielectric [TR-TAR §6.4.1; SCTE 300 §7.4e].

**Drop and premises.** Home run, tap → bonding block → CM; no splitters, amps or filters; 1.8 GHz splitter only if unavoidable; Series 6 tri-shield, Series 11 on long drops [guideline] [SCTE 300 §6.1, §7.11, §7.12; TR-TAR §5.2]. Standard D3.1 premises can stay where only D3.x service continues [TR-TAR §1.3.1]. MoCA overlaps FDD and must be removed or filtered, and a neighbor's unfiltered MoCA can interfere [SCTE 300 §7.11.1].

**Cascade.** No source sets a limit for FDD or FDX. N+0 is the best first candidate [TR-TAR §6.1.1]. N+X is an explicit choice in both alternatives [TR-TAR §1.3.2], pushing fiber deeper is a possible step rather than a rule [SCTE 300 §7.6], and operators have indicated chains up to six on D4.0 [field] [SCTE 300 §7.6.2]. SCTE 279 targets drop-in at legacy amp sites [SCTE 279 §1.2]. Group delay near band edges grows with cascade [SCTE 300 §9.8.2].

## 9. PNM and MIBs

**PNM is largely unchanged in form.** PHY defers to 3.1 [PHY4 §9]. A text diff of `docsPnm*` and `docsIf31*` names, 3.1 vs 4.0 OSSI, shows no new test objects on the CM and only `docsPnmBulkDataTransferCfgEntry` on the CCAP [inferred]. File headers keep Hz and channel ID: RxMER has subcarrier zero frequency, first active index, spacing; spectrum capture has first/last segment center, span, bins [CM-OSSI4 Table 91, Table 101]. Full-band capture must reach 1794 [PNM-HFC §VIII.2.1.1.3]; D3.1 modems stop at 1.2 GHz and D3.0 at 1 GHz, so visibility above 1.2 GHz depends on D4.0 penetration [PNM-HFC §VIII.2.1.6.1].

**New MIBs.**

| Module | CM objects | Notes |
|---|---|---|
| DOCS-FDX-MIB | `docsFdxCmFdxStatusTransmissionGroupId`, `...CurrentRbaSubbandDirSet`; `docsFdxCmEcTrainingStatus{EcTrainingStatus,EcTrainingPhase}`; `docsFdxCm{Ds,Us}OfdmChannelFdxEnabled`; `docsFdxCmUsOfdmaChannelExtendedUsChannelTxPower` | EC table FDX only, not FDX-L [CM-OSSI4 Annex A.3, §F.2.3.13, §F.2.3.14, §F.3.3.3.6] |
| DOCS-FDD-MIB | `docsFddCmFddSystemCfgStateDiplexer{DsLower,DsUpper,UsUpper}BandEdgeCfg`; `docsFddDiplexer*CapabilityTable`; `docsFddCmUsOfdmaChannel{IsExtendedUsChannel,ExtendedUsChannelTxPower}` | Values in MHz; 0 means not in extended spectrum [CM-OSSI4 Annex A.4, §F.2.3.12, §F.3.3.3.7] |
| CCAP | `docsFdxCmtsCmFdxStatus*` (TG, direction set, EC), `docsFdxCmtsTransmissionGroupStatus*`, channel `FdxEnabled` | [CCAP-OSSI4 Annex A.1, §7.2.1.14.4, §7.2.2.2.8] |
| Capability | `docsIf40CmCapabilitiesAdvBandPlanCapability`: supportsFdxL, supportsFdx, supportsFdd | [CM-OSSI4 §F.2.3.4.7] |

`RbaSubbandDirSetType`: one byte per sub-band, lowest first; 0 DS, 1 US, 2 undefined. EC status converged, notConverged, notApplicable; phase initial, periodic, notApplicable [CM-OSSI4 Annex E, §F.2.3.14].

CCAP-OSSI4 adds a YANG-modeled streaming telemetry PNM model pushed over gNMI, with FDX and FDD barely mentioned there [CCAP-OSSI4 §7.5.2, §7.5.4, §10]. OPT PNM exists to trigger MER and profile tests [CCAP-OSSI4 §7.3.6]. A DBC changing the RCS aborts active PNM tests [normative] [CM-OSSI4 §D.1.1]; DBCs are routine in FDX init and re-init [inferred].

**What's not reportable.** No OSSI reporting of IG membership, triggered RxMER (grep of both OSSIs finds no object) or EC residual and reserve. The PNM guide asks for echo before/after, margin, and per-leg node data as future objects [PNM-HFC §VIII.2.1.4.1–§VIII.2.1.4.4].

**Impact on tests** [PNM-HFC Table 46]. FDD: only more bins for FBC and UTSC. FDX in the FDX band: DS symbol capture, NPR, channel estimate need the RBA set to DS; FBC is dual-direction, with vendor filter differences; FEC summary is affected by RBA over minutes; upstream histogram and impulse noise are open or discouraged; US RxMER may read low when other TGs are DS; UTSC SID filtering needs TGs aligned; DS RxMER and MER margin are expected to be unaffected. FBC at a CM can see neighbors' upstream [PNM-HFC §VIII.2.1.6.2]. Smart amps add FBC and CM-based tests at amp locations, for both alternatives [PNM-HFC §VIII.2.1.3.6.2, §8.3.3]. Time-triggered RxMER coordinated with a second CM's burst is a candidate nonlinear and localization tool; sounding data may verify topology [guideline] [PNM-HFC §VIII.2.1.4.1].

## 10. MAC behavior seen as symptoms

| Symptom | Cause | Source |
|---|---|---|
| Registered but no upstream beyond low band for a while | Two-phase init on UHS: extended channels added by DBC after registration | [MULPI4 §13.2] |
| Partial service during FDX init, then recovery | Interference during DS acquisition; US ranging failure; DS lock failure; DBC-RSP or CM-STATUS | [MULPI4 §12.2.2.3, §12.7.1] |
| Channel gives no traffic though locked | EC not converged on current direction set; CMTS grants nothing until told | [MULPI4 §12.4.3] |
| Traffic drops on one direction set, others fine | ECT expiry on that set; recovered on retrain | [MULPI4 §12.4.4] |
| All FDX lost on one config change | OCD, FFT, CP or sub-band change forces FDX Reset and full band re-init | [MULPI4 §12.6] |
| Burst of uncorrectables that PNM counters don't show | Sounding under DPR; errors inhibited and counters paused | [MULPI4 §6.4.56; PHY4 §7.2.6.1] |
| Lower RxMER only when one sub-band is upstream | Self-ALI/ACI residual, or neighbor in another TG | [PHY4 §7.2.7; PNM-HFC Table 46] |
| Re-ranging on channel return after being away | Away > T4/2: unicast ranging; > T4: treated as new | [MULPI4 §12.5.1] |
| All US lost: legacy re-range first | Legacy US ranges first, then each FDX US | [MULPI4 §12.7.2] |
| CM-STATUS silence on FDX channel | DS events only when sub-band is DS; no secondary MDD events | [MULPI4 §12.7.3] |
| Channel dropped, partial service, lower bit loading | TCP or min-channel headroom exhausted | [SCTE 300 §7.14.3] |
| Neighbor video tiling, D3.x FEC on bursts | Front-end overload from high-power OFDMA, tied to duty cycle | [SCTE 300 §7.13] |

Note a D3.1 high-split CM gets no contention opportunities on 108–204 under UHS, so its requests go elsewhere; ranging only if the CMTS offers it [MULPI4 §13.1.5]. SC-QAM RxMER isn't reduced by the neighbor's burst even when tiling occurs [SCTE 300 §7.13.3].

## 11. Coexistence

- Any 3.0/3.1 segment can be upgraded without changing their services [TR-TAR §1.3.1]; D4.0 CMs run backward-compatible modes [PHY4 §7.2, §7.3].
- **FDD:** D3.1 and FDD CMs share 108–204 with different rules: CCAP manages minimum grants, D3.1 CMs lack contention and use legacy grant rules [MULPI4 §13.1.3, §13.1.5]. Above the split, D3.x are limited to their legacy range: under UHS-684 with an 834 edge, that leaves 384 MHz (834–1218) for D3.1 [inferred]; the top of the band is OFDM-only [PHY4 Figure 29].
- **FDX:** FDX CMs coexist with FDX-L in mid- or high-split and FDD-XL in UHS; all FDX-L in a service group are assumed to share diplexer edges [MULPI4 §12.2.1, §12.3.1]. D3.1 CMs without FDX-L software have undefined behavior in FDX spectrum [MULPI4 §12.1.2]. Mixed TGs or same-TG with `undefined` are both allowed [MULPI4 §12.5].
- **Neighbors.** Upstream bursts reach other homes through tap return loss, tap value, port isolation, splitter isolation and premises loss; these form the CACIR risk score. Each CPE model has its own threshold [SCTE 300 §7.13.4]. It applies to D3.1 high split as well as D4.0 [SCTE 300 §7.13.1]. Mitigations in order: fix wiring and levels; band-stop filter at CPE as last resort; reduce the interferer; swap CPE [SCTE 300 §7.13.5].

## 12. Migration practice

- **Prerequisites** [TR-TAR §1.4]: DAA, digital optics, PMA, channel moves, OFDM proficiency, mid- or high-split proficiency, topology with home-to-tap, IP video. These are recommendations, not hard requirements.
- **Variables** [TR-TAR §1.3.2]: FDX or FDD, spectrum top, which sub-bands bidirectional, N+0 vs N+X. Actives and passives go first; drops and premises are opportunistic [TR-TAR §5.1, §6; SCTE 300 §9].
- **Prioritize** by telemetry (RxMER 2nd percentile, FEC, bins, UTSC, US RxMER) with caveats that averages lose variance and current DS performance may not predict the same frequency as US [TR-TAR §6.1.1].
- **Predict upstream** from FBC: DS path loss over the future US band gives expected upstream level, so archive FBC and node configuration before migration; after a band turns US-only no DS remains to measure [SCTE 300 §7.13.3, §7.13.4].
- **Ingress changes direction:** frequencies converted from DS to US carry their ingress into the return path; in FDX it hits both; above 1 GHz is largely untested [SCTE 300 §9.1; PNM-HFC §VIII.2.1.3.2]. 108–137 MHz aeronautical leakage needs upstream-based detection, OUDP being a candidate [PNM-HFC §VIII.2.1.3.5]. Over-the-air users above 1 GHz include aeronautical bands, 1240–1300 MHz amateur and GPS [SCTE 300 §9.2; PNM-HFC §VIII.2.1.3.2].
- **Setup:** CCN not CNR as the figure for distortion; in all-digital plant each 1 dB level rise worsens CIN by 1–2 dB; step-down above 1 GHz offsets TCP [SCTE 300 §9.8.4].

## Implications for hfc-twin

All items below are our conclusions [inferred], grounded in the citations given. The test for (a): after shipping the wrong shape, can a mechanical migration carry artifacts forward and keep the code change local? If yes, it is not (a).

### (a) Must be right in step 1

- **A1. Component ratings replace `qualifies_for: Vec<ProfileId>`** in `component`. Add `rated_lo_mhz`, `rated_hi_mhz` to each RF spec, and to Amp/Node `duplex: Diplexed { splits: Vec<(us_hi_mhz, ds_lo_mhz)>, remote_switchable: bool } | FullDuplexEc`. Qualification becomes a computed function of component and profile. Why: taps are split-agnostic and fit by bandwidth [TR-TAR §6.4.2]; amps differ by split options and EC [SCTE 279 §7.1.1; SCTE 296 §1.2], which class `Amp` and `allowed_component_classes` can't express. A stored list is a derived value kept as input (invariant 1), needs editing on every new profile, and has no mechanical migration: ratings must be re-collected from datasheets after step 9's data entry. Tag the same fields `Generic` where unknown.
- **A2. Per-channel observation records are self-describing** in `observation`. Key by stable `channel_id`, not plan position. Subcarrier arrays carry `zero_freq_mhz`, `first_active_index`, `spacing_khz`; `fbc_dbmv` carries first center and bin width; each record carries `profile_ref`. Why: channels move in migration [TR-TAR §1.4.3; SCTE 300 §7.6.3]; generations see different subsets on one plant and different FBC ranges [PNM-HFC §VIII.2.1.6.1]; models must compare across profiles by frequency (DESIGN holds out by profile). Sim data can be regenerated, but real telemetry from step 7 can't be repositioned without the plan then in force. The real files already carry these [CM-OSSI4 Table 91, Table 101]. Also needed today for D3.0/D3.1 mixes.
- **A3. Guard, no change: edges keep port identity** on taps and actives, as Task 01 has it ("every tap port used at most once"). Don't flatten to `tap_id` on the drop. Port-to-port isolation and FDX North/South need it [SCTE 300 §7.13.4; SCTE 296 §5.2], and it can't be rebuilt later.

### (b) Must be right in the sim's architecture (steps 3, 4)

- **B1. Port-to-port path engine.** Implement `transfer(port_a, port_b, f_mhz)` over the tree in either direction; §3.2 and §7.1 become special cases. CM-to-CM coupling is needed for FDX CCI [MULPI4 §12.3] and for neighbor overload on any high-split plant [SCTE 300 §7.13]. Measure the budget at step 3 with modem-pair coupling in mind: precompute per tap pair plus drops, not per modem pair.
- **B2. Direction comes from each element and each modem, not global edges.** Use `carries(element, f, direction)` in walks. On one plant D3.1 and FDD CMs have different diplexers and DS reach [MULPI4 §13.1.1, §6.4.28.1.20]; in the FDX band one frequency has both directions [PHY4 §7.2]; amp diplexers can differ during rollout [SCTE 279 §7.1.2].
- **B3. An interval is a set of conditions, not one evaluation.** Evaluate per RBA direction set and traffic condition, and combine FEC counts by occupancy weight. RBA can change faster than a telemetry step [MULPI4 §6.4.51; PNM-HFC §VIII.2.1.4.2]. IG, TG and RBA are derived inside the sim, never inputs (invariant 1), from path losses plus a deterministic policy, since real algorithms are vendor-specific [MULPI4 §12.3]. Traffic demand is an input not on invariant 1's list: add it to the environment or fix the demand in the profile, and change the invariant's wording.
- **B4. Echoes are referred to a port.** §5.1 computes a cavity per modem path. FDX needs `s11(port, f)` summed across the tree at the node or amp port [PHY4 §7.2.4.14.2, Table 14] and at the CM's first tap [PHY4 §7.2.7]. Add residual terms to §7.3 `N_node` and §4.5 for the state where a sub-band is up. EC depth is a parameter, `Generic`, since it's proprietary. A connector fault then raises upstream noise for every modem on the leg via the node residual, which is a new diagnosis pattern.
- **B5. Transmit power per TCS, with a frequency-dependent ceiling.** Replace the one check in §7.2 with: per-TCS budget, `Pref(f)`, minimum-channel headroom before TCP, dropping channels into partial service [PHY4 §7.3.4.12; SCTE 300 §7.14]. UHS-684's ceiling is 3 dB below UHS-492's.
- **B6. Output level is a function of frequency.** Route all uses of §3.1 through `out_level(active, f)`, allowing steps and cable shape, with TCP computed from it and checked as a constraint [PHY4 §7.2.5.9.1, App IV; SCTE 279 Table 2]. Scale distortion from TCP, not a single reference level [SCTE 279 §7.2.8].
- **B7. Validity is decided at emit time.** A capability mask is static; FDX availability isn't. Fields are `None` when the sub-band is the wrong way, and counters pause during sounding [PNM-HFC Table 46; MULPI4 §6.4.56]. `Option` already fits; the emit path must take the condition.
- **B8. Table lookups error outside the rated band.** §2.1's hold-flat rule is unsafe when legacy passives collapse above 1.3 GHz [PNM-HFC §VIII.2.1.3.1].

### (c) Can wait: a new schema version or profile data file

- **`split` → `band_plan`.** A list of segments `{lo_mhz, hi_mhz, direction: Us|Ds|Both, tcs, modem_classes}` plus transition bands. Today's four edges can't express 5–85 + 108–684 or overlap, but the JSON migrates mechanically and the code risk is B2, so it isn't (a). It costs one type today; take it then if convenient.
- **Ingress variants merge.** One `Ingress` source whose reach follows the band plan, replacing separate upstream and downstream variants, so a profile switch keeps the same physical fault (DESIGN Reactivity; [SCTE 300 §9.1]). A lossless mapping of `CauseClass`; bites at the first split what-if in v2.
- **Modem generation on the Modem spec**: `kind: D30|D31|D31FdxL|D40Fdd|D40FddXl|D40Fdx|D40Unified`, diplexer options, and effective capability = profile ∩ modem. `profile.docsis` then describes the plant and CMTS.
- **Component fields:** tap `port_isolation`, `directivity`, `return_loss`; amp `north/south` semantics, EC depth, US slope, CCN vs TCP table; `power_w` vs voltage table and `v_turn_on`; cable table to 1794.
- **Modem limits:** `tx_max_*` scalars → per-band table from §5.
- **`design_levels`:** `out_profile` with steps, `tcp_max_dbmv`, per-modulation node set point; `max_cascade` is a data value. Modulation thresholds become level-dependent per the table in §4.
- **Observation additions:** `us_tx_rel_ref_db` for extended channels, condition key, `tg_id`, `rba_dir_set`, `ec_status`, `ec_phase`, triggered RxMER, active-point FBC. IG pairs as ground truth only.
- **Faults:** diplexer mismatch at an active, EC non-convergence cause, low tap isolation.
- **Thresholds:** `docs/physics.md` §4.6 by plant top edge; PHY4 Table 46 asks 2–5 dB more CNR than Table 45 at 16–1024-QAM.
- **Channel plan:** sub-bands and allocated spectrum; `node_kind` adding RMD; D4.0 profiles having only a DAA variant.
- **No change:** `Timestamp` in seconds; interval conditions (B3) don't need a sub-second timeline. Tree topology stays. Service group as the recompute unit stays, since CM-to-CM coupling stays within it, though funneling and EC residual are per root port (B4).

### DESIGN.md profile table against sources

| DESIGN value | Sources | Verdict |
|---|---|---|
| ESD 5–684 up, 834–1794 down | One of five splits; DS lower edge 684–834; D4.0 CM up is 5–85 + 108–684; D3.1 5–204 [PHY4 Table 29, §7.3.2.3] | Conservative instance, not the plan. Up is two bands; add the other splits |
| ESD N+3 | No source sets it; operators cite to six; N+X is a choice [SCTE 300 §7.6.2; TR-TAR §1.3.2] | Our design choice; say so and derive from CCN |
| "Requires 1.8 GHz amps and taps" | Conditional on DS above about 1.2 GHz; plant may stop at 1218 [PHY4 §7.3.5.12.1; TR-TAR §6.4.2]. Amps must support the chosen split in any case [TR-TAR §6.4.5] | Overstated |
| FDX 108–684 shared, down to 1218 | CFDX only; SFDX is 258–834; DS to 1794 possible; allocated 96–576; legacy up omitted [PHY4 §7.2.2] | Incomplete |
| FDX N+0, "no amps" | FDX amps exist with EC [SCTE 296; SCTE 300 §9.8.1.1; TR-TAR §6.4.5]; static FDX through high-split amps [MULPI4 App XIV] | Contradicted; N+0 is simplest, not required |
| "Each profile has a DAA variant" | D4.0 assumes distributed; no analog variant [PHY4 §7.2, §7.3] | For D4.0, DAA only |

### "A profile is a data file; adding one adds no code"

ESD: true after one code change, then all five splits and DS edges are data. That change is B2 and B5 (two TCS, tilted ceiling), so high-split D3.1 is data today and the first UHS profile isn't. FDX: false for the first profile. Port-to-port coupling is general, but the scheduler, IG and TG derivation, EC residuals and condition-dependent telemetry are mechanisms with no D3.1 equivalent. After they exist, CFDX vs SFDX, sub-band layout and static vs dynamic are data. Suggested wording: a profile selects among implemented duplex mechanisms; adding one adds no code unless it needs a new mechanism.

## Gaps

- **EC depth:** no source gives node, amp or CM cancellation in dB; only pass/fail test conditions. Any figure is `Generic`.
- **CM-to-CM isolation:** no numeric tap isolation, directivity or return loss to 1794 in these sources; `docs/physics.md` §2.1's ≥20 dB is marked approx. No tap or passive spec above 1.2 GHz here, and no value for how CM-to-CM energy crosses an FDX amp.
- **FDX on amps:** SCTE 296 doesn't say how IG behavior changes across amps, or whether sounding spans them. Only CFDX to 1218 is covered; SFDX and 1.8 GHz FDX amp values don't exist in the set.
- **TG/RBA algorithms:** vendor-specific; no public policy to model. Our policy is a guess.
- **No OSSI for IG, triggered RxMER or EC residual:** verified by search in CM-OSSI4 and CCAP-OSSI4; the field set may change in future issues. `ExtendedUsChannelTxPower` units aren't shown in the text.
- **Spec anomalies**, printed as extracted and checked in the PDF for Table 14: PHY4 Table 14 SFDX row reads "254 / 15,2", presumably 354 MHz, 15.2 dB [inferred]. Table 8 header says 833.8 where text says 833.2. Neq is 256 and 281 within §7.3.5.9.1.1: matching 258–1794 and 108–1794 [inferred]. CCAP-OSSI4 §6.5.4.12.3 states 108 MHz as the only lower edge, not updated for SFDX. PNM-HFC quotes CM total input over 54–1794 from I07, I08 says 108–1794.
- **Not distilled:** spurious tables (PHY4 Tables 9, 10, 20, 34, 35, 43), Annex A detail, CCF/YANG models beyond existence, and most of MULPI4 message encodings. RBA and ECT timer detail exists in MULPI4 Annex B but only refresh and gap values are recorded here.
- **Multi-leg nodes:** per-leg EC and funneling vary by vendor [PNM-HFC §VIII.2.1.4.3]; whether one plant file must split into several service groups for 4.0 is open.
