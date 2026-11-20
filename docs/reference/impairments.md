# Plant impairments

Catalog of HFC plant impairments for the sim and diagnoser: cause, signature per measurement, look-alikes,
localization, thresholds, severity and repair.

Units: MHz, ft, dBmV. dBc is relative to the main signal. NDI is noise, distortion and interference. FBC is full-band
capture. SID is the CableLabs spectral impairment detector (NDA library). Figure readings are approximate. PNM-HFC
§7.1 and §7.2 derive from SCTE 280 and SCTE 294, which we have not read [PNM-HFC §7.1, §7.2].

## 1. Signature summary

Condensed from [PNM-HFC Table 24] [guideline]. The usual service effect of all of these is failed SC-QAM channels,
lower OFDM/A bit loading, bit errors and profile changes.

| Impairment | Common location | FBC shape | RxMER | Other |
|---|---|---|---|---|
| Adjacency | Headend, hub, field combiner, RPD | Step between channel groups | Differs by channel | FEC when severe |
| Filter | Tap, drop, set-top | Band missing | Roll-off at edge | Phase distortion at edge |
| Suckout | Amp internals, tap, hardline | Narrow concave notch | Dip | US pre-eq and headroom stressed; DS equalizer stressed |
| Ingress | Inside wire, drop, connector | Excess energy in bands | Abrupt dip | FEC |
| CPD | Corroded hardline contacts | Raised US floor | US degraded | Variable levels |
| Resonant peak | Amplifier | Peak over a limited range | Dip around peak | FEC |
| Roll-off | House wiring, drop, amp | Falling band edge | Roll-off at edge | Phase distortion at edge |
| Ripple | House wiring, drop, hardline | Periodic, most of band | Drops when uncompensated | Ripple in equalizer if compensating |
| Water | Drop, tap, hardline | Non-periodic ripple, HF worst | Degraded | Equalizer affected |
| Tilt | System | Linear slope; band-edge clipping if excessive | Only if excessive | |
| Flat loss | Drop, tap, amp | Uniformly low | Drops | Transmit levels high |
| Pullout | Hardline | Gradual roll-off toward LF | Drops | Transmit levels high |

## 2. Reflections, standing waves and micro-reflections

- **Cause.** Two impedance mismatches form an echo tunnel; the delayed copy interferes with the main signal [PNM-HFC
  §5.2.6.11, §IV.3.2]. "Standing wave" is the common but loose name for the resulting amplitude ripple;
  "micro-reflection" just means small delay [PNM-HFC §7.1.7, §6.2.1; SCTE 183 §9.1]. Sources: damaged or missing
  terminators, loose seizure screws, unterminated tap ports, poor tap or splitter isolation, kinked or cracked cable,
  water-damaged or corroded devices, cable-ready TVs, traps in their stop band [PNM-HFC §I.5.2].
- **Distance.** `D_ft = 492·VF / Δf_MHz`, with Δf the crest or trough spacing [PNM-HFC §5.2.6.11; SCTE 183 §9.5].
  Oracles at VF 0.87: 8 MHz is 53.5 ft [PNM-HFC §5.3.4.2]; 54 MHz is 7.9 ft [PNM-HFC §IV.3.2]. This is the spacing of
  the reflectors, not their place in the plant [PNM-HFC §6.2.1.1.1]. For in-or-near-home calls one reflector is
  assumed to be in the home, usually correctly [PNM-HFC §8.12.1].
- **Amplitude.** −18 dBc gives 2.2 dB ripple; −25 dBc gives 1 dB [PNM-HFC §8.11]. From ripple, `dBc = 20·log10((E_P −
  E_N)/(E_P + E_N))`; 36.63 and 26.61 mV give −16 dBc and 2.78 dB [PNM-HFC §I.6]. Worked example: two water-damaged
  taps 100 ft apart, return loss degraded from new 15–18 dB to 7 dB, 1 dB cable loss, give a −16 dBc echo [SCTE 183
  §9.1] [field]. Components range from a few dB to 30+ dB return loss [SCTE 183 §9.1].
- **What a modem is built to work through.** DOCSIS assumes a downstream dominant echo of −10 dBc at up to 0.5 µs,
  −15 dBc to 1.0 µs, −20 dBc to 1.5 µs and −30 dBc beyond, and the cable modem's receiver is specified against that
  channel [DOCSIS 3.0 PHY, assumed downstream RF channel characteristics] [guideline, unverified]: these four rows come from
  search summaries of secondary sources (an IEEE 802.3 EPoC deck based on DOCSIS 3.0, and DOCSIS 1.0 and 2.0 guides) whose
  PDFs couldn't be read to confirm them, and the specification itself wasn't read. The downstream receiver's adaptive
  equalizer is blind (DOCSIS sends no training sequence) and its span is what sets the longest echo it can compensate
  [Hranac, "How Adaptive Equalization Works", broadbandlibrary.com] [guideline]. That page gives no cancellation depth and no
  downstream span.
- **Geometries** [PNM-HFC §6.2.1.1]:
  - *Multiple transit:* both reflectors on the path to the CMTS. Endless decaying echoes, each smaller by Γ1Γ2; the
    equalizer needs one post-main tap.
  - *Single reflection via tap isolation:* the reflector is **off the modem's path**. Signal leaks out the tap's
    output port (23 dB tap with 38 dB isolation, so 15 dB down), hits an open port or line end, and returns. One echo
    in the channel; the equalizer shows a decaying tap sequence, and leftover energy lowers RxMER. Most visible on
    high-value taps with long spans behind them.
  - *Composite:* poor amp return loss plus an open port; if a pole span apart, one open port makes two echoes, usually
    30+ dB down but numerous. Clean single echoes may exist only in labs.
- **FBC.** Evenly spaced scallops, sinusoidal in linear power [PNM-HFC §IV.3.2]; often invisible inside one 6 MHz
  channel [PNM-HFC §5.2.6]. Non-uniform: nearly 20 dB peak-to-null at 100 MHz, fading to about 5 dB at 800 MHz,
  because the echo path attenuates [PNM-HFC §7.1.7.2] [field]. More mismatches superimpose ripples [PNM-HFC §7.1.7.1].
- **RxMER can stay flat under real ripple.** The receiver divides by H, so `e = n1 + n2/H`, with n1 noise before the
  reflection and n2 after. Noise before: RxMER = SNR1, no ripple. Noise after (e.g. modem noise figure): RxMER =
  |H|²·SNR2, full ripple. Mixed: partial, and low level can hide it [PNM-HFC §5.3.4.2]. Lab: channel estimate 7 dB p-p
  and 134.7 ns group delay p-p, with RxMER mean 34.6 dB, std dev 0.59 dB [PNM-HFC §5.3.4.2 Figs 65–66] [field]. Ripple
  in RxMER is evidence; its absence proves nothing. `1/RxMER = 1/SNR1 + 1/(|H|²·SNR2)` is our algebra from that error
  equation [inferred]. CP beyond the taper allowance compensates echoes [PNM-HFC §8.13.1].
- **Pre-eq (SC-QAM).** Fractional delays spread to neighboring and some pre-main taps [PNM-HFC §6.2.6.3.12.3].
  Reflection level uses taps 9–24; group delay uses taps 1–7 [PNM-HFC §8.11]. A 0.5 µs reflection gives negative PPESR
  and high PostMTTER (N+0: MTC 0.79 dB, NMTER −7.78 dB, PPESR −9.84 dB) [PNM-HFC §6.2.6.3.12.3 Table 16] [field].
  Pre-eq hides in-channel response from spectrum displays, so read coefficients [PNM-HFC §7.2.2]. Deep notches or long
  echoes can leave no solution, and impulsive NDI causes unstable adaptation [PNM-HFC §8.12].
- **Band-edge confounder.** Diplexer group delay fills pre-main taps with no reflection. At 40.4 MHz, 3.2 MHz wide:
  N+0 NMTER −29 dB; N+5 NMTER −15.9 dB, PreMTTER −16.3 dB, PostMTTER −25.9 dB, PPESR +9.5 dB, MTC 0.11 dB [PNM-HFC
  §6.2.6.3.12.1 Table 12] [field]. Positive PPESR marks group delay, negative marks reflection [PNM-HFC §6.2.6.3.12].
  Other sources: AC coils (5–10 MHz), filters, roll-off [PNM-HFC §I.4].
- **Reflector type.** Echo phase against the main tap acts like a TDR: in phase is open, 180° is short. Missing
  terminators and radial cracks read open; a dented buried cable reads short; water reads repeating shorts; a
  wind-moved crack alternates [PNM-HFC §9.6.3] [field]. A 3 dB pad at the modem drops the reflection by 6 dB or more
  if the modem is a reflector [PNM-HFC §6.2.1.1.1].
- **Thresholds** [guideline]. Reflection: green below −25 dBc, yellow to −18, red above [PNM-HFC §8.11.2]. NMTER for
  64-QAM: yellow −30 dB, red −27 dB. MTC above 2 dB suggests equalization is running out and costs that much receive
  power [PNM-HFC §6.2.6.3]. Field: a cracked trunk gave a 9T echo of about 1 dB, cavity about 795 ft ±85 ft; OFDM
  estimates resolved six echoes [PNM-HFC §9.7.1]. **Repair:** terminate, tighten or replace [inferred from PNM-HFC
  §I.5.2]; pre-eq lowers urgency but does not replace repair [PNM-HFC §8.11].

## 3. Water

- **Cause.** Water changes the dielectric constant, hence velocity, impedance and attenuation. It distributes
  unevenly, so these vary randomly along the cable [PNM-HFC §7.1.7.3]. It migrates to taps, splitters, splices,
  filters and pads; gravity took one drop's water into the tap below [PNM-HFC §7.1.6.4] [field].
- **Signature.** Non-periodic waves with attenuation rising strongly with frequency; HF pockets and sometimes large
  tilt on drops; hardline differs somewhat, undescribed [PNM-HFC §7.1.7.3]. Water-soaked tap: roll-off from
  about 500 MHz to nearly 30 dB at 750 MHz [PNM-HFC §7.1.6.4] [field]. Drop: about 20 dB down by 700–850 MHz with deep
  pockets [PNM-HFC Fig 212, read from figure]. DS power and RxMER degrade; **upstream is relatively unaffected**;
  repair restored 40+ dB RxMER [PNM-HFC §5.2.6.12.1.1] [field]. Bench: S21 smooth when dry, variable and lower when
  wet; S11 higher; group delay uneven (100 ft Series 6) [PNM-HFC App VI].
- **Time.** Coincides with rain, can clear completely when water drains or evaporates [PNM-HFC §7.1.7.3]. Signature
  weakens when frozen, and freeze-thaw worsens damage [PNM-HFC §5.2.6.12.1.3]. Physical damage only grows; RF impact
  can recover [PNM-HFC §VII.5].
- **Versus ripple.** IFFT of the spectrum, autocorrelate, sorted CDF. Standing wave: one strong time peak, CDF with
  initial spike. Water: spread energy, CDF starts lower and climbs faster [PNM-HFC §5.2.6.12.2]. Thresholds are in
  [Fox et al.], not read.
- **Localization and repair.** Likely a small hardline span or one drop, isolated by comparing FBC [PNM-HFC App VI].
  Severity ladder: wave detection, water classification, frequency and power, MER, uncorrectables, partial channel,
  packet loss, resets [PNM-HFC Fig 44]. Replace the drop tap to ground block, or the faceplate [PNM-HFC §5.2.6.12.1.1,
  §7.1.6.4] [field].

## 4. Suckouts and notches

- **Cause.** Three mechanisms [PNM-HFC §7.1.3, §IV.3.4]: half a period of long-period ripple from a very short
  reflection; an accidental series RLC from grounding in a housing; periodic discontinuities, each adding width and
  depth. Mold spike: disk-dielectric hardline with an SRL spike at 500–600 MHz, later above 1 GHz [PNM-HFC §7.1.3.2].
  Craft: loose seizure, module or lid screws, wrong pin length, kinks. Manufacturing: cold solder, casting, module
  grounding [PNM-HFC §7.1.3.1, §7.1.3.2].
- **Size** [field]. About 20 dB near 715 MHz, −3 dB width about 50 MHz, Q = f0/BW = 14.3; severe wide case 210 MHz at
  560 MHz, Q = 2.67 [PNM-HFC §IV.1, §IV.4]. About 18 dB at 697–731 MHz, six QAM channels [PNMP §7.2.2]. An open λ/4
  stub notches at odd multiples of its fundamental [PNM-HFC §IV.5.1].
- **Signature.** Vee-shaped in FBC [PNM-HFC §IV.1]; RxMER dip; upstream pre-eq and headroom stressed; FEC affected
  [PNM-HFC Table 24 row 3]. Impact depends on depth, width and what is carried there [PNM-HFC §7.1.3].
- **Look-alikes.** Ripple troughs and vacant channels are not notches; an upper-edge suckout showing one side looks
  like roll-off [PNM-HFC §IV.3.2, §IV.3.3, §7.1.6.4]. A decline continuing over channels separates it from adjacency
  [PNM-HFC §5.2.6.5.1]. Intentional notches come from filters; positive traps are high-Q [PNM-HFC §IV.3.3].
- **Detection.** SID `SuckoutIdentifier` tunes minimum delta, search range, slopes, adjacent drop, averaging window,
  run length, channel gap and slope plus AVp-v. No defaults are published, and these alone do not separate suckout
  from ripple [PNM-HFC §5.2.6.5.1]. **Localization:** anywhere on the path; compare modems across the node [PNM-HFC
  §7.1.3]. **Repair:** fix the grounding or craft fault [inferred from PNM-HFC §IV.6].

## 5. Resonant peaking

- **Cause.** Frequency-specific gain from an oscillation inside an active, via feedback from poor grounding: module
  seating, oxidation, loose screws, worn ground springs [PNM-HFC §7.1.5, §7.1.5.1]. Almost always an amplifier
  [PNM-HFC §VII.7]; a faulty passive can look similar [PNM-HFC §7.1.5; PNMP §7.2.7]. Amps with the same peak add,
  raising and widening it [PNM-HFC §5.2.6.8].
- **Signature** [field]. Convex bump, e.g. at 716 MHz with four channels either side affected and a 17 dB rise; over 3
  dB across a channel starts impairment; above −15 dBmV/30 kHz FBC flat-tops it [PNM-HFC §5.2.6.8]. Usually a handful
  of channels [PNMP §7.2.7]. Affects levels, phase, group delay and RxMER [PNM-HFC §7.1.5]. On an AGC pilot the amps
  respond dB for dB across the spectrum; off it, damage appears in CSO/CTB and RxMER [PNM-HFC §5.2.6.8, §7.1.5].
- **Other.** Intermittent, can move, temperature dependent, may include a suckout [PNM-HFC §7.1.5]. Misalignment looks
  similar with a different cause [PNM-HFC §5.2.6.9]. Several modems with one peak point to a common device [PNM-HFC
  §5.2.6.8]. **Repair:** reseat module, tighten hardware, replace ground spring; bumping is usually temporary [PNM-HFC
  §7.1.5.1].

## 6. Roll-off, pullout, tilt, flat loss

- **Roll-off.** Gradual nonlinear fall, usually toward 1 GHz. Most often water; also parts with a lower pass band
  (450–870 MHz) whose effects add and sharpen, under-rated equalizers, bandwidth-limited seizures, old cable, filters
  [PNM-HFC §5.2.6.10, §7.1.6]. Look-alikes: high-end suckout, short-distance ripple [PNM-HFC §5.2.6.10]. RxMER
  roll-off at OFDM edges comes from taper, CP, adjacent energy, total power and silicon, not plant [PNM-HFC §5.3.4.4].
  Anecdotally prevalent, never quantified [PNM-HFC §7.1.6.4].
- **Pullout.** Hardline contracts in cold and pulls from the connector, separating shield or center conductor [PNM-HFC
  §5.2.6.15, §7.1.10]. Capacitive coupling gives a gradual LF roll-off like a round-shouldered high-pass filter
  [PNM-HFC §5.2.6.15]. RxMER drops and **transmit levels rise** [PNM-HFC Table 24 row 13]. Cold correlation and
  upstream severity follow, and repair re-seats the connection [inferred].
- **Tilt.** Level difference low to high; not necessarily a fault. Added on purpose at the node; a far modem can see
  negative tilt within design [PNM-HFC §5.2.6.13]. Causes: measuring near amps or line ends, poor setup [PNM-HFC
  §7.1.8]. Excess reduces resolution or clips at band edges [PNM-HFC Table 24 row 11]. Limits: none in North America
  beyond the input window; Europe 12 dB; SID `TiltIdentifier` maximum absolute 15 [PNM-HFC §5.2.6.13, §5.2.6.13.1].
- **Flat loss, leveling.** Uniform shift against peers, from excess splits, wrong couplers, pads or a misset amp
  [PNM-HFC §5.2.6.14, §7.1.9]. Check channel power within −15 to +15 dBmV and compare to proximal modems and another
  leg; it is comparative, outside SID. Examples: about 10 dB high at one modem, about 5 dB low across several. Often
  seen, rarely service-affecting [PNM-HFC §5.2.6.14] [field]. **Repair:** remove splitters, correct pads [PNM-HFC
  §7.1.9].

## 7. Adjacency, filters, missing channels

- **Adjacency.** Channel blocks combined at wrong levels, almost always headend or hub; also field combiners, RPDs,
  narrowcast overlays, MDU local insertion [PNM-HFC §7.1.1, Table 24]. Lower channels may show poor MER; a high
  channel's mask energy hurts neighbors [PNM-HFC §5.2.6.3]. Reference: FCC analog adjacent within 3 dB; good practice
  digital within 1 dB at the headend [PNM-HFC §5.2.6.3] [guideline]. Example about 8 dB [field]. **Localization:** the
  same step on all modems under a transmitter; not field-correctable [PNM-HFC §5.2.6.3].
- SID `AdjacentIdentifier` defaults [PNM-HFC §5.2.6.3.1] [guideline]: delta 2.9 to 14 dB; maximum average amplitude
  difference 4 dB (Maximum ICFR in PNM-3.1); maximum slope 1.75; individual high tilt 2.5 over 3 channels; cumulative
  high tilt 4.0; trigger range 110–700 MHz; discard if over 10 high-tilt or 9 high-delta channels.
- **Filters.** Video traps and data-only filters are band-stop, at the tap or drop; noise filters are high-pass,
  blocking upstream to protect the funnel, masking rather than fixing a shield fault; MoCA filters sit at the tap or
  house [PNM-HFC §5.2.6.4, §7.1.2]. Signature: channels absent at one drop; negative traps about 6 MHz; one bandstop
  took about 40 channels [PNM-HFC §IV.3.3] [field]. Stop-band return loss is low, so echoes and edge group delay can
  accompany them [PNM-HFC §I.5.2, §I.4]. A modem whose diplexer fails to switch looks filtered [PNM-HFC §7.1.2.3].
  **Repair:** remove [PNM-HFC §5.2.6.4].
- SID `MissingChannelsIdentifier` [PNM-HFC §5.2.6.4.1]: minimum 68 digital channels; amplitude threshold 18.0 for
  active; −35 minimum dB for uniformly low devices, which may indicate a filter; 575 MHz old-plant cutoff.

## 8. Downstream ingress, LTE, direct pickup

- **Cause.** External signals couple onto the shield as common-mode current and cross at a defect: loose connectors,
  cracked shields, rodent chews, warped lids [SCTE 209 §8; PNM-HFC §7.1.4]. Usually at the premises [PNM-HFC
  §5.2.6.6]. Drops suffer more because levels are lower; hardline is worst near an amp input; an open lid can affect
  everyone below [SCTE 209 §8].
- **Bands.** FM 88–108 MHz, odd tenths, 200–300 kHz wide [PNM-HFC §7.1.4.2]. Cellular above about 600 MHz; LTE/5G
  614–806 MHz [PNM-HFC §7.1.4.3; SCTE 209 §8]. VHF to 300 MHz; UHF to 3 GHz [PNM-HFC §7.1.4.4]. Defects are
  frequency-selective: little or no VHF-to-UHF correlation at one point [SCTE 209 §8; PNM-HFC §7.1.4.8], yet FM, VHF,
  UHF and cellular often appear together [PNM-HFC §7.1.4.2]. Operators' own FM-band carriers can mimic it [PNM-HFC
  §7.1.4.2].
- **Signature.** FBC spikes when strong; in guard bands or hidden under carriers when not [PNM-HFC §7.1.4.8; SCTE 209
  §8]. RxMER per subcarrier is the sensitive view: abrupt drop in-band, e.g. near 740, 760 and 870 MHz [PNM-HFC
  §8.7.3], or from about 40 to 34 dB at 890 MHz [PNM-HFC Fig 62] [field]. Inverted RxMER reads as the equalized NDI
  floor [PNM-HFC §5.3.4.1]. High FM relative to QAM marks a defect very near the receiver, such as a loose modem
  connector [PNM-HFC §9.7.2] [field]. A cracked trunk showed small FM only beyond the fault, plus echo and leak
  [PNM-HFC §9.7.1] [field].
- **Whale plots** [PNM-HFC §V.3] [field]. Lab, 0 dBmV; correctable and uncorrectable % against MER, read from figures:
  - *Flat AWGN, 4096-QAM:* correctables fall from 100% at about 42 dB to near 0 by 46 dB; uncorrectables start just
    below 36 dB CM-reported. Under 0.5 dB more noise loses sync. Receive level (−10 to +10 dBmV) did not matter.
  - *8 MHz LTE in 96 MHz (about 8%):* uncorrectables near 15 dB CM-reported notch MER, then sync loss.
  - *8 MHz LTE in 192 MHz (about 4%):* 4096-QAM holds until near 8–9 dB notch MER; 256-QAM to about 2–7 dB. So notch
    MER is not comparable to the flat threshold [inferred].
  - *CM vs instrument:* the VSA reads notch threshold about 13 dB (±1) above what the modem reports.
  - *Channel-average RxMER:* only about 2 dB from first correctables to full uncorrectables. Raising the floor made no
    difference; LTE level dominated [PNM-HFC §V.3.2, §V.3.3].
- Guide's rules [guideline]: 100% correctables are normal within about 6 dB of threshold, ignorable on a stable plant,
  but any correctables plus in-channel ingress should be addressed. LTE over about 5% of the OFDM bandwidth is more
  likely to hurt; use profile management [PNM-HFC §V.4; PNM-3.1 §II.4].
- **Direct pickup.** Interference enters CPE via case slots, seams or poor common-mode rejection, with no plant
  defect; older devices are worse [SCTE 209 §9; PNM-HFC §7.1.4.1]. An LTE handset at up to +23 dBm gives over 2 V/m at
  1 m. Effects: one channel, or all on fundamental overload; packet loss on modems [SCTE 209 §9]. Newer CPE helps only
  if attached cables are also well shielded [SCTE 209 §9]. Test jumpers can also pick up signals and mislead [SCTE 209
  §8].
- **Leakage.** Same defects, opposite direction. PNM cannot measure it, but ingress implies likely leakage [PNM-HFC
  §8.7.2]; there is still no correlation of levels [SCTE 209 §8]. Limits: 17.4 µV/m at 3 m in 108–137 MHz; 13.1 µV/m
  at 30 m in UHF [SCTE 209 §6.3].
- **Localization and repair.** Only modems below the entry see it; the fault lies between the last device showing it
  and the first not [PNM-HFC §8.8.1, §8.9]. Example: LTE Band 5 on all modems below one amp, none on the sibling
  output, a tower beside that amp [PNM-HFC §8.9.1.2] [field]. Field: halve with a meter, then near-field probe [SCTE
  209 §8]. Restore shielding; abandoning channels is short term [SCTE 209 §8].

## 9. Upstream noise, ingress and interference

- **Funnel.** Ingress anywhere reaches all upstream at the node, unscheduled [PNM-HFC §8.8.1.1]. Symptoms: lower SNR
  or OFDMA RxMER, more corrected and failed codewords, per channel; bonded modems move to partial mode [PNM-HFC
  §8.8.4]. Example: bursts at 2–16 MHz, SNR 29.2 dB [PNM-HFC §8.8.5] [field]. Vacant-spectrum ingress is lower
  priority but still a shield problem [PNM-HFC §8.8.10].

| Type | Signature | Source |
|---|---|---|
| Impulse, burst | 1 µs to tens of µs; **codewords degrade while RxMER looks normal** | [PNM-HFC §7.2.1.1] |
| Narrowband (CB, ham, SW, FM) | Under 1 kHz to tens of kHz; flags shielding | [PNM-HFC §7.2.1.2, §8.8.5] |
| Switching regulator | Spurs at the switching rate, e.g. every 50 kHz | [PNM-HFC §7.2.1.4] |
| Power line gap | Wideband bursts, up to UHF near source | [PNM-HFC §7.1.4.6] |
| CMD | 150 kHz–30 MHz; hump near 21 or 23 MHz, older modem models | [PNM-HFC §7.2.1.8, §8.8.5] |
| HPNA, ballasts | Wideband, from miswired devices; conducted hash | [PNM-HFC §7.2.1.3, §7.2.1.7] |
| OBI | RFoG only; simultaneous transmitters | [PNM-HFC §7.2.1.9] |
| Defective CM, home amp | Burst spurious; oscillation, phase noise | [PNM-HFC Table 24 row 4d] |

- **Localization differs from downstream.** Per-modem data generally does not help because of the funnel [PNM-HFC
  §8.8.2]. Standard: at each junction disconnect or attenuate a leg while watching node spectrum, then tap to tap; it
  disturbs service and can temporarily mask the fault [PNM-HFC §8.8.2]. Automation: logical tree, breadth-first from the
  node, attenuate at each leg's ingress or wink switch while watching UTSC and channel metrics, to the last leg
  showing it; then a cool-down, since it may merely have gone quiet [PNM-HFC §8.8.6, §8.8.10].
- *Switches:* On, Off, or attenuated, commonly 6 dB [PNM-HFC §8.8.9.1]. Modems short of headroom may go partial; check
  first [PNM-HFC §8.8.8].
- *UDA:* some D3.1 modems capture the upstream band and hear nearby ingress. Compare max-hold of about 15 quiet
  captures to UTSC, close in time. One match: that tap, drop and home. Several: their common equipment [PNM-HFC
  §8.8.7].
- *Upstream RxMER:* same time and frequency across modems points to shared plant; outlier subcarriers flag
  intermittents [PNM-HFC §6.3.4.1, §6.3.4.2].

## 10. Common-path distortion and PIM

- **Cause.** Corroded contacts form a thin oxide acting as a diode, mixing downstream signals in plant common to both
  directions: seizure screws, tap plates, terminators [PNM-HFC §8.16.1.1; SCTE 183 §4.3]. CPD is a subset of PIM,
  which also comes from ferrites, dissimilar metals and thin plating [PNM-HFC §7.2.1.6, §5.2.6.7].
- **Signature.** Analog carriers: second order at 6, 12, 18 MHz; third at 9, 15, 21; ±1.25 MHz sidebands; 8 MHz
  spacing in Europe [PNM-HFC §8.16.1.1]. All digital: raised upstream floor, modeled by convolving the downstream with
  itself (second order) and again (third); each haystack gives a triangle twice as wide, peaking every 6 MHz to 42 MHz
  [PNM-HFC §8.16.1.1]. Fragile to wind, vibration and temperature; one source rose 26 dB in two hours [PNM-HFC
  §7.1.4.7, §8.16.1.2] [field]. Repairs are often short-lived [SCTE 183 §4.3].
- **Detection and localization.** PNM has no automatic detector; use spectrum analysis [PNM-HFC §8.16.1.2]. Active
  test: 859.25 and 818.75 MHz carriers gave a 40.5 MHz beat [field]. Correlation radar: distance is half the delay.
  The source usually also mismatches, so an echo may help locate it [PNM-HFC §8.16.1.2]. UDA has not identified CPD
  [PNM-HFC §8.8.7]. Fixing reflections reduces its incidence [PNM-HFC §8.16.1.2] [field].

## 11. Amplifier, node, laser and optical

- **Misalignment.** Low input padding lowers CNR; high output degrades distortion; wrong equalizers change tilt
  [PNM-HFC §7.2.3]. Amps far out of range show an uneven floor: `dn = hfnf − lfnf`, high floor (above the modulated
  band) minus low floor (diplex region), tested statistically; thresholds still unstudied [PNM-HFC §8.15]. A gap
  inside an OFDM channel compared to another gap is an NPR-like check [PNM-HFC §5.6.5].
- **Compression and clipping.** Overdrive, misalignment or defects exceed compression; added channels raise drive
  [PNM-HFC §7.2.1.5]. Laser causes: clipping, setup, added OFDMA, FP lasers, failing lasers, high-level ingress, CPD
  [PNM-HFC §7.2.5, §7.2.5.1]. Signatures:
  - *Histogram:* upstream clipping trims a tail, with a spike near 1.25σ; not yet field-developed [PNM-HFC §6.8.2,
    §6.8.3]. Downstream laser test: RxMER fell 37 to 23 dB while max delta from Gaussian rose about 5 to 10 dB
    [PNM-HFC §5.9.4.4] [field].
  - *Analyzer:* short spikes in zero span at a CSO beat, BER up; averaging hides it [SCTE 183 §4.1].
  - *Constellation:* compression pattern in corner zoom [PNM-HFC §5.7.4]. Distortion otherwise acts as additive noise
    [PNM-HFC §6.2.1].
- **Levels.** Low amp input: one SC-QAM had about 15 dB CNR [PNM-HFC Table 24 row 4d] [field]. CM input too high can
  clip; too low fails to demodulate [PNM-HFC §10.2.7]. Low upstream receive comes from net attenuation: impairments,
  wrong parts, water and ice, failing transmitter, node optoelectronics, return receiver, amp setup [PNM-HFC §7.2.4].
- **Optical and hard failure.** Dirty connectors, bend radius, abrasion, frozen bundles, leaking closures get causes
  but no RF signature [PNM-HFC §7.2.5.2]. Cut cable, power failure or a failed active make modems vanish [PNM-HFC
  §VII.3 Table 44].

## 12. Connector and drop faults

One defect commonly shows echo, ingress and leakage together, plus CPD when corroded [PNM-HFC §9.7, §6.2.1]. Rapid
reflection swings may mark a loose connector or imminent failure [PNM-HFC §8.11.4].

- **Scope rules** [PNM-HFC §8.8.9.3, §8.12.1]: one modem in a home but not another is in-home; one subscriber but not
  another on the tap is drop or in-home; several on a leg is cable or active. Most unique echo tunnels are under 50 m
  (164 ft), affecting only taps 9 and 10 [PNM-HFC §8.11.6]. Opposite sides of a street may be different legs [PNM-HFC
  §8.8.9.3]. FMECA annotates the drop with 70% of problems in home or yard; drop cuts and hardline cuts and metal
  fatigue rate high, staples very low [PNM-HFC Tables 41, 42]. Drop amp modes include defective PSU, hum,
  self-oscillation, wrong diplex [PNM-HFC Table 43].

## 13. Power-related symptoms

These sources say little about plant power. **Hum:** amplitude modulation at 60 and 120 Hz, mainly from amps with
defective module supplies and overloaded system supplies; switching regulators do it at higher rates. Limit 3% p-p
(FCC analog; SCTE 40 digital worst case) [SCTE 183 §4.4, §4.5]. `Hum% = 100·|1 − 10^(Hum_dB/20)|`; 0.5 dB is 5.9%
[SCTE 183 §4.5, magnitude ours]. QAM receivers track moderate hum via AGC and main tap; above that, a post-FEC cliff
[SCTE 183 §4.5]. **Other:** AGC hunting and wind intermittents [SCTE 183 §4.4]; coil group delay at 5–10 MHz [PNM-HFC
§I.4]; modems lost to commercial power motivate Formula 2 (§16). T3/T4 are late indicators [PNM-HFC §10.1.10]. D4.0
amps with an onboard CM may report voltages [PNM-HFC §VIII.2.1.5]. Powering design is in plant-and-powering.md.

## 14. Telling look-alikes apart

| Seen | Could be | Discriminator | Source |
|---|---|---|---|
| Ripple | Echo vs water | Time-domain peak vs spread; weather tracking | [PNM-HFC §5.2.6.12.2, §7.1.7.3] |
| HF loss | Water, low-rated part, one-sided suckout, wide ripple | Pockets; pass-band edge; other notch side; variation in time | [PNM-HFC §7.1.6] |
| Band absent | Filter, vacant channel, stuck diplexer | Filter skirt; neighbors on tap; lineup | [PNM-HFC §IV.3.3, §7.1.2.3] |
| Narrow FBC dip | Suckout, ripple trough, trap | Periodicity; Q; one drop vs shared | [PNM-HFC §IV.3] |
| Narrow RxMER dip | Ingress vs suckout | Suckout also dips level in FBC and channel estimate; ingress overlaps a licensed band | [PNM-HFC §8.7.3, Table 24] [inferred] |
| Upward bump | Peaking, misalignment, adjacency | Convex skirts vs band steps | [PNM-HFC §5.2.6.8, §5.2.6.9] |
| Level step | Adjacency vs plant | Same on all modems of a transmitter; in-channel flatness change means plant | [PNM-HFC §5.2.6.3] |
| Pre-main taps | Band-edge group delay vs short echo | PPESR sign; channel frequency; cascade depth | [PNM-HFC §6.2.6.3.12] |
| Raised US floor | Ingress, CPD, amp noise | 6 MHz structure; burstiness | [PNM-HFC §8.16.1] |
| FEC bad, RxMER fine | Impulse noise | Burst capture | [PNM-HFC §7.2.1.1] |

## 15. Grouping and localization

- **Principle.** Find the path common to modems sharing a signature, then exclude any path shared with clean modems;
  the fault is between the last device showing it and the first not [PNM-HFC §8.8.1.1, §8.9, §8.10]. One modem bounds
  it only to its whole path [PNM-HFC §8.10]. Works with pre-eq, FBC and RxMER [PNM-HFC §8.8.9.3, §8.9]. Example:
  ripple on all modems under amp A and no other output of upstream amp B puts it on the cable or connectors between
  them [PNM-HFC §8.9.1.1] [field]. Delay matching a known span narrows further; sparse modems widen the area [PNM-HFC
  §8.10, §8.10.1].
- **Signature.** The pair level and delay. Ripple magnitude alone can collide; collisions are rare within a node,
  except sub-T echoes, which need topology [PNM-HFC §8.10.2]. A modem's echo can be longer by an added transit (2.8T
  vs neighbors) [PNM-HFC §8.10.3]. Interpolate on three taps in dB: `a = (y0 − 2·y1 + y2)/2`, `xm = (y0 − y2)/(4a)`,
  `ym = −(y0 − y2)²/(16a)`. Oracle: taps 9, 10, 11 at 35, 40, 29 dB give tap 9.8125, 40.28 dB [PNM-HFC §8.10.4].
- **Matching** [PNM-HFC §8.11.6.1] [guideline]:
  1. List a node's modems; drop invalid coefficients. Compare only on one logical channel.
  2. Optionally drop unimpaired modems, e.g. MTR above 25 dB.
  3. Compute a match value per pair and threshold it, e.g. 18 dB (procedure) or 25 dB (§8.11.6.2). Too low a threshold
     matches by factory design.
  4. Force the matrix symmetric; form groups; use GIS to reject distant members.
  5. Line tech tickets for groups; service tech tickets for single bad homes.
- *Frequency division:* zero-pad 24 taps to 32, FFT, divide; flat quotient means match. IFFT and take MTR: 14.09 and
  15.3 dB modems matched at 27.3 dB [PNM-HFC §8.11.6.2] [field]. In-home echoes can mask a shared hardline echo: zero
  the close-in taps, for long hardline echoes only [PNM-HFC §8.11.6.2].
- **Topology discovery.** Add ingress-switch attenuation and see which modems raise transmit power; identifies amp
  legs, not taps [PNM-HFC §8.8.9.3].

## 16. Severity and prioritization

- **Per modem** [PNM-HFC §8.11] [guideline]: static, trending, intermittent.
- *Static:* −25 and −18 dBc as in §2. Fig 276 labels those negative values "MTR", while Tables 26–28 use positive MTR.
- *Trending:* 3 dB merits attention. With daily change D and last level MR, red if `MR + 3·D > −18`; `days_to_repair =
  (−18 − MR)/D`. Compare the same hour across days [PNM-HFC §8.11.3].
- *Intermittent:* red if `avg 4 h swing / (−18 − avg level) > 0.25`; under 0.1 is safe [PNM-HFC §8.11.4].

| MTR (dB) | Good | Marginal | Bad | Use |
|---|---|---|---|---|
| Table 26 | ≥ 22 | 11–22 | < 11 | New to PNM |
| Table 27 | ≥ 25 | 18–25 | < 18 | Under 4 cascade, established team |
| Table 28 | ≥ 32 | 25–32 | < 25 | Modem below expected modulation |

- **Plant health.** `10·(1 − (red + 0.5·yellow)/(Total − NonResponding))`, or `Total + NonResponding` when green
  modems were lost to outside causes. Healthy is about 7; use per upstream, not per node. Track and shrink the number
  of groups [PNM-HFC §8.11].
- **Deciding work.** Criticality combines customers affected, duration and severity [PNM-HFC §VII.4]. Consider
  location, impact, whether enough is known to act, cost, resources, likely success; find anomalies, estimate customer
  and profile impact, then select. A repair can even hurt profiles [PNM-HFC §10.3]. Combine spectrum with profile, MER
  and FEC; flag no-impact ingress for monitoring [PNM-HFC §8.8.7]. Act while pre-eq still compensates [PNM-HFC §8.11].

## Implications for hfc-twin

Everything here is [inferred]. Under invariant 5 each row is a physical parameter set and signatures come out of the
sim. SID thresholds and rules above are diagnoser features or baselines, never sim logic.

| Impairment | Class (physics §8) | Status |
|---|---|---|
| Echo, compound ripple | Connector / reflection (`location`, `return_loss_db`) | Partial: items 1–2 |
| Water | Water ingress (`affected_len_ft`, `k_w`, `rain_coupling`) | Partial: item 3 |
| Suckout | Suckout / notch (`f0_mhz`, `depth_db`, `width_mhz`) | Partial: item 4. Use 18–20 dB, Q 2.7–14 |
| Tilt, amp flat gain, amp failure | Amp drift / failure (`gain_delta_db`, `tilt_delta_db`, `Failed`) | OK |
| Flat loss on one drop | Bad drop (`extra_loss_db`, optional `return_loss_db`) | OK |
| Downstream ingress, LTE | Downstream ingress (`entry_location`, `f_mhz`, `bw_mhz`, `power_dbmv`, `schedule`) | Levels OK; multi-band via compound; item 5 on FEC |
| Direct pickup | Downstream ingress, entry at modem | Approximate; overload not modeled |
| Upstream narrowband, wideband | Upstream ingress (same parameters) | OK |
| Upstream impulse | Upstream ingress | Partial: `schedule` has no burst structure, item 7 |
| Compression | Amp `gain_delta_db` plus §4.4 | Partial: §4.4 is linear, no compression knee |
| Dropout, partial service | Power sag (`psu_id`, `v_delta`, `schedule`) | Plausible; these sources do not confirm it |
| Peaking, filter, roll-off part, pullout, adjacency, CPD, laser as noise | Task 16's faults | Done, see `rxmer-impairments.md` §4. A laser's clipping signature in the histogram isn't emitted |
| Hum | Power sag | Not producible per interval; low priority |

1. *ripple_penalty (physics §4.5, §5.2).* We add −E dB as noise (33.9 dB in the oracle). PNM-HFC §5.3.4.2 has no such
   term for OFDM: RxMER is unmodified for pre-echo noise and |H|²-shaped for post-echo noise. Split noise into pre-
   and post-echo; keep a residual only for echoes past the CP or equalizer span. SC-QAM may keep it, since NMTER is a
   stated RxMER estimate for linear distortion [PNM-HFC §6.2.6.3].
2. *Reach (physics §8, "every modem whose path crosses the point").* Tap-isolation echoes come from reflectors off the
   path, so model tap isolation as a sneak path. Also §5.1 assumes one reflector near the modem, while plant cavities
   have both ends away from it; the fault needs a partner mismatch from the graph, and a sign if open vs short is to
   be observable.
3. *Water (physics §8).* Smooth `√f` loss gives tilt and roll-off, not the non-periodic waves that identify water.
   Draw per-seed segment variations of Z0, VF and loss over `affected_len_ft` and compute the response. Add freezing
   from `temp_c` and drain lag. Allow water on a drop, which `span_id` may not cover. DESIGN's "RxMER tilt" undersells
   this: FBC waves, low RxMER, clean upstream.
4. *Suckout.* DESIGN pairs "suckout / cracked shield", but sources attribute suckouts to grounding, craft and
   manufacture, and cracks to reflection plus ingress. Magnitude-only loss gives no group delay; use a minimum-phase
   response so coefficients stress as Table 24 says. `width_mhz` is where dB depth falls to 1/e, not the −3 dB
   bandwidth behind Q.
5. *FEC under notches (physics §4.6).* `m = RxMER_p2 − threshold` makes an 8 MHz band in 192 MHz (over 2% of
   subcarriers, so p2 lies inside) fail once notch MER is under 35 dB for 4096-QAM; whale plots show uncorrectables
   near 8–9 dB. §4.5's 33.04 dB oracle band would be wrongly fatal. Use a code-spread model, e.g. mutual information
   averaged over a codeword. Consider the 13 dB CM vs instrument offset.
6. *New parameter sets:* `ResonantPeak` on an amp (`f0_mhz`, `gain_db`, `q`, wander, with AGC response);
   frequency-selective loss at any location with a shape (band-stop, high-pass, low-pass), covering traps, pullout and
   low-rated parts, carrying stop-band return loss; `HeadendLevelOffset` per channel block; CPD (`location`, second
   and third order conversion, drift), its floor computed by convolving the actual downstream at that point; node or
   laser (RF drive, clipping point), so ingress and added OFDMA can clip.
7. *Upstream (physics §7.3, §9).* §7.3 says localization needs per-modem transmit-power deltas. Sources say modem data
   generally does not localize upstream ingress; use attenuation switching with UTSC or UDA, while power deltas serve
   topology discovery. The sim needs an ingress-switch action model and a node spectrum, added because the sim can
   produce them (invariant 2). Bursts need duration and rate.
8. *Pre-eq band edge (physics §5.3).* Without diplexer group delay, healthy modems show clean pre-main taps; sources
   show −15.9 dB NMTER at N+5 at band edge with no reflection. A diagnoser trained on our data would mis-flag real
   plant. Add diplexer and coil group delay. Compute taps as the channel inverse, so multi-transit and
   single-reflection geometries give their different patterns.
9. *FBC floor and joint faults.* §8.15 needs vacant-bin floor from the cascade, not a constant. One damaged connector
   can give echo, ingress, CPD and leakage together, so training priors should sample these jointly.

## Gaps

- **Unpublished:** SID suckout defaults, slope units and the scale behind "amplitude threshold 18.0" and "analog dB
  threshold −20" [PNM-HFC §5.2.6.6.2]; SID code is NDA.
- **Not read:** SCTE 280, SCTE 294, [Fox et al.], [O'Dell 2023], [Hranac 2023].
- **No distributions:** sources give examples, not severity ranges, and no prevalence data. No typical `k_w`, ingress
  power at entry, CPD level, or connector return loss beyond the 7 dB water-damaged tap.
- **Undescribed:** hardline water; a telemetry test separating direct pickup from drop ingress; pullout severity.
- **Power:** no source here describes a modem-visible signature of a sagging plant supply, so DESIGN's "intermittent
  partial service across one powering zone" and physics §6.3 are unverified here. Time-of-day gets passing mention
  only [PNM-HFC §6.2.6.4.2.1].
- **Optical:** causes only, no quantified signature.
- **FMECA Tables 41–43** are low-resolution images: "9" entries undefined, probabilities mostly blank, alignment
  uncertain. Only row labels and stated probabilities are used.
- **Whale readings:** small plots, ±1–2 dB, one LTE-like shape at 375 MHz, few modems.
- **Source inconsistencies:** VHF lower bound 108 vs 90 MHz (§5.2.6.6, §7.1.4.4); CMD 21 vs 23 MHz; match threshold 18
  vs 25 dB; §8.11 cites Figure 275 for Figure 276; §8.11.6.1 says MTR below 25 dB means no serious echo, where above
  is meant; CPD third order at 9, 15, 21 MHz (PNM-HFC) vs ±1.25 MHz around second order (SCTE 183 §4.3); SCTE 183's
  Hum% as printed is negative.
- **Other revisions:** PNM-3.1 agrees where it overlaps but lacks §7.1–7.2, Table 24 and §8.8; PNMP has the same
  severity numbers.
