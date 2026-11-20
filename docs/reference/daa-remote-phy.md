# Distributed access and Remote PHY

Scope: what Remote PHY and related distributed architectures change for a plant twin: where the PHY sits, RPD RF levels and ports, PNM collection, status, failure modes, timing and pre- and post-migration comparison.

Source notes: R-PHY is the 2026 specification; TR-DCA is a much older overview (see Gaps). Protocol encodings are out of scope; TLV numbers appear only to name a field. Sources are CableLabs documents and say nothing about any vendor's products.

## Architectures compared

| | I-CCAP | Remote PHY (RPD) | Remote MAC-PHY (RMD) |
|---|---|---|---|
| Split | None; MAC and PHY in one hub chassis | Between MAC and PHY | Above the MAC; compact CMTS in the node |
| In the field | Analog optics and an RF node | PHY: modulators, burst receivers, pseudowire logic | DOCSIS MAC and PHY, maybe forwarding and EQAM |
| Hub to node | Linear optics, RF-modulated signals | IP over digital optics (Ethernet or PON) | L2 link (Ethernet or PON) |
| Scheduler | Chassis | Core; distributed scheduler not specified yet | Node |
| DOCSIS timing | Internal [inferred] | Must share a timestamp with the Core | No hard distributed requirement |
| Management | CCAP OSSI | Core is the single config point; RPD is its slave | Direct OSSI, or a controller; no one standard |

Citations: R-PHY §5.1, §5.2.2, §12.1, §12.1.1; TR-DCA §7.1.1, §7.3, §8.8.1, §9.1, §9.2, §9.3, §9.7, §9.8.

- Core plus RPD is meant to be functionally equal to an I-CCAP. DOCSIS signaling and CPE do not change [R-PHY §5.1, §5.7; TR-DCA §8.1].
- The RPD takes no part in the MAC. Its one upstream exception is copying bandwidth requests into their own pseudowire so they can get priority [R-PHY §5.5.1, §5.7].
- C-DOCSIS II is a Split-MAC third option, with data forwarding and PHY in the node and scheduling and management in a hub controller [TR-DCA §7.1.1, §7.4]. Out of scope for us [inferred].
- Flexible MAC Architecture (FMA) is the later name for Remote MAC-PHY. The MAC can sit in an RMD, an RMC, a chassis pCore or a server vCore [PNM-HFC §VIII.1.1.6.2].

**Virtual CMTS.** Sources treat virtualization as a Core-side packaging choice:
- Virtual functions still need hardware for RF modulation, so some distributed architecture is a prerequisite [TR-DCA §11.10.1].
- PNM-HFC mentions cores that are physical or cloud native [PNM-HFC §VIII.1.1.6.1] and groups rCCAP and vCCAP as one row set for spectrum capture methods [PNM-HFC Table 23].
- High availability shifts to small failure groups and horizontal scaling, such as one virtual CMTS per serving group [TR-DCA §11.7].
- So a vCMTS with RPDs would look to the plant like any Remote PHY deployment: same RPD, same RF side, Core in software [inferred]. Whether a given vendor's product conforms in each detail is not established here (see Gaps).

## What moves into the node

A Remote PHY Node (RPN) holds two logical modules [R-PHY §5.4.2]:
- **RPD Module:** Ethernet or PON ports, DEPI and UEPI termination, modulators, demodulators, clock circuit, local CPU.
- **RF Module (RFM):** gain amplifiers, attenuators, tilt control, diplexers, combiners, diagnostic circuits.

Two reference points matter for every level:
- **Interface C:** between the RPD Module and the RFM. Annex E and F level requirements apply here.
- **Interface D:** the external Node Ports (NPs) facing the coax.

These may be one physical module, with no wired interface C [R-PHY §5.4.2]. Standard RFM management is limited to gains, downstream tilt and C-to-D topology [R-PHY §5.4.2].

Placement varies [R-PHY §5.2.4; §B.5.3.8.10]:
- **In the node (RPN):** digital optics; meets R-PHY Annex E and F and DRFI Annex D. `SupportsOpticalNodeRf` (50.40) = true.
- **In a hub shelf (RPS):** linear optics to an analog node; meets PHY and DRFI main requirements. `SupportsOpticalNodeRf` = false.

RF ports can drive coax or analog optics, and may be unidirectional or bidirectional [R-PHY §5.1]. RPD output can be combined with overlay analog or digital video [R-PHY §5.3.1]. The node may keep other optical receivers and transmitters for analog video or telemetry alongside the RPD Module [R-PHY §E.1.1, §F.1.1].

RPDs can share a backhaul through a switch or by daisy-chaining, where each RPD behaves like a three-port switch [R-PHY §12.2]. Failure of an upstream chain member would therefore cut the ones behind it [inferred].

The fiber link is the Converged Interconnect Network (CIN), which may include L2 switches and L3 routers [R-PHY §5.2.5]. The Core can then sit in a hub, headend or regional data center [R-PHY §5.2.5].

## RF side: what changes and what stays

Stays the same:
- The coax plant remains an FDM plant with its split [R-PHY §5.2.3]. Modems and DOCSIS signaling are unchanged [R-PHY §5.1].
- Powering: 60 or 90 VAC over coax with or without standby. Power draw may change, which can require more source capacity [TR-DCA §11.8].
- Plant defects remain. Ideal short runs would be free of ingress and micro-reflections, but aged and patched plant makes that unrealistic [TR-DCA §6.1.3].

Changes:
- **Analog optics leave the signal path.** Optical noise and system nonlinearity go with them, along with headend coax and splitting networks [TR-DCA §6.2.1; R-PHY §E.1.1, §F.1.1]. The stated aim is better received signal quality and higher modulation orders [R-PHY §E.1.1, §F.1.1].
- **Link length sets the size of the gain.** Strong cases are links beyond 25 km (82,000 ft) and replacing broadcast-narrowcast overlays. Gain may be considerably lower when the hub is within 8 km (5 miles, 26,400 ft) [TR-DCA §6.2.3].
- **Return-path spectrum monitoring moves.** Headend analyzers lose sight of the return path, so UTSC replaces them [PNM-HFC §8.6.2]. Sweep systems lose their headend connection through the combining network; alternatives are PNM tools, a node module or a node API [R-PHY App. I].
- **Faults specific to the analog path should disappear from that node** [inferred]. PNM-HFC lists them as laser clipping, failing lasers and setup errors on the node-to-hub link [PNM-HFC §7.2.5], and a failing headend return receiver [PNM-HFC §7.2.4]. Amplifier compression stays, and fiber damage would show as link failure rather than RF degradation [inferred].
- **Levels become configured digital values** rather than a consequence of headend combining. Adjacent-channel mismatch can come from RPD signals not matching headend or hub levels [PNM-HFC Table 24].

## RPD RF levels and port behavior

### Downstream

All at interface C and for an RPD in a node [R-PHY §F.1.1, Table 68]. All [normative].

| Parameter | Value |
|---|---|
| Channel Power Reference Setting | At least 20 dBmV per 6 MHz, no upper limit; may be fixed, local only, or remote |
| Full-fidelity level, adjustable RPD | At or below `60 − ceil(3.6·log2(Neq))` dBmV per 6 MHz |
| Commanded per-channel range | 0 to −2 dBc remotely; step at most 0.2 dB, strictly monotonic |
| Channel power matching | Adjacent ≤ 0.5 dB; within a 48 MHz block ≤ 1 dB; across 108–1218 MHz ≤ 2 dB normalized for bandwidth |
| Absolute accuracy per channel | ±3 dB |
| Diagnostic carrier suppression | ≥ 50 dB, in three remote modes |
| RF port mute | ≥ 73 dB below unmuted aggregate, in each 6 MHz channel, 54–1218 MHz |
| Load | 75 Ω |

- `Neq = N + 32·N_OFDM` equivalent legacy channels [R-PHY §F.2]. An OFDM channel's active count `Neq'` is `ceil(modulated_mhz / 6)` [R-PHY §F.2].
- Example: a fully loaded 108–1218 MHz port has Neq = 185 and a bound of 60 − 28 = 32 dBmV per 6 MHz [inferred; our arithmetic].
- Physics.md §3.1 uses 52 dBmV at 1218 MHz at the node output. That implies about 20 to 32 dB of RFM gain, depending on the reference setting [inferred].
- Commanded power must hold for Neq' down to Neq/4 (MUST), and down to Neq/16 (SHOULD) [R-PHY §F.2.1].
- Local-only adjustments cannot be changed remotely and may interrupt service [R-PHY §F.2.1].

Configuration and readback fields:
- `BasePower` (61.3), in 0.1 dBmV per 6 MHz, is the port reference. A channel's base power adds `10·log10(occupied_mhz / 6)`; 192 MHz adds 15.05 dB [R-PHY §B.5.7.14.3; our arithmetic]. `PowerAdjust` (62.16, in 0.1 dB) offsets each channel [R-PHY §B.5.7.16.15].
- Tilt at the port: `TiltValue` (61.5, 0.1 dB) and `TiltMaximumFrequency` (61.6, Hz) [R-PHY §B.5.7.14.3.2, §B.5.7.14.4].
- RFM gain: `DsCfgRfmGain` (160.1.2), where supported [R-PHY §B.5.7.42.1.2].
- Per Node Port status, `RfmStatus` (161.1) [R-PHY §B.5.9.8.1]:
  - `ReportedDsGain`, `ReportedRfmDsTilt` and `TotalDsTilt` in 0.1 dB.
  - `DsOutputPower` in 0.1 dBmV per 6 MHz of occupied spectrum.
  - A status enum says whether a gain or tilt was set by GCP, stored locally, defaulted or unknown; physical pads are the case behind the local values.
  - **A reported value of 0 means unknown**, for `DsOutputPower`, `UsExpectedRxPower`, gains and tilts.
- `RfMute` (61.4) is a diagnostic state: only output power changes, and `operStatus` and transmit counters continue [R-PHY §B.5.7.14.3.1, §B.5.8.1.1].

### Upstream

For an RPD in a node, Annex E replaces the PHY receiver level requirements [R-PHY §E.2, §E.3]. All [normative] except where noted.

| Modulation | Set point range, dBmV per 6.4 MHz | Burst window around set point |
|---|---|---|
| QPSK to 64-QAM | −4 to 25 | −9 / +3 dB |
| 128- and 256-QAM | 0 to 25 | −9 / +3 dB |
| 512- and 1024-QAM | 0 to 25 | −3 / +3 dB |
| 2048-QAM | 7 to 25 | −3 / +3 dB |
| 4096-QAM | 10 to 25 | −3 / +3 dB |

- An RPD may implement part of this range per US RF port, or one fixed set point [R-PHY §E.2].
- `BaseTargetRxPower` (98.3) carries the set point in 0.1 dBmV per 1.6 MHz, which reads 6.02 dB lower than per 6.4 MHz. Recommended range −20 to +40 dBmV per 1.6 MHz; default 0 [R-PHY §B.5.7.41.3; conversion ours]. Each channel adds a `TargetRxPowerAdjust` [R-PHY §E.2].
- Composite: the RPD must operate with average total power, including ingress and noise, up to 6 dB above the composite implied by the set point, at interface C. This replaces the PHY fixed 31 dBmV limit [R-PHY §E.3].
- Example: 15 dBmV per 6.4 MHz across 80 MHz gives 15 + 10.97 + 6 = 31.97 dBmV [inferred; our arithmetic].
- Typical values the spec describes [field; R-PHY §E.2, §E.3]: node input 8 to 24 dBmV per ATDMA channel; return transmitter input 0 to 10 dB above that; RPD input composite up to 35 dBmV average and 50 dBmV instantaneous, especially with 204 MHz high split.
- `UsExpectedRxPower` (161.1.9) reports expected input per Node Port in 0.1 dBmV per 1.6 MHz; `ReportedUsGain` gives the NP-to-C gain, typically attenuation [R-PHY §B.5.9.8.1.4, §B.5.9.8.1.9, §B.5.7.42.2].

### Ports and topology

- **Full spectrum (FS) ports** carry all channels in a direction. **Partial spectrum (PS) ports** carry a subset, and the RFM splits and combines them losslessly to Node Ports [R-PHY §5.4.3].
- `NodeRfPortCapabilities` (50.60.17) and `NodePortMap` (50.60.18) report the mapping. Absence of `NodePortMap` means no PS capability. Topology is fixed after reset [R-PHY §5.4.3, §5.4.3.3, §5.4.3.5].
- One example in the spec has SC-QAM per Node Port but OFDMA combined across NP pairs, on separate demodulators [R-PHY §5.4.3]. So **upstream noise funnels per RF port, and the group can differ by channel type within one node** [inferred from that example].
- Gain is pair-wise: a gain control index for each NP and RF port pair, defined only over that port's occupied spectrum [R-PHY §5.4.3.4].
- `operStatus` is up or down only. A port is up if at least one channel is. A channel is down if not physically capable or not configured, by vendor-specific criteria [R-PHY §B.5.8.1.1].
- **Virtual splitting and combining** can map one MAC-level service group onto several RPDs. DS uses multicast or unicast replication. US uses one pseudowire per RPD channel. RPDs are unaware of combining [R-PHY §12.4.1, §12.4.2].
- D4.0 modes (FDD, FDX) use PS ports heavily. An RPD runs one mode at a time and may need a reset to change [R-PHY §5.1, §18.1].

## PNM with an RPD

The split is by where the measurement physically lives. Modem-side tests stay on the modem. CMTS-side tests are captured by the RPD but controlled by and normally delivered through the Core [R-PHY §15].

| Test | Measured at | Control | Results reach PNM server by | Limits [normative unless noted] |
|---|---|---|---|---|
| DS RxMER, FBC, channel estimate, CM FEC, CM pre-eq | Modem | CM MIBs | CM upload, unchanged | [inferred from R-PHY §5.1] |
| DS symbol capture | Modem and RPD modulator | PNM server to Core; Core to RPD (`Dssc*`) | RPD to Core by GCP read; Core by TFTP | One per RPD; setup ≥ 0.5 s before trigger; result within 0.5 s [§15.1] |
| US histogram | RPD wideband receiver | Core to RPD (`UsHsg*`) | GCP read; Core by TFTP | One per RPD; 256 values; bin 0 is zero with odd symmetry [§15.2] |
| UTSC | RPD Spectrum Analysis Circuit (SAC) | Core to RPD (`Usc*`, `ScCfg*`) | SpecMan or PNM pseudowire to Core, or static pseudowire direct to PNM server | ≤ 100 kHz bins; free-running required; full US band on all US RF ports [§15.3] |
| UPC (active and quiet probes) | RPD | Core to RPD (`Upc*`, TLV 42) | PNM pseudowire per OFDMA channel | Required; one P-MAP per test [§15.4] |
| US RxMER per subcarrier | RPD, on a probe | MER bit in P-MAP; no GCP attributes | PNM pseudowire, shared with UPC or dedicated | Required; rate vendor-defined; no RxMER probes during UPC on a shared pseudowire [§15.5, §15.5.1] |
| US impulse noise | RPD | GCP | PNM or SpecMan pseudowire | Reported by capability bit; detail not in §15 [§15, §B.5.4.5.1] |

- **PNM server interface.** Symbol capture and histogram steps at the server are unchanged between I-CCAP and R-PHY, so one server implementation serves both [R-PHY §15.1, §15.2].
- **Required triggers on the RPD:** free-running, minislot count, first minislot of a SID, and active or quiet probe symbol. The RPD has no CM MAC trigger; the Core translates MAC to SID [R-PHY §15.3].
- **Capabilities the RPD reports:** `SupportedPnmTests` (50.64.1) bitmask: bit 0 UPC, 1 US RxMER, 2 impulse noise, 3 DS symbol capture, 4 histogram. UTSC is reported separately in `SpectrumCaptureCapabilities` (50.59): SAC count, per-SAC ranges, triggers, RF port reach and static pseudowire support [R-PHY §B.5.4.5.1, §15.3]. `UpcCapabilities` (50.64.2) gives probe symbol ranges by subcarrier spacing [R-PHY §B.5.4.5.2].
- **Calibration reference moves.** `CalibrationConstantK` is referenced to the line card connector on an I-CCAP and to interface C on an RPD [R-PHY §B.5.9.2.6.2; CCAP-OSSI §7.3.5.6.2.11]. Calibrated bin level: `D_calibrated = D_raw − K` [CCAP-OSSI §7.3.5.6.2.11]. Analysis at interface D through a single-NP PS port is optional, and concurrent reference points are not required [R-PHY §15.3].
- **File headers identify the RPD.** `RpdId` (6 bytes) and `RpdPortNum` (1 byte) appear in PNM capture files, and default to 0 or null on I-CCAP [CCAP-OSSI §7.3.3.1.7, §7.3.3.1.8, §7.3.4.1.11.6].
- **Indexing.** UTSC configuration on an RPD port may use `docsCableUpstreamRfPort`, rather than `docsCableUpstream` or `docsCableUpstreamChannel` [PNM-HFC §8.6.5.1.1].
- **Virtual splitting and combining break PNM's one-to-one assumption.** Symbol capture on a split channel involves several modulators, and current specs lack controls for it. Spectrum analysis and histogram on a combined group need redefinition. The Core must correct collision and no-energy counters [R-PHY §12.4.3].
- **Channel statistics** are read by the Core from the RPD. SC-QAM: `ChannelSnr` (78.7, 0.1 dB), FEC counters. OFDMA: `AverageMer` per IUC (79.2.9, 0.1 dB), pre- and post-decode counters [R-PHY §B.5.8.9, §B.5.8.10]. `EqualizationCoeffEnable` (65.11) shows the RPD sends pre-eq coefficients with ranging bursts [R-PHY §B.5.7.20.10].

Field practice [field]:
- UTSC direct from the RPD runs 80 to 100 Mbps per port, refreshing every 100 to 200 ms at 50 kHz for sub- and mid-split, 100 kHz for high-split and up. TFTP is about 1 Mbps at 200 ms to 1 s. Proprietary SNMP is under 50 kbps at 2 to 10 s [PNM-HFC Table 23, §6.4.4].
- Direct streaming needs IP protocol 115 open between RPD and server [PNM-HFC §6.4.4].
- R-OSSI does not cover configuring a direct pseudowire destination, so vendors use custom `docsPnmCmtsUtscCfgTable` settings or vendor MIBs [PNM-HFC §6.4.4, §8.6.3].
- Free-running is the only widely available trigger. All known UTSC deployments need proprietary elements [PNM-HFC §8.4.6, §8.6.2].
- Some platforms cannot run UTSC and RxMER at once, and concurrent captures are limited per device, line card and RPD [PNM-HFC §8.6.3].
- Some operators send telemetry to an auxiliary core to relieve the main Core [PNM-HFC §8.4.2].

## RPD status and telemetry

Three paths exist:
- **Through the Core (normal):** back office uses CLI, NETCONF or SNMP against the Core, which reads the RPD over GCP [TR-DCA §8.8.1].
- **Direct management (exception):** SSH or local CLI, and possibly SNMP [TR-DCA §8.8.4; R-PHY §5.4.1].
- **gNMI streaming:** the RPD is the server, dial-in or dial-out, and the client need not be a Core. GCP still does configuration [R-PHY §17.1, §17.3, §17.4, §17.5].

| Group | Fields | Source |
|---|---|---|
| Device state | `TopLevelRPDState` (87.1): localRpdInit, networkAuthentication, ipAddressAssignment, waitingTod, connectPrincipalCore, waitOperationalPrincipalCore, operationalPrincipalCore | §B.5.9.5.1 |
| Core sessions | `RpdGcpConnectionStatus`: Inactive, Connecting, Connected, Reconnecting; `AuthenticationStatus`; `AuxCoreRPDState` (87.4.4) | §7.2.1.2, §B.5.9.5.10 |
| Timing | `LocalPtpSyncStatus` (87.5); `PtpResult` (86.5): freeRunning, acquiring, holdoverOutOfSpec, holdoverWithinSpec, synchronized; `ClockState` values seen: holdover, phaseAligned; `RpdPtpPortCurrentClockSource` (97.8.19) | §B.5.9.5.11, §B.5.18.6.9, §8.2.3.3, Table 60 |
| Optical and Ethernet | `SfpPlusStatus` (100.18), `IfEnet` (100.8), `IfEnetStats` (100.9), per Ethernet port. Attributes in R-OSSI | Table 66 |
| Temperature and sensors | `RpdSensor` (100.7), `EntityObject` (100.6), per entity. Attributes in R-OSSI | Table 66 |
| RF | `RfmStatus` (161); `operStatus` per port and channel | §B.5.9.8, §B.5.8.1.1 |
| Pseudowires | `RpdL2tpSessionInfo` (100.2); buffer history (83) and alerts (84) | Table 65, Table 66 |
| Upstream timing symptoms | `LateMaps` (78.4), `IllegalMaps` (78.5), `LateMinislots` (78.11) | §B.5.8.9 |
| History | `ResetHistory` (100.31, at least 50 entries); `EventNotification` (85), with a pending queue kept during GCP loss | §B.5.3.13.4, §7.2.1.1 |

- `RpdInfo` (100) is defined in R-OSSI, which is the definitive source [R-PHY §B.5.18, Table 66]. We have field names only.
- Counters persist over an operational down and continue on return. `discontinuityTime` marks a reset [R-PHY §B.5.8].
- RF channel `operStatus` says nothing about pseudowire status [R-PHY §B.5.8.1.1].
- Compared with an analog node, which reports almost nothing unless a smart transponder is fitted, RPDs report much more, and a configurable device exposes hardware mismatches through config errors. Link monitoring helps separate network from node problems, but the information is not organized for diagnosis and vendor work matters most [TR-DCA §11.4].
- Legacy comparison: a transponder node may report optical power in units of 0.1 mW (`fnOpticalReceiverPower`, `fnReturnLaserOpticalPower`) and laser temperature in °C (`fnReturnLaserTemp`) [SCTE 38-5 §fnOpticalReceiverPower and related objects].

## Failure modes

R-PHY defines RPD behavior but rarely states the modem-side effect. The last column is ours [inferred].

| Failure | RPD behavior [normative] | Telemetry | Modem-visible effect [inferred] |
|---|---|---|---|
| RPD offline: power, crash, hard reset | Full init from Local RPD Init; stored config restored, including RF power and pilot tones [§6.3, §8.1] | Silence, then `TopLevelRPDState` sequence, Startup Notify, `ResetHistory`, event 66070212 | All modems on the RPD drop at once, DS and US; recovery needs PTP too |
| Soft reset | Abandons GCP and L2TPv3 sessions; holds 1588 frequency; PTP warm start in holdover if feasible; pilot tones SHOULD continue [§8.2.3] | Startup Notify status softReset; first PTP Notify may be holdover | Same outage shape, shorter; RF tones can persist with no data |
| CIN link down | At boot: no interface after `CinIfTimeout` (default 120 s, 1 to 600 s) leads to SoftResetAttempt, event 66070231; up logs 66070214 [§6.3.1, §6.13.1]. In service: an inactive Ethernet port counts as GCP loss [§7.1.3.2] | `IfEnet`, `SfpPlusStatus`, then GCP states at the Core | All modems drop; RPD itself may stay alive, unlike power loss; other ports or redundancy may save it [§6.12; TR-DCA §11.7] |
| GCP loss | Control plane only. Declared by idle timer (`MaxGcpIdleTime`, default 0 = off, up to 300 s, recommended ≥ 3× keepalive) or TCP or link indication. Runs `GcpRecoveryAction` [§7.1.3] | Events 66070200 (aux, backup) or 66070201 (principal); Core 66080400; `Reconnecting` | **No RF effect during reconnect**: config and L2TPv3 kept [§7.2.1.1]. Outage only if retries exhaust |
| GCP retries exhausted | Principal: SoftResetAttempt, or handover if configured; event 66070226. Auxiliary: per `AuxReconnectFailReset`; event 66070225 [§7.2.2] | As above, then reset sequence | Principal: full outage. Auxiliary: only that Core's channels |
| L2TPv3 loss | Sends `L2tpConnectionFailureNotify` to the failed Core and the Principal. Session IDs of 0 mean control connection; nonzero means one session [§19.5.4.5, §B.3.4.5, §B.5.18.12] | Notify, `RpdL2tpSessionInfo`; `operStatus` may still be up | One session: that channel or function only. PNM pseudowire: data gaps, no RF change. Control connection: all sessions on it |
| Loss of timing | Sync absent 5 s (`PTP_SYNC_TIMEOUT`): event 66070701, keeps trying; recovered: 66070700. Enters holdover. Out of spec for a clock application: 66070702 [§6.9.1.1, §6.9.1.5] | `PtpResult`, `ClockState`, `LocalPtpSyncStatus` | None at first: DOCSIS can run in holdover [§8.2.3.3]. Out of spec: eventual upstream failures; threshold unknown |
| Never synchronizes | After 300 s (`RPD_PTP_OK_TIMEOUT`): PTP Timeout Notify, event 66070703; keeps trying [§6.9.1.2] | RPD may wait in `waitOperationalPrincipalCore` | Core may hold RF channels off [§6.9, §6.10.1]; modems never register |
| Core and RPD clocks misaligned | Core may check by DLM; event 66080304 [§6.9.1.3] | Core event; negative latency possible | Late or misplaced grants; ranging trouble |
| Core failure or handover | Backup roles exist; RPD keeps config and L2TPv3 during handover (`GcpHandoverTimeout` default 30 s) [§7.1.3.3, §7.4] | `RpdBackupCoreStatus`, `HandoverNotify` | Wide blast radius across RPDs without backup [TR-DCA §11.7] |
| Init stall | Timeouts and retries lead to next core or reboot; `WaitOperationalRetryCount` exhausted: event 66070335 and reboot [§6.10.3, §6.13] | State stuck, events | RPD loops; modems stay off |
| CIN congestion | No state change; packets drop; PTP settling and tracking degrade [TR-DCA §11.9.2, §11.9.4.1] | `LateMaps`, `IfEnetStats`, buffer alerts | Throughput loss with clean RF; a non-plant cause |

GCP recovery defaults [R-PHY §7.1.3.3] [normative]: `GcpRecoveryAction` = 2 (`GcpReconnectToTheSameCore`); `GcpRecoveryActionRetry` = 3 (0 to 255); `GcpRecoveryActionDelay` = 0 s (0 to 600); `GcpReconnectTimeout` = 30 s (5 to 120). Keepalive reply within 1000 ms, SHOULD be 100 ms [R-PHY §7.1.2]. A Core may reject a reconnect on vendor-specific criteria [R-PHY §7.2.1.1].

SoftResetAttempt: one soft reset if `SoftResetAttemptPending` is true, otherwise hard reset; enabled by default [R-PHY §8.2.4]. So repeat failures escalate from a short outage to a long one [inferred].

Init timer defaults [R-PHY §6.13] [normative]: `CoreConnectTimeout` 60 s; `NO_IRA_RCVD_TIMEOUT` 90 s, 3 retries; `NO_REX_RCVD_TIMEOUT` 10 s, 3 retries; `InitialConfigCompleteTimeout` 300 s, 1 retry; `WaitOperationalTimeout` 300 s, 3 retries; `NO_PRINCIPAL_CORE_FOUND_TIMEOUT` 60 s, 3 retries; backoff 60 to 300 s. Full PTP sync typically takes several minutes [R-PHY §8.2.3.3].

Operational state is not RF state. `MoveToOperational` ends initialization, but `AdminStatus` and `RfMute` decide whether a port or channel is active, and channels can start earlier [R-PHY §6.10.3]. The state before operational is therefore not proof of no RF [inferred].

Multi-core: an auxiliary Core can supply DOCSIS, video or OOB on its own resource set, with the Principal owning common parameters such as downstream power [R-PHY §6.8.1]. Failure of a video Core would not touch DOCSIS channels [inferred].

Blast radius changes both ways: an RPD failure hits one small service group, while a Core or CIN failure can exceed a line card failure [TR-DCA §11.7]. New failure points appear [TR-DCA §11.7].

## Timing

- **DTI** gave a common 10.24 MHz and DOCSIS timestamp in MHAv1, where DS and US PHY were physically apart. R-PHY does not need that DTI server, because both PHYs share one RPD and clock [R-PHY §5.3.1, §5.4.1].
- **R-DTI** only has to share a timestamp between Core and RPD, for SYNC, MAP, REQ, burst receiver and scheduler. It uses IEEE 1588v2 PTP; a 1588-compliant CIN improves accuracy but is not required [R-PHY §5.5.2, §6.9].
- **Modes:** `RpdRdtiMode` (97.1): rpdNodeSlave (default, MUST), rpdNodeMaster (requirements deferred to a future version), rpdNodeSlaveHybrid (SyncE for frequency, PTP for phase; MUST if `SupportsSynce`) [R-PHY §B.5.16.1.1]. TR-DCA lists RPD master as a mode [TR-DCA §8.7]; the current spec takes precedence.
- **Sequence:** Principal sets `RpdPtpPortAdminState` up, RPD syncs to the grandmaster, then notifies all Cores [R-PHY §6.9]. PTP can proceed in parallel with configuration.
- **Holdover in spec** means meeting the configured `ClkApplications`, or DOCSIS if none is set [R-PHY §B.5.18.6.9].
- **Network effects:** moderate or heavy load hurts PTP settling and tracking. Mitigations are priority handling, transparent or boundary clocks, and SyncE [TR-DCA §11.9.4.1]. IPsec on PTP costs accuracy; MACsec is the supported alternative [R-PHY §6.9].
- **Latency:** fiber delay is about 500 µs per 100 km, or 1.5 µs per 1,000 ft (our conversion) [TR-DCA §11.9.3]. Performance is nearly identical to I-CMTS up to 100 miles (528,000 ft); beyond that, REQ-GNT grows by several ms and grant size compensates [R-PHY §12.1]. MAP and REQ priority makes the centralized scheduler behave similarly [R-PHY §5.6].
- **PNM depends on it:** DS symbol capture uses a 32-bit timestamp to pick the PLC frame [R-PHY §15.1]. One timestamp unit is 97.66 ns [R-PHY §B.5.8.4.8].
- **Remote MAC-PHY:** no hard timing need for DOCSIS, but 1588 and DTP for mobile backhaul [TR-DCA §9.7, §11.9.4].

## Pre- and post-migration comparison

Comparable directly: modem-side RxMER, FEC, channel estimate and FBC on unchanged coax [R-PHY §5.1]. All below are ways a naive comparison would mislead [inferred], with the fact behind each cited.

| Difference | Fact | Effect on a comparison |
|---|---|---|
| Analog optics gone | TR-DCA §6.2.1; R-PHY §E.1.1, §F.1.1 | Expect a rise in DS and US RxMER, larger on long links; no number in sources |
| Plant defects remain | TR-DCA §6.1.3 | Reflections, ingress and tilt should carry over; if they change, suspect the plant or the construction work |
| Set point changes | R-PHY §E.2 | Modem transmit levels can shift with no plant change; re-baseline level drift features |
| RF output re-aligned | R-PHY §5.4.3.4, §B.5.9.8.1 | DS levels and tilt can step at cutover |
| Calibration point | R-PHY §B.5.9.2.6.2 | UTSC dBmV is not comparable unless RFM gain is removed |
| UTSC instrument | PNM-HFC Table 23 | Faster sampling catches short bursts, so apparent burst noise can rise with no plant change |
| Histogram meaning | R-PHY §15.2; PNM-HFC §6.8.2 | Laser clipping cannot occur on the removed link; a clipping signature post-migration points to compression |
| Service group map | R-PHY §12.4; TR-DCA §6.1.2 | Node splits and virtual combining change the funnel, so compare per RF port and legs, not per legacy node |
| Combined counters | R-PHY §12.4.3 | Virtual-combined counters are Core-corrected aggregates |
| Identifiers | CCAP-OSSI §7.3.3.1.7; PNM-HFC §8.6.5.1.1 | Key on plant location; RpdId and ifIndex types change. Counters restart: honor `discontinuityTime` |
| New outage class | R-PHY §6.9, §7; TR-DCA §11.7 | Availability differs by failure group and adds timing and network causes |
| Power draw | TR-DCA §11.8 | Node load changes; re-run AC budget |

Use the migration as an explicit dated event on the time series, rather than treating its step changes as faults [inferred].

## Implications for hfc-twin

All [inferred].

**DAA variant profile.** The variant differs from its analog twin in more than `node_kind`:
- Legacy needs an optics noise term that physics.md lacks: §4.5 and §7.3 have no optical contribution, so today the sim would show no RxMER gain from migrating. Add a per-node analog link CNR and clipping model to the I-CCAP profile, and omit it on RPD.
- `design_levels.us_node_in_target_dbmv` stops meaning a return-path requirement (physics.md §7.2). Make it the RPD set point plus RFM gain, validated against the Annex E table and composite rule. Keep per 6.4 MHz; convert to 1.6 MHz at adapters.
- `capability` differs and needs more flags: UTSC, UPC, US histogram, impulse noise, DS symbol capture, per-RPD concurrency limits, RPD identity in file headers. The current list lacks these.
- Capability is a function of (Core, RPD model, firmware), not firmware alone; see the field facts on UTSC.
- `Rpd` component has no `spec` in docs/tasks/01-schemas.md §c. It needs RFM gain and tilt ranges, DS reference setting, set point range, power draw, CIN port count, SAC count and `SupportedPnmTests`.
- Model the node as an RPD Module plus RFM. RFM gain and tilt reuse the Amp/Node forward model; level limits apply at C and `out_dbmv_at_hi` at D.
- Plant schema needs RF port and Node Port topology, since upstream funneling per RF port can differ by channel type. Virtual combining also sums legs across RPDs.
- Keep RPS (hub shelf, linear optics) out of v1, but state that assumption rather than asserting that all DAA fiber is digital.

**Observation additions.** Per invariant 2, add a field only where the sim derives it:
- RPD state: `top_level_state`, `gcp_status`, `ptp_result`, `local_ptp_sync`, RF port `oper_status`, derived from fault set and timers.
- Levels: `ds_output_dbmv_per_6mhz[np]`, gain and tilt readback with an unknown case. Treat 0 as unknown in adapters.
- Optical: receive power needs a fiber span and transceiver budget as plant input; otherwise it is scripted. Temperature can derive from `Environment` plus a self-heating parameter.
- `late_maps` needs a CIN model; omit until there is one.

**RPD fault classes for step 9.** All are physical or network parameters per invariant 5, with state and telemetry computed from a state machine using §6.13 and §7.1.3 timers:
1. `RpdFailure { rpd_id, mode: HardReset | SoftReset | Dead }`.
2. `CinLinkFault { rpd_id, port, cause: Cut | ExtraOpticalLoss { loss_db } }`. Honors port redundancy and daisy chains.
3. `GcpLoss { rpd_id, core, duration_s }`, with no RF effect until retries exhaust. Key diagnostic trap.
4. `L2tpLoss { rpd_id, scope: ControlConnection | Session { channel or function } }`, including PNM pseudowire loss as data gaps only.
5. `TimingLoss { scope: Rpd | Grandmaster, duration_s, drift }` through holdover within and out of spec.
6. `RfmMisalignment { np, ds_gain_delta_db, tilt_delta_db, us_gain_delta_db }` and RFM failure, reusing amp drift logic. Likely at cutover.
7. `CoreFailure { core, has_backup }`, spanning many RPDs. Possibly later.
8. `CinCongestion { loss_frac, latency_ms, jitter_ms }`. Later; needs a CIN model.

Power sag needs no new class: an existing `Power supply sag` under an RPD brownout should drive `RpdFailure` behavior by derivation.

**Diagnosis.** RPD-wide simultaneous loss with clean prior RF points away from coax. Separation evidence: GCP or L2TP state, `ptp_result`, link status, AC telemetry. Train with held-out DAA layouts as DESIGN.md already plans, including the trap cases: GCP lost with healthy RF, and `operStatus` up with a failed pseudowire.

**Adapter.** Normalize units: 0.1 dBmV, per 1.6 or per 6 MHz, Hz to MHz, 0.01 dB for K. Refer UTSC to one reference point using RFM gain when known, and flag it when not.

## Gaps

- **No vendor vCore verified.** We need from a vCore vendor: its API or telemetry reference; RPD models and their `SupportedPnmTests` and SAC counts by firmware; UTSC delivery method and trigger support; optical, temperature and RF field names and units; `ClockState` and PTP exposure; RFM gain and tilt control (remote or pads); typical boot and PTP times; virtual split and combine usage; GCP recovery settings; how far the product conforms to R-PHY, and how an RPD and vCore map to it. DESIGN.md's "via the vCore" cannot be confirmed from these sources.
- **R-OSSI not in our set.** It defines `RpdInfo`: `SfpPlusStatus`, `RpdSensor`, `IfEnet`, `EntityObject`, also `ClockState`, event texts and Core `ifOperStatus` values. PNM-HFC cites CM-SP-R-OSSI-I23. Fetch it before step 9 fixes optical or temperature fields; no thresholds or units for these are known.
- **R-DTI not in our set.** Holdover drift limits, clock applications, and how long DOCSIS survives out-of-spec holdover are unknown.
- **R-DEPI and R-UEPI not in our set.** L2TPv3 failure detection timers, and detail on buffer monitoring and impulse noise delivery are unknown.
- **No numbers for migration gain.** Sources describe improved SNR without a dB value. Analog link CNR will need a generic value, or SCTE or field data.
- **No RPD temperature or optical alarm limits, power draw or boot time** in these sources. Spec sheets needed.
- **Modem consequences of RPD faults** are our inference. Check them against mac-operations.md and a lab where possible.
- **NPR tests** (`DsOfdmNoisePowerRatio` in CCAP-OSSI §7.3.4.2) are not mentioned in R-PHY §15; RPD behavior unverified. UsOfdmaRxPower likewise.
- **`ClockState` enumeration** is only seen in prose (holdover, phaseAligned), not as a defined TLV in R-PHY.
- **TR-DCA revision mismatch.** The file reads CM-TR-DCA-V01-150908, dated 2015-09-08; README.md lists 2020-09-02. Its content is older than current R-PHY; prefer R-PHY on conflict.
- **Extraction checks.** R-PHY Table 67 confirmed against the PDF text layer. The Table 68 title has math lost in extraction; values read clean. R-PHY §E.2 has a unit typo ("dBmV/6. MHz"), read as per 6.4 MHz per §B.5.7.41.3.
