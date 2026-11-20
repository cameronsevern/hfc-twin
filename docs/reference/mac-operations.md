# MAC operations as plant symptoms

Scope: DOCSIS 3.1 modem and CMTS behavior (initialization, ranging, timeouts, partial service, CM-STATUS, profiles, upstream power) that appears as an observable symptom of a plant problem.

Power is in dBmV/1.6 MHz (source symbol P1.6) unless stated. Source fields in 1/4 dB are converted. Times stay in the source's units.

## Key points

- **The CMTS commands upstream power.** After registration the modem follows the command even beyond the dynamic range window and reports the violation [normative] [MULPI §6.4.5.1.1, §8.3.3.1].
- **No rule in MULPI or PHY says exceeding a transmit limit causes partial service.** Partial service means a channel is unusable: not acquired, T4 expiry, T3 retries exceeded, or lost lock [normative] [MULPI §8.4, §10.3]. Only guidance links headroom exhaustion to partial service, and only as a possibility [guideline] [PNM-HFC §8.8.8, §8.14.3].
- **The per-channel limit depends on the channel set.** P1.6hi = Pmax - 10log10(Neq), not a fixed 53.2 [normative] [PHY §7.4.12.2].
- **T3 and T4 point in different directions.** T3: the modem transmitted and heard no response. T4: no opportunity reached it. This reading is [inferred] from [MULPI §10.3 Figure 169].
- **A modem never moves itself between profiles.** It reports; the CMTS reassigns [normative] [MULPI §10.6.3, §11.8].

## Timing constants

From [normative] [MULPI Annex B Table 107]. "max" and "min" are that table's limit columns; a simulator can use a max as the worst case.

| Name | Meaning | Value |
|---|---|---|
| T1 | Wait for UCD | max 5 x 2 s UCD interval = 10 s |
| T2 | Wait for broadcast ranging opportunity | max 5 x 2 s ranging interval = 10 s |
| T3 | Wait for RNG-RSP | default 200 ms |
| T4 | Wait for unicast opportunity | default 30 s; multiplier 1-10 gives 30-300 s; any pending-till-complete is added |
| T-OFSM | OFDMA wait for first station maintenance (fine ranging) | max 10 s |
| Contention / invited ranging retries | Broadcast / unicast | 16 / 16 |
| CM ranging response time | Minimum before next unicast opportunity | 1 ms |
| T6, registration retries | Wait for REG-RSP or REG-ACK | default 3 s, 3 retries |
| T9 | CMTS: RNG-RSP success to REG-REQ | 15 min |
| Initializing channel timeout | CM ranging on TCC channels | default 60 s; TLV 52 sets 1-65535 s [MULPI §C.1.5.6] |
| T17 | Ranging hold-off | default 300 s |
| Lost sync interval | No SYNC or TIMESTAMP (SYNC period max 200 ms) | max 600 ms |
| MDD interval, lost MDD timeout | | max 2 s, min 6 s |
| OCD/DPD on PLC | Interval; CM timeout | default 200 ms, max 250 ms; 1.25 s |
| DPD on Profile A | Interval; CM timeout | default 500 ms, max 600 ms; 3 s |
| Profile advance time | New DPD published before use | min 500 ms |
| OPT-RSP timer, OPT test timer | CMTS | max 800 ms; max OPT Max Duration + 3 s |
| OPT-ACK timer, retries | CM | max 800 ms, 3 |

Also:

- The modem applies RNG-RSP within 50 ms unless a Global Reconfiguration Time is needed [normative] [MULPI §6.4.6].
- The CMTS should not start a probe or unicast opportunity within 50 ms (plus interleaver delay) of a RNG-RSP to that modem [normative] [MULPI §10.3].
- On OFDMA the CMTS must allow CM ranging response time plus T3 before the next unicast or probe opportunity [normative] [MULPI §10.2.3.4]: 201 ms at defaults [inferred].
- The periodic ranging subinterval sets the event rate and is CMTS-dependent. It must be short enough that one missed MAP doesn't cause T4 [normative] [MULPI §10.3]. See Gaps.

## Initialization and registration

Flow: scan for downstream, sync, resolve service group and range, authenticate (EAE, if enabled), establish IP, register, Baseline Privacy (if EAE off and privacy on), operational. IP failure returns to scanning [normative] [MULPI §10.2 Figure 133].

**`CmRegState`** [normative] [CM-OSSI §F.2.2]: other(1), notReady(2), notSynchronized(3), phySynchronized(4), dsTopologyResolutionInProgress(21), usParametersAcquired(5), rangingInProgress(22), rangingComplete(6), eaeInProgress(14), dhcpv4InProgress(15), dhcpv6InProgress(16), dhcpV4Complete(7), dhcpV6Complete(17), todEstablished(8), securityEstablished(9), configFileDownloadComplete(10), registrationInProgress(18), registrationComplete(11), accessDenied(13), operational(12), bpiInit(19), forwardingDisabled(20), rfMuteAll(23).

**Downstream acquisition** (reinit starts here):

- Tries the stored downstream first, then scans continuously, preferring OFDM over SC-QAM as primary [normative] [MULPI §10.2.1].
- OFDM is valid as primary only after timestamp, OCD, DPD for Profile A and NCP, and a Profile A decode; otherwise scanning continues [normative] [MULPI §10.2.1.1]. A fault that breaks Profile A therefore also blocks re-registration on that channel [inferred].
- No valid UCD within T1: keep scanning [normative] [MULPI §10.6 Table 98].

**Registration outcomes** [normative] [MULPI §10.2.6.1]:

| Condition | Result |
|---|---|
| T6 expires with retries used up | Reinit T6_EXPIRED |
| Primary downstream not acquired | Reinit FAILED_PRIM_DS |
| Non-primary downstream not acquired | Downstream partial service |
| Ranging fails on all TCS channels | Reinit TCS_FAILED_ON_ALL_US |
| Ranging succeeds on some channels | Upstream partial service |
| No ranged channel on the primary upstream flow | No REG-ACK; reinit NO_PRIM_SF_USCHAN |

REG-ACK partial-service code is 30 [normative] [MULPI §C.4]. Failed downstreams are listed in TLV 49.6, failed upstreams in a TCC error per channel [normative] [MULPI §6.4.9, §C.1.5.3.6].

D3.1 operation on the downstream requires registering in Multiple Transmit Channel (MTC) mode. Upstream power problems that block choosing a bonding group can leave the modem rejected and cycling; a single-channel group should still be allowed [guideline] [PNM-HFC §6.1.2.3].

## Ranging

| Stage | Opportunity | Message | Purpose |
|---|---|---|---|
| Broadcast initial | Contention, IUC3 | SC-QAM B-INIT-RNG-REQ; OFDMA O-INIT-RNG-REQ | Find level and timing |
| Fine (OFDMA) | Unicast, within T-OFSM | Version 5 B-INIT-RNG-REQ | Refine; reports power |
| Station maintenance | Unicast | RNG-REQ | Periodic power, timing, frequency check |
| Probe (OFDMA) | P-IE in a probe MAP | PHY signal, no data | Pre-eq on every non-excluded subcarrier; optional CMTS RxMER |

Sources: [normative] [MULPI §5.2.3, §10.2.3, §10.2.3.3.2, §10.5.1.2]. Fine ranging is equivalent to init(r2) or station maintenance [guideline] [PNM-HFC §6.1.2.3]. Initial ranging defaults to one-third up from the bottom band edge; a higher frequency is the worst case for attenuation but can affect TCS selection and the DRW decision [guideline] [PNM-HFC §6.1.2.3].

**Starting power before registration** [normative] [MULPI §10.2.3.4.1]: stored value if valid; else, on OFDMA, UCD TLV 22 (start level and per-retry step, 1/4 dB units [MULPI §6.4.3]); else the minimum. The modem must cover its dynamic range within 16 retries with no untried gap over 12 dB. Before fine ranging, per-subcarrier power can be up to 9 dB below P1.6low [normative] [PHY §7.4.12.2]. Before registration, power can exceed default Pmax if the MDD's Extended Upstream Transmit Power TLV is On [normative] [MULPI §6.4.28.1.15].

**Initial ranging failure** [normative] [MULPI §10.2.3.3, §10.2.3.3.1, §10.2.3.3.2, §10.2.3.3.3]:

- T2, T-OFSM or exhausted T3 retries fail that upstream; the modem tries the next candidate.
- All previously successful upstreams fail in ambiguity resolution: reinit ALL_US_FAILED.
- RNG-RSP abort: continue downstream scanning.
- After a reinit, UCD TLVs 18 and 19 can hold off ranging by Ranging Class ID for up to T17. Lost SYNC during the wait gives T17_LOST_SYNC.

**RNG-RSP status**: 1 continue, 2 abort, 3 success [normative] [MULPI §6.4.6.1]. Continue suspends all but RNG-REQ and probes on that channel until success; the CMTS should use it only if it can't guarantee reception of all available burst types, then schedule the next opportunity quickly [normative] [MULPI §10.3]. "Good enough" means within CMTS tolerance for power, timing, frequency and equalization, with no number given [normative] [MULPI §10.3 Figure 168].

## T3 and T4 during station maintenance

The Periodic Maintenance Timeout Indicator (PMTI), in MDD, sets which OFDMA opportunities drive T4 and the retry counter: 0 unicast ranging, 1 probe, 2 either [normative] [MULPI §6.4.28.1.17, §10.3].

Per upstream channel [normative] [MULPI §10.3 Figure 169]:

1. **Opportunity.** Ignored if the last RNG-RSP adjustment isn't applied or T3 is active. If the PMTI counts this type: restart T4, send, increment retries, start T3. If not: send, start T3.
2. **T3 expires.** If that transmission counted and retries are exceeded, go to 5. Otherwise wait for the next opportunity. The figure shows no power step; the implementation may adjust, since non-receipt handling is implementation-dependent [normative] [MULPI §10.2.3.4.1].
3. **T4 expires.** Go to 5.
4. **RNG-RSP.** Clear T3; reset retries if the transmission counted. Abort goes to 5. Success clears suspension and applies adjustments. Otherwise suspend all but ranging.
5. **Channel unusable.** A usable channel remains on the Primary Upstream Service Flow: partial service, suspend all but ranging, start CM-STATUS. None remains: reinitialize MAC, even if another TCS channel works [normative] [MULPI §10.3].

Consequences for a simulator [inferred]:

- One RNG-RSP resets the count, so exhaustion needs consecutive misses: 17 in a row from a fresh counter, probability p^17 for independent loss p.
- T4 restarts on transmission, not on response, so an upstream-only fault doesn't expire T4 while MAPs still arrive.
- At most one T3 expiry per 200 ms per channel.
- Time to exhaustion is about 17 x opportunity spacing, with a floor near 3.4 s on OFDMA.

**The two sides are coupled.** With no response and retries exceeded, or after an abort, the CMTS checks for a stable channel on the primary flow. If one exists it removes the channel from the TCS or tries to recover it; if not, it deregisters the modem [normative] [MULPI §10.3 Figure 168]. Recovery options include re-ranging, DBC-REQ, or CM-CTRL-REQ reinit (reason CM_CTRL_INIT) [normative] [MULPI §6.4.35, §10.3]. A CMTS that stops scheduling a deregistered modem gives T4 after the T3s [inferred].

**Non-MTC modems** reinit with T4_EXPIRED [normative] [MULPI §10.3]. Given the MTC requirement, a D3.1 modem on a D3.1 CMTS should normally report NO_PRIM_SF_USCHAN instead [inferred].

**Counters are per upstream channel** [normative] [CM-OSSI §F.2.3.2, §F.2.3.3]: `T3Timeouts`, `T4Timeouts`, `RangingAborteds`, `T3Exceededs`, `IsMuted`, `RangingStatus` (other 1, aborted 2, retriesExceeded 3, success 4, continue 5). MIB tables: `docsIf3CmStatusUs*`, `docsIf31CmStatusOfdmaUs*` [normative] [CM-OSSI Annex A]. Per MAC domain: `Resets`, `LostSyncs`, `InvalidMaps`, `InvalidRangingRsps`, `T1Timeouts`, `T2Timeouts` [normative] [CM-OSSI §F.2.3.1].

**Event IDs** [normative] [CM-OSSI Annex C]: R05.0 unicast no response T3; R06.0 unicast retries exhausted; R07.0 abort received; R08.0 16 consecutive T3 on a channel; R10.0 commanded power above Pmax; R11.0 commanded power above DRW top; T07.0 RCS partial service; U102.0 TCS partial service.

**Operator use**: T3, T4 and uptime are reactive measures, and customers are likely to notice what they show. Power, uncorrectables, low RxMER, spectrum and pre-eq energy are the proactive set [guideline] [PNM-HFC §1.3.2.1, §1.3.2.2, §10.1.10].

## Downstream lost lock

**Triggers** [normative] [MULPI §10.6.1]. SC-QAM: loss of symbol timing, FEC framing or MPEG sync. OFDM: (1) pilot detection error, (2) preamble sync loss, (3) PLC unreliable-codeword ratio over limit, (4) NCP CRC rate over limit, (5) Profile A FEC over limit. Limits in 3-5 are vendor-specific.

**Response** [normative] [MULPI §8.4, §10.6.1 Figure 180]:

- Move MAP reception to another channel if this one carries MAPs.
- **Primary with a backup:** switch, wait for ranging success on all upstreams, send CM-STATUS. Cases 3-5 end in partial channel; cases 1-2 in partial service. Original primary isn't reclaimed automatically [MULPI §10.6.1.2].
- **Primary, no usable backup:** cases 3-5 keep using the channel and monitor. Cases 1-2: upstream stops and the modem reacquires until T4 expiry or 17 T3 expirations on all upstreams force reinit, reason NO_PRIM_SF_USCHAN [MULPI §10.6.1.2].
- **Non-primary:** CM-STATUS, then partial channel (3-5) or partial service (1-2).

**Short interruptions** of the primary [normative] [MULPI §10.6.1.1]: 5 ms or less, must resume normal operation within 2 s. Over 5 ms but under the lost sync interval, may continue if recovered within 2 s. Otherwise reacquire timing, be ready to range within 2 s, and wait for RNG-RSP success per channel.

## Partial service and partial channel

Three conditions a single boolean merges [normative] [MULPI §8.4, §10.6.3]:

| State | Scope | Entry | Signaled by | Exit |
|---|---|---|---|---|
| Upstream partial service | TCS channel unusable | Not acquired; T4; T3 retries; abort | REG-ACK, DBC-RSP, CM-STATUS | RNG-RSP success on all, or DBC removes them |
| Downstream partial service | RCS channel unusable | Not acquired; lost; all profiles lost on OFDM | REG-ACK, DBC-RSP, CM-STATUS | All reacquired, or DBC removes them |
| Partial channel (OFDM) | Some profiles, or non-primary PLC | A profile fails while at least one works and pilots and preambles are fine | CM-STATUS | All recover, or all lost (to partial service) |

A DPD missing at registration for a non-A, non-NCP profile also means partial channel [normative] [MULPI §10.6 Table 98].

While in these states [normative] [MULPI §8.3.3.1.1, §8.3.3.1.2, §8.4, §10.6.3]:

- **Upstream:** no request, data or broadcast initial maintenance on the channel; must answer unicast ranging. The CMTS gives only ranging opportunities.
- **Downstream:** keep trying a non-primary channel; the CMTS stops unicast to the modem there.
- **Partial channel:** keep trying the profile, stopping only if DBC-REQ removes it. The CMTS may stop using it or shift flows.
- **CMTS** must try to resolve both states.
- **P1.6hi is retained** from the full TCS and changes only if DBC changes the TCS. Losing a channel doesn't itself raise the per-channel limit.

**CMTS reporting** [normative] [CCAP-OSSI §6.4.33-§6.4.36, §7.2.2.2.1, §7.2.2.2.3]: `PartialSvcState` (other, none, DS only, US only, both). `PartialChanState` bits: fecErrorsDsProfile, fecErrorsNcpProfile, fecErrorsPlc, lostFecLockUsProfile. `PartialChanReasonType` uses CM-STATUS codes 16, 18, 20, 21. `PartialSvcReasonType`: secondaryChanMddTimeout, lostFecLock, iuc13CwErrors, other. `iuc13CwErrors` means IUC 13 codeword errors over a vendor limit, so a CMTS can declare upstream partial service from upstream FEC with no modem timeout.

## CM-STATUS

**Generation** [normative] [MULPI §6.4.28.1.1, §6.4.28.1.7, §6.4.28.1.11, §10.6.4.1]: per-event-type state machine, with on/off state per channel, DSID or profile; an event occurs on off-to-on. None before operational or during a primary MDD timeout. Silent unless MDD has Event Control TLV 11 for that type and the enable bit is set. TLV 11.2 Maximum Holdoff: 20 ms units, 1-65535 (20 ms to about 21.8 min). TLV 11.3 Maximum Reports: 0 unlimited, else 1-255.

**Rate model** [normative] [MULPI §10.6.4.1.1]: first report after uniform random 0 to Maximum Holdoff; nothing is sent if no event is still on. Repeats after exactly Maximum Holdoff until reports run out, a matching CM-STATUS-ACK arrives, the event clears, or reporting is disabled. A new off-to-on while sending resets the report count and starts a new 16-bit transaction ID (skips 0). So a burst shorter than its holdoff can leave no report [inferred].

**Plant-relevant codes** [normative] [MULPI §10.6.4.1.2 Table 99]:

| Code | Event | On when | Off when (besides channel removal) |
|---|---|---|---|
| 1 / 4 | Secondary MDD timeout / recovery | Lost MDD timer on active non-primary / MDD received | Reverse |
| 2 / 5 | QAM/FEC lock failure / recovery | Lost lock on active downstream / regained | Reverse |
| 6 | T4 timeout | T4 expires on the channel | Maintenance opportunity received |
| 7 | T3 retries exceeded | Retries exceeded | RNG-RSP received |
| 8 | Ranging success after T3 exceeded | Success after code 7 reported | Retries exceeded again |
| 9 / 10 | On battery / back on AC | AC lost or restored over 5 s (battery modems) | Reverse |
| 16 / 24 | DS OFDM profile failure / recovery | Profile FEC over limit / recovered | Reverse, or profile removed |
| 17 | Primary downstream change | Primary lost, backup acquired | N/A |
| 18 | DPD mismatch | DPD count LSB differs from NCP odd/even bit | DPD or NCP reacquired |
| 20 / 22 | NCP failure / recovery | NCP FEC over limit / recovered | Reverse |
| 21 / 23 | PLC failure / recovery | PLC FEC over limit / recovered | Reverse |

Omitted: 3, 11, 25, 26, 27 (not plant-driven); 19 is deprecated. MULPI defines no numeric FEC limit for these events [inferred, from not finding one].

## Downstream profiles and OPT

- **Profiles:** ID 0 is Profile A, common and used at initialization. IDs 1-3 are B-D. NCP has its own profile. A modem supports at least four profiles plus a transition profile per OFDM channel; a transition profile carries no traffic for that modem [normative] [MULPI §6.4.41, §7.12, §7.12.1].
- **Before registration:** Profile A and upstream IUC 13 only [normative] [MULPI §10.4].
- **Profile A's modulation is the operator's.** It can be lower, such as 64-QAM or 16-QAM, so every modem can communicate in the worst part of the plant. Uncorrectables on it send the modem back to D3.0 channels [guideline] [VIAVI2016]. No source gives what operators run; our fixtures use 256-QAM (`generic`).
- **Assignment:** TLV 49.7.3 in the RCC, at registration or by DBC-REQ, at least one per OFDM channel [normative] [MULPI §C.1.5.3.5.6, §11.5.1.1]. Moving a flow between profiles can need no modem action, depending on packet-order sensitivity [normative] [MULPI §11.5.1.2.4].

**OPT transaction** [normative] [MULPI §6.4.44, §6.4.45, §10.4.1, §10.4.2]:

- OPT-REQ goes to operational modems only, at most four per modem per OFDM channel, never for an already-assigned data profile.
- First OPT-RSP has status Testing; a second has the result; the CMTS answers OPT-ACK.
- Status: 1 testing, 2 already testing, 3 no free resource, 4 max duration expired, 5 aborted, 6 complete, 7 already assigned, 8 DS lock lost.
- A test ends at Nc codewords, Ne uncorrectable, or Max Duration (at most 3 min).
- Results as requested: RxMER per subcarrier (1/4 dB, N at most 7600), threshold comparison, count below (target - margin), SNR margin (signed, 1/4 dB), codeword total, corrected and uncorrectable counts, NCP field and CRC counts.
- Spec example: CER threshold 1e-3, Ne = 10, Nc = 1e4, over 1e3 test codewords/s, Max Duration 10 s [guideline] [MULPI §10.4.1]. SNR margin's algorithm is vendor-specific [guideline] [PNM-HFC §5.12].

**Moving down** [normative] [MULPI §10.6.3, §11.8]: profile errors raise code 16 and partial channel. The CMTS must stop unicast on that profile and resolve it. Fallback should be CMTS-driven and fast, without waiting for a PMA [guideline] [TR-PMA §6.3].

**Moving up**: the CMTS tests with OPT, then assigns via DBC, from achievable RxMER [guideline] [PNM-HFC §10.2.1.1]. Downstream assignments change rarely, so their history is itself a stability signal [guideline] [PNM-HFC §10.2.1.1.2]. The PMA recommends; the CMTS applies [guideline] [TR-PMA §5.2.2].

**DPD change** [normative] [MULPI §11.8]: a modem that can't decode a changed profile raises code 16. A missed DPD is confirmed after one DPD Profile A interval (OCD/DPD PLC interval for Profile A and NCP): code 18 and partial channel, with packets dropped meanwhile.

**Upstream data profiles** [normative] [MULPI §10.5.1, §10.5.1.1-§10.5.1.3]: IUC 13 is the robust profile for all pre-registration grants. Each OFDMA channel gets one or two Assigned OUDP IUCs by TCC, changeable by DBC; evaluation is vendor-specific, using probes and test bursts. Bit loading drops if the signal is buried in noise, with an increase tried once stable; a modem in distress can change often [guideline] [PNM-HFC §10.2.1.1.1].

## Upstream power and the dynamic range window

**Definitions** [normative] [PHY §7.4.12, §7.4.12.2, §7.4.12.3.1]:

- Pmax: maximum total power; default and lowest allowed 65 dBmV.
- Neq = ceil(BW_legacy/1.6) + ceil(BW_OFDMA/1.6), BW in MHz.
- P1.6hi = Pmax - 10log10(Neq), one value for the whole TCS.
- With a D3.1 CMTS and modulated spectrum at or under 24 MHz, commanded P1.6hi must not exceed 53.2 + (Pmax - 65).
- P1.6min = 17 dBmV. With boosted pilots, P1.6low adds 1 dB (50 kHz) or 0.5 dB (25 kHz).
- A 6.4 MHz SC-QAM's actual power is P1.6r_n + 6 dB. Actual power per equivalent channel is within +/- 2 dB of reported, pre-eq off.

| TCS | Neq | P1.6hi at Pmax 65 |
|---|---|---|
| 1 x 6.4 MHz SC-QAM | 4 | 53.2 (cap; formula gives 58.98) |
| 4 x 6.4 MHz SC-QAM | 16 | 52.96 |
| 47 MHz OFDMA modulated | 30 | 50.23 [PNM-HFC §6.1.2.2] |
| 95 MHz OFDMA modulated | 60 | 47.22 [PNM-HFC §6.1.2.1] |
| 4 SC-QAM + 96 MHz OFDMA | 76 | 46.19 [inferred] |
| 192 MHz OFDMA | 120 | 44.21 [inferred] |

**DRW** [normative] [PHY §7.4.12.2; MULPI §8.3.3]: 12 dB wide. Top = P1.6hi - P1.6load_min_set; bottom = top - 12 dB. The CMTS should keep commands in the window. A 12 dB window normally handles up to 10 dB of tilt across the upstream band. Levels should stay near the top because fidelity is specified from there. A new P1.6load_min_set applies at a Global Reconfiguration Time.

**Control messages** [normative] [MULPI §6.4.5.1.1, §6.4.6.1, §6.4.6.8]: Commanded Power TLV 17 holds P1.6load_min_set (17.1) and, per channel, UCID + power in 1/4 dBmV (17.2); a zero is ignored. It is used for version 5 RNG-REQ and B-INIT-RNG-REQ, which report power as 9 bits in 1/4 dB in SSAP and DSAP. O-INIT-RNG-REQ reports none; there the CMTS may use Power Level Adjust TLV 2.

**When a limit is hit:**

| Condition | Behavior | Source |
|---|---|---|
| Command above DRW top | CM applies, sets RNG-REQ SID bit 15, logs; spurious and noise specs stop applying | [normative] [MULPI §6.4.5.1.1, §6.4.6.8] |
| Command over 6 dB below top | CM applies, sets SID bit 14, logs; same relaxation | [normative] [MULPI §6.4.5.1.1, §6.4.6.8] |
| Command above Pmax | CM logs; behavior vendor-specific | [normative] [MULPI §6.4.6.8] |
| Command above P1.6hi | CMTS should not, except under MSM | [normative] [PHY §7.4.12.2, §7.4.12.3] |
| D3.0 CMTS | CM ignores a violating command and flags it; violation at registration is reinit code 19; DBC reject code 210 | [normative] [MULPI §8.3.3.2] |

Initializing channels from REG-RSP-MP or DBC, the modem starts from a Power Offset TLV, a stored value in the DRW, or the DRW bottom, and on T3 tries other levels in a vendor-specific way. It must stay in the DRW until a RNG-RSP [normative] [MULPI §8.3.3.1.1, §8.3.3.1.2]. A modem the CMTS can't hear within the window therefore fails that channel, which is a defined path from low headroom to partial service at registration [inferred].

**Headroom** [guideline] [PNM-HFC §8.8.8, §8.14, §10.2.2]:

- TCP headroom = Pmax - commanded total composite power (linear sum of P1.6r_n + 10log10(BW/1.6)).
- Min Ch headroom = P1.6hi - max(P1.6r_n). Under tilt it usually breaks first.
- Exhaustion may bring FEC errors and lower bit loading, a dropped channel and partial service, a lower profile at lower power, or brief offline periods. No sequence or threshold is given.
- A DRW violation may compromise fidelity, and eventually the most loaded channels are dropped.
- Any net attenuation change drives long-loop ALC, which moves transmit power and can break headroom.
- 6 dB added by a wink switch can cause partial mode; seldom on sub- and mid-split, more likely on high-split.
- Pre-eq variation inside a channel is not bound by the DRW, but uneven loss uses headroom [PNM-HFC §6.1.2.3, §8.14.3].

**TCS selection:** the CMTS assigns the TCS via TCC in REG-RSP-MP and DBC [normative] [MULPI §8.3.3]. See Gaps.

## Resets and re-registration

The reason is retained across reinit and reported in REG-REQ-MP TLV 57 [normative] [MULPI §C.1.3.6 Table 109]. Plant-drivable codes: POWER-ON (1), T17_LOST-SYNC (2), ALL_US_FAILED (3), T6_EXPIRED (6), FAILED_PRIM_DS (9), TCS_FAILED_ON_ALL_US (10), T4_EXPIRED (16), NO_PRIM_SF_USCHAN (17), CM_CTRL_INIT (18), DYNAMIC-RANGE-WINDOW-VIOLATION (19).

A bad CRC discards any MAC management message [normative] [MULPI §10.6]. A missed MAP blocks transmit for the period it covers, and a UCD count mismatch suspends the channel [normative] [MULPI §10.6 Table 98]. Device reboots and connection instability are listed among possible severe effects of wet-plant and standing-wave impairments [guideline] [PNM-HFC §8.1].

## Implications for hfc-twin

All [inferred]: our modeling proposals, not source statements.

| Physical condition (physics.md) | Expected symptom |
|---|---|
| Margin lost on a high-order profile: notch, water, reflections (§4.5, §4.6, §8) | Uncorrectables on that profile, code 16, partial channel, CMTS assigns a lower profile. No partial service |
| Margin lost on Profile A, NCP or PLC (§4.6 `plc`, `ncp`) | Lost lock cases 3-5; codes 16, 20, 21; partial channel, not reinit |
| All profiles, NCP and every other channel lost, pilots held (§7.4) | MAPs and replies stop decoding: T4 and T3 on upstream, then reinit, and up and down near the edge. Where this happens depends on the boot profile |
| Pilots or preamble lost: amp `Failed`, deep suckout, brownout (§6.3, §8) | Primary: upstream stops, T4 climbs, T3 flat, reinit NO_PRIM_SF_USCHAN near 30 s at defaults. Non-primary: partial service, code 2 |
| Upstream-only loss with healthy downstream (§7.1) | T3 at every opportunity, T4 flat at first. Code 7, then partial service or reinit. T4 may follow if the CMTS deregisters |
| Upstream ingress bursts, funneling (§7.3) | Sporadic T3 across the service group, p from burst SNR. No state change until consecutive losses |
| Net upstream loss rising: water, temperature, bad drop (§7.1, §8, §10) | `us_tx_dbmv` climbs, headroom falls, no MAC event until a limit is hit |
| Required P1.6 above P1.6hi (§7.2) | CMTS receive level below target by the deficit, lower `us_snr_db`, FEC and OUDP downgrade; with a larger deficit, T3 on the worst channel, then upstream partial service |
| Required P1.6 spread over 12 dB across the TCS: upstream tilt (§8) | SID bits set, fidelity relaxed, worst channel eventually dropped |
| Can't be heard within the DRW at registration (§7.2) | REG-ACK 30, or TCS_FAILED_ON_ALL_US and cycling |
| PSU sag, node outage (§6.3, §8) | Many modems reinit together, then ranging contention. The correlation across modems localizes the fault |

Rules that follow:

- **Derive limits.** Compute P1.6hi from Pmax and the TCS (invariant 1). The modem input is Pmax only. Keep the §7.2 per-6.4 MHz versus per-1.6 MHz comparison explicit.
- **Don't test `P_tx > limit` and set `partial_service`.** That scripts a symptom (invariant 5). Let the commanded level saturate per a policy, the received level fall, and burst loss drive T3.
- **Saturation policy is a profile parameter,** because above Pmax it is vendor-specific and what to command is CMTS policy. Clamping (with R10.0) is a reasonable v1 default, marked approx.
- **Vendor limits belong in the profile,** marked `generic`: FEC limits for lost lock and profile failure, CMTS tolerance, ranging detection threshold. Tie them to §4.6 curves.
- **Model both directions of ranging loss:** RNG-REQ or probe on the upstream (§7.3) and RNG-RSP or MAP on the downstream (§4.6). All draws, including CM-STATUS holdoff, use the seeded RNG.
- **Station maintenance spacing is a profile parameter.** T3 per interval = p x interval / spacing, capped at interval / 200 ms per channel.

**§9 fields are too simple.** Suggested direction, as a new schema version (not made here):

- `partial_service: bool` becomes per-channel, per-direction state: `us_channel_state[channel]` (usable, suspended, unusable) and `ds_channel_state[channel]`. Without direction a diagnoser can't separate upstream from downstream faults.
- Add `partial_channel` per OFDM channel with profile, NCP and PLC bits, mirroring `PartialChanState`. It is the earlier state in a gradual downstream fault.
- `t3_timeouts` and `t4_timeouts` become per upstream channel, matching the MIBs. Add `t3_exceeded`, `ranging_aborted`, `ranging_status`.
- Add `reg_state`, `resets` with last `init_reason`, `lost_syncs`, and DRW error bits.
- Add `assigned_ds_profiles[channel]` and `assigned_us_iucs[channel]`. Profile changes are a visible symptom, and `codewords[...][profile]` needs to know which are live.
- CM-STATUS events could be a timestamped list, or wait for a later version. Whether they fire depends on CMTS MDD policy, so they need a capability mask.

Test oracles available now: the P1.6hi table, 17-attempt exhaustion, reinit near 30 s on total downstream loss, and holdoff bounds.

## Gaps

- **Station maintenance and probe spacing:** CMTS-dependent, no value in these sources, yet it sets T3 rates. Needs a measured or vendor value.
- **Re-registration duration:** not given, and DHCP, ToD and TFTP timing is out of scope here. Summing maximum message intervals (PLC 0.25 s, MDD 2 s, UCD 2 s, ranging 2 s, T3 0.2 s) gives about 6.5 s before fine ranging, registration and provisioning, as a floor only [inferred].
- **Vendor limits:** lost-lock and profile FEC limits, CMTS tolerance, and the level at which a CMTS stops detecting ranging bursts are unspecified.
- **What carries MAC messages on OFDM:** that MAPs and RNG-RSP ride the channel's most robust profile is [inferred]. Not checked in MULPI.
- **Behavior above Pmax:** vendor-specific. PNM-HFC says a channel may be dropped but not which device decides. The sources it cites, [Hranac 2023] and [Hranac LLALC], aren't in our set.
- **TCS selection:** no algorithm in MULPI or PNM-HFC.
- **Retry wording:** 16 retries (Annex B), 17 T3 expirations (MULPI §10.6.1), 16 consecutive T3 (CM-OSSI R08.0). We read it as first attempt + 16 retries; no source confirms this with a worked example.
- **Power search gap:** initializing TCC channels, MULPI §8.3.3.1.1 and §8.3.3.1.2 say no untried gap over 3 dB, while §10.2.3.4.1 says 6 dB. Unresolved; treat as a profile value.
- **Counters:** whether a MAC reinit (not a system reboot) clears T3 and T4 counters isn't stated [CM-OSSI §F.2.3.2]. Nor is whether T3 expiry on a non-counting transmission (RNG-REQ with PMTI 1) increments `T3Timeouts`, though the attribute text suggests any expiry does.
- **Figures:** 133, 168, 169 and 180 were read from the PDF, and the step lists are our reading of flowcharts.
- **Not read:** MULPI Appendix XII, DBC state machines beyond §11.5.1.1 and §11.5.1.2.4, and MSM detail in PHY §7.4.12.1.
