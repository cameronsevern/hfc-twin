//! PNM file formats [CM-OSSI §D.2], with offsets from `docs/reference/collection-management.md` §5. Big-endian.
// `chunks_exact` reads as what these are: fixed-size fields off the wire.
#![allow(clippy::chunks_exact_to_as_chunks)]

use thiserror::Error;

/// A file that doesn't parse.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum PnmError {
    /// Too short for what its header says.
    #[error("truncated PNM file")]
    Truncated,
    /// Not the file type expected.
    #[error("file type {0:08x}, expected {1:08x}")]
    WrongType(u32, u32),
}

/// DS RxMER.
pub const TYPE_RXMER: u32 = 0x504E_4E04;
/// DS channel estimate.
pub const TYPE_CHAN_EST: u32 = 0x504E_4E02;
/// US OFDMA pre-eq.
pub const TYPE_US_PREEQ: u32 = 0x504E_4E06;
/// Full-band capture.
pub const TYPE_SPECTRUM: u32 = 0x504E_4E09;
/// CCAP US RxMER.
pub const TYPE_CCAP_US_RXMER: u32 = 0x504E_4E69;

/// What every CM file starts with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    /// File type.
    pub file_type: u32,
    /// Major version: 1 on CM files, 2 on CCAP.
    pub major: u8,
    /// Minor version.
    pub minor: u8,
    /// Capture time, Unix seconds.
    pub capture_time: u32,
}

/// Where subcarrier data sits in frequency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Axis {
    /// Subcarrier zero's frequency, Hz.
    pub subcarrier_zero_hz: u32,
    /// `FirstActiveSubcarrierIndex`.
    pub first_active: u16,
    /// Spacing, kHz.
    pub spacing_khz: u8,
}

/// RxMER or channel estimate: `RxMER 04, channel estimate 02` share a layout.
#[derive(Debug, Clone, PartialEq)]
pub struct DsFile<T> {
    /// The header.
    pub header: Header,
    /// Downstream channel id.
    pub channel_id: u8,
    /// The CM's MAC.
    pub cm_mac: [u8; 6],
    /// Frequency axis.
    pub axis: Axis,
    /// The data.
    pub data: Vec<T>,
}

/// US pre-eq.
#[derive(Debug, Clone, PartialEq)]
pub struct UsPreeqFile {
    /// The header.
    pub header: Header,
    /// Upstream channel id.
    pub channel_id: u8,
    /// The CM's MAC.
    pub cm_mac: [u8; 6],
    /// The CMTS's MAC.
    pub cmts_mac: [u8; 6],
    /// Frequency axis.
    pub axis: Axis,
    /// Coefficients as (re, im).
    pub data: Vec<(f64, f64)>,
}

/// Full-band capture.
#[derive(Debug, Clone, PartialEq)]
pub struct SpectrumFile {
    /// The header.
    pub header: Header,
    /// The CM's MAC.
    pub cm_mac: [u8; 6],
    /// First bin's centre, Hz.
    pub first_center_hz: u32,
    /// Last bin's centre, Hz.
    pub last_center_hz: u32,
    /// Span, Hz.
    pub span_hz: u32,
    /// Equivalent noise bandwidth, hundredths of a bin.
    pub enb: u16,
    /// Window code.
    pub window: u16,
    /// Bins, dBmV.
    pub bins_dbmv: Vec<f64>,
}

/// CCAP US RxMER.
#[derive(Debug, Clone, PartialEq)]
pub struct CcapUsRxmerFile {
    /// The header.
    pub header: Header,
    /// OFDMA ifIndex.
    pub if_index: u32,
    /// The CM's MAC.
    pub cm_mac: [u8; 6],
    /// Frequency axis.
    pub axis: Axis,
    /// RxMER per subcarrier, dB. `None` where unmeasured.
    pub data: Vec<Option<f64>>,
}

fn header(file_type: u32, major: u8, capture_time: u32, out: &mut Vec<u8>) {
    out.extend_from_slice(&file_type.to_be_bytes());
    out.push(major);
    out.push(0);
    out.extend_from_slice(&capture_time.to_be_bytes());
}

fn read_header(bytes: &[u8], expected: u32) -> Result<Header, PnmError> {
    let file_type = u32_at(bytes, 0)?;
    if file_type != expected {
        return Err(PnmError::WrongType(file_type, expected));
    }
    Ok(Header {
        file_type,
        major: *bytes.get(4).ok_or(PnmError::Truncated)?,
        minor: *bytes.get(5).ok_or(PnmError::Truncated)?,
        capture_time: u32_at(bytes, 6)?,
    })
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32, PnmError> {
    let slice = bytes.get(at..at + 4).ok_or(PnmError::Truncated)?;
    Ok(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn u16_at(bytes: &[u8], at: usize) -> Result<u16, PnmError> {
    let slice = bytes.get(at..at + 2).ok_or(PnmError::Truncated)?;
    Ok(u16::from_be_bytes([slice[0], slice[1]]))
}

fn mac_at(bytes: &[u8], at: usize) -> Result<[u8; 6], PnmError> {
    bytes
        .get(at..at + 6)
        .ok_or(PnmError::Truncated)?
        .try_into()
        .map_err(|_| PnmError::Truncated)
}

/// [CM-OSSI §D.2.1.3]: one byte per subcarrier, dB × 4, clipped at 0 and 0xFE. 0xFF means no measurement.
pub fn rxmer_byte(rxmer_db: Option<f64>) -> u8 {
    match rxmer_db {
        Some(db) => (db * 4.0).round().clamp(0.0, 254.0) as u8,
        None => 0xff,
    }
}

/// The inverse of `rxmer_byte`.
pub fn rxmer_from_byte(byte: u8) -> Option<f64> {
    (byte != 0xff).then(|| f64::from(byte) / 4.0)
}

/// [CM-OSSI §D.2.1.2]: two's complement s2.13, I then Q.
pub fn s2_13(value: f64) -> i16 {
    (value * 8192.0).round().clamp(-32767.0, 32767.0) as i16
}

fn write_ds_prefix(
    file_type: u32,
    capture_time: u32,
    channel_id: u8,
    cm_mac: [u8; 6],
    axis: Axis,
    len: u32,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(28 + len as usize);
    header(file_type, 1, capture_time, &mut out);
    out.push(channel_id);
    out.extend_from_slice(&cm_mac);
    out.extend_from_slice(&axis.subcarrier_zero_hz.to_be_bytes());
    out.extend_from_slice(&axis.first_active.to_be_bytes());
    out.push(axis.spacing_khz);
    out.extend_from_slice(&len.to_be_bytes());
    out
}

type DsPrefix<'a> = (Header, u8, [u8; 6], Axis, &'a [u8]);

fn read_ds_prefix(bytes: &[u8], expected: u32) -> Result<DsPrefix<'_>, PnmError> {
    let header = read_header(bytes, expected)?;
    let channel_id = *bytes.get(10).ok_or(PnmError::Truncated)?;
    let cm_mac = mac_at(bytes, 11)?;
    let axis = Axis {
        subcarrier_zero_hz: u32_at(bytes, 17)?,
        first_active: u16_at(bytes, 21)?,
        spacing_khz: *bytes.get(23).ok_or(PnmError::Truncated)?,
    };
    let len = u32_at(bytes, 24)? as usize;
    let data = bytes.get(28..28 + len).ok_or(PnmError::Truncated)?;
    Ok((header, channel_id, cm_mac, axis, data))
}

/// Writes an RxMER file.
pub fn encode_rxmer(
    capture_time: u32,
    channel_id: u8,
    cm_mac: [u8; 6],
    axis: Axis,
    rxmer_db: &[Option<f64>],
) -> Vec<u8> {
    let mut out = write_ds_prefix(
        TYPE_RXMER,
        capture_time,
        channel_id,
        cm_mac,
        axis,
        rxmer_db.len() as u32,
    );
    out.extend(rxmer_db.iter().map(|m| rxmer_byte(*m)));
    out
}

/// Reads an RxMER file.
pub fn decode_rxmer(bytes: &[u8]) -> Result<DsFile<Option<f64>>, PnmError> {
    let (header, channel_id, cm_mac, axis, data) = read_ds_prefix(bytes, TYPE_RXMER)?;
    Ok(DsFile {
        header,
        channel_id,
        cm_mac,
        axis,
        data: data.iter().map(|b| rxmer_from_byte(*b)).collect(),
    })
}

/// Writes a channel estimate file. `None` is an excluded subcarrier, 0x8000 on I and Q.
pub fn encode_chan_est(
    capture_time: u32,
    channel_id: u8,
    cm_mac: [u8; 6],
    axis: Axis,
    coefficients: &[Option<(f64, f64)>],
) -> Vec<u8> {
    let mut out = write_ds_prefix(
        TYPE_CHAN_EST,
        capture_time,
        channel_id,
        cm_mac,
        axis,
        (coefficients.len() * 4) as u32,
    );
    for coefficient in coefficients {
        let (i, q) = match coefficient {
            Some((re, im)) => (s2_13(*re), s2_13(*im)),
            None => (i16::MIN, i16::MIN),
        };
        out.extend_from_slice(&i.to_be_bytes());
        out.extend_from_slice(&q.to_be_bytes());
    }
    out
}

fn complex_from(pair: &[u8]) -> Option<(f64, f64)> {
    let i = i16::from_be_bytes([pair[0], pair[1]]);
    let q = i16::from_be_bytes([pair[2], pair[3]]);
    (i != i16::MIN || q != i16::MIN).then(|| (f64::from(i) / 8192.0, f64::from(q) / 8192.0))
}

/// Reads a channel estimate file.
pub fn decode_chan_est(bytes: &[u8]) -> Result<DsFile<Option<(f64, f64)>>, PnmError> {
    let (header, channel_id, cm_mac, axis, data) = read_ds_prefix(bytes, TYPE_CHAN_EST)?;
    Ok(DsFile {
        header,
        channel_id,
        cm_mac,
        axis,
        data: data.chunks_exact(4).map(complex_from).collect(),
    })
}

/// Writes a US pre-eq file.
pub fn encode_us_preeq(
    capture_time: u32,
    channel_id: u8,
    cm_mac: [u8; 6],
    cmts_mac: [u8; 6],
    axis: Axis,
    coefficients: &[(f64, f64)],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(34 + coefficients.len() * 4);
    header(TYPE_US_PREEQ, 1, capture_time, &mut out);
    out.push(channel_id);
    out.extend_from_slice(&cm_mac);
    out.extend_from_slice(&cmts_mac);
    out.extend_from_slice(&axis.subcarrier_zero_hz.to_be_bytes());
    out.extend_from_slice(&axis.first_active.to_be_bytes());
    out.push(axis.spacing_khz);
    out.extend_from_slice(&((coefficients.len() * 4) as u32).to_be_bytes());
    for (re, im) in coefficients {
        out.extend_from_slice(&s2_13(*re).to_be_bytes());
        out.extend_from_slice(&s2_13(*im).to_be_bytes());
    }
    out
}

/// Reads a US pre-eq file.
pub fn decode_us_preeq(bytes: &[u8]) -> Result<UsPreeqFile, PnmError> {
    let header = read_header(bytes, TYPE_US_PREEQ)?;
    let channel_id = *bytes.get(10).ok_or(PnmError::Truncated)?;
    let cm_mac = mac_at(bytes, 11)?;
    let cmts_mac = mac_at(bytes, 17)?;
    let axis = Axis {
        subcarrier_zero_hz: u32_at(bytes, 23)?,
        first_active: u16_at(bytes, 27)?,
        spacing_khz: *bytes.get(29).ok_or(PnmError::Truncated)?,
    };
    let len = u32_at(bytes, 30)? as usize;
    let data = bytes.get(34..34 + len).ok_or(PnmError::Truncated)?;
    Ok(UsPreeqFile {
        header,
        channel_id,
        cm_mac,
        cmts_mac,
        axis,
        data: data
            .chunks_exact(4)
            .map(|c| complex_from(c).unwrap_or((0.0, 0.0)))
            .collect(),
    })
}

/// Writes a full-band capture: bins as 16-bit hundredths of a dB [CM-OSSI §D.2.1.4].
#[allow(clippy::too_many_arguments)]
pub fn encode_spectrum(
    capture_time: u32,
    cm_mac: [u8; 6],
    first_center_hz: u32,
    last_center_hz: u32,
    span_hz: u32,
    enb: u16,
    window: u16,
    bins_dbmv: &[f64],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(39 + bins_dbmv.len() * 2);
    header(TYPE_SPECTRUM, 1, capture_time, &mut out);
    out.push(0);
    out.extend_from_slice(&cm_mac);
    out.extend_from_slice(&first_center_hz.to_be_bytes());
    out.extend_from_slice(&last_center_hz.to_be_bytes());
    out.extend_from_slice(&span_hz.to_be_bytes());
    out.extend_from_slice(&(bins_dbmv.len() as u16).to_be_bytes());
    out.extend_from_slice(&enb.to_be_bytes());
    out.extend_from_slice(&window.to_be_bytes());
    out.extend_from_slice(&((bins_dbmv.len() * 2) as u32).to_be_bytes());
    for bin in bins_dbmv {
        let hundredths = (bin * 100.0).round().clamp(-32768.0, 32767.0) as i16;
        out.extend_from_slice(&hundredths.to_be_bytes());
    }
    out
}

/// Reads a full-band capture.
pub fn decode_spectrum(bytes: &[u8]) -> Result<SpectrumFile, PnmError> {
    let header = read_header(bytes, TYPE_SPECTRUM)?;
    let cm_mac = mac_at(bytes, 11)?;
    let len = u32_at(bytes, 35)? as usize;
    let data = bytes.get(39..39 + len).ok_or(PnmError::Truncated)?;
    Ok(SpectrumFile {
        header,
        cm_mac,
        first_center_hz: u32_at(bytes, 17)?,
        last_center_hz: u32_at(bytes, 21)?,
        span_hz: u32_at(bytes, 25)?,
        enb: u16_at(bytes, 31)?,
        window: u16_at(bytes, 33)?,
        bins_dbmv: data
            .chunks_exact(2)
            .map(|c| f64::from(i16::from_be_bytes([c[0], c[1]])) / 100.0)
            .collect(),
    })
}

/// Writes a CCAP US RxMER file.
pub fn encode_ccap_us_rxmer(
    capture_time: u32,
    if_index: u32,
    ccap_id: &str,
    cm_mac: [u8; 6],
    axis: Axis,
    rxmer_db: &[Option<f64>],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(297 + rxmer_db.len());
    header(TYPE_CCAP_US_RXMER, 2, capture_time, &mut out);
    out.extend_from_slice(&if_index.to_be_bytes());
    let mut id = [0u8; 256];
    for (slot, byte) in id.iter_mut().zip(ccap_id.bytes().take(255)) {
        *slot = byte;
    }
    out.extend_from_slice(&id);
    out.extend_from_slice(&[0u8; 6]);
    out.push(0);
    out.extend_from_slice(&cm_mac);
    out.extend_from_slice(&1u16.to_be_bytes());
    out.push(1);
    out.extend_from_slice(&axis.subcarrier_zero_hz.to_be_bytes());
    out.extend_from_slice(&axis.first_active.to_be_bytes());
    out.push(axis.spacing_khz);
    out.extend_from_slice(&(rxmer_db.len() as u32).to_be_bytes());
    out.extend(rxmer_db.iter().map(|m| rxmer_byte(*m)));
    out
}

/// Reads a CCAP US RxMER file.
pub fn decode_ccap_us_rxmer(bytes: &[u8]) -> Result<CcapUsRxmerFile, PnmError> {
    let header = read_header(bytes, TYPE_CCAP_US_RXMER)?;
    let len = u32_at(bytes, 293)? as usize;
    let data = bytes.get(297..297 + len).ok_or(PnmError::Truncated)?;
    Ok(CcapUsRxmerFile {
        header,
        if_index: u32_at(bytes, 10)?,
        cm_mac: mac_at(bytes, 277)?,
        axis: Axis {
            subcarrier_zero_hz: u32_at(bytes, 286)?,
            first_active: u16_at(bytes, 290)?,
            spacing_khz: *bytes.get(292).ok_or(PnmError::Truncated)?,
        },
        data: data.iter().map(|b| rxmer_from_byte(*b)).collect(),
    })
}

/// [CM-OSSI §7.1.3.7]: `DocsEqualizerData`: main tap location, taps per symbol, forward taps, reverse
/// taps, then signed 16-bit real and imaginary per tap. Scale here is 1.0 at 2047.
pub fn encode_equalizer(main_tap: u8, taps: &[(f64, f64)]) -> Vec<u8> {
    let mut out = vec![main_tap, 1, taps.len() as u8, 0];
    for (re, im) in taps {
        let scale = |x: f64| (x * 2047.0).round().clamp(-32768.0, 32767.0) as i16;
        out.extend_from_slice(&scale(*re).to_be_bytes());
        out.extend_from_slice(&scale(*im).to_be_bytes());
    }
    out
}

/// Reads `DocsEqualizerData`: (main tap, taps).
pub fn decode_equalizer(bytes: &[u8]) -> Result<(u8, Vec<(f64, f64)>), PnmError> {
    let main_tap = *bytes.first().ok_or(PnmError::Truncated)?;
    let forward = *bytes.get(2).ok_or(PnmError::Truncated)? as usize;
    let data = bytes.get(4..4 + forward * 4).ok_or(PnmError::Truncated)?;
    Ok((
        main_tap,
        data.chunks_exact(4)
            .map(|c| {
                (
                    f64::from(i16::from_be_bytes([c[0], c[1]])) / 2047.0,
                    f64::from(i16::from_be_bytes([c[2], c[3]])) / 2047.0,
                )
            })
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x2a];
    const AXIS: Axis = Axis {
        subcarrier_zero_hz: 827_600_000,
        first_active: 148,
        spacing_khz: 50,
    };

    #[test]
    fn spec_examples() {
        // 23.75 dB is 0x5F [CM-OSSI §D.2.1.3].
        assert_eq!(rxmer_byte(Some(23.75)), 0x5f);
        assert_eq!(rxmer_from_byte(0x5f), Some(23.75));
        assert_eq!(rxmer_byte(None), 0xff);
        assert_eq!(rxmer_byte(Some(80.0)), 0xfe);
        // 0x2400F800 in s2.13 is 1.125 − 0.25j [CM-OSSI §D.2.1.2].
        assert_eq!(s2_13(1.125) as u16, 0x2400);
        assert_eq!(s2_13(-0.25) as u16, 0xf800);
        assert_eq!(
            complex_from(&[0x24, 0x00, 0xf8, 0x00]),
            Some((1.125, -0.25))
        );
        // Excluded subcarrier.
        assert_eq!(complex_from(&[0x80, 0x00, 0x80, 0x00]), None);
    }

    #[test]
    fn rxmer_file_offsets() {
        let bytes = encode_rxmer(1000, 7, MAC, AXIS, &[Some(40.0), None]);
        assert_eq!(&bytes[..4], &[0x50, 0x4e, 0x4e, 0x04]);
        assert_eq!(bytes[4], 1);
        assert_eq!(bytes[10], 7);
        assert_eq!(&bytes[11..17], &MAC);
        assert_eq!(u32_at(&bytes, 17).unwrap(), 827_600_000);
        assert_eq!(u16_at(&bytes, 21).unwrap(), 148);
        assert_eq!(bytes[23], 50);
        assert_eq!(u32_at(&bytes, 24).unwrap(), 2);
        assert_eq!(bytes[28], 160);
        assert_eq!(bytes[29], 0xff);
        let file = decode_rxmer(&bytes).unwrap();
        assert_eq!(file.data, vec![Some(40.0), None]);
        assert_eq!(file.axis, AXIS);
        assert_eq!(file.header.capture_time, 1000);
    }

    #[test]
    fn other_files_round_trip() {
        let c = vec![Some((1.125, -0.25)), None];
        let bytes = encode_chan_est(5, 1, MAC, AXIS, &c);
        assert_eq!(decode_chan_est(&bytes).unwrap().data, c);

        let p = vec![(0.5, -0.5), (1.0, 0.25)];
        let bytes = encode_us_preeq(5, 2, MAC, [9; 6], AXIS, &p);
        let file = decode_us_preeq(&bytes).unwrap();
        assert_eq!(file.data, p);
        assert_eq!(file.cmts_mac, [9; 6]);

        let bytes = encode_spectrum(
            5,
            MAC,
            54_000_000,
            1_000_000_000,
            946_000_000,
            150,
            1,
            &[-12.34, 20.0],
        );
        let file = decode_spectrum(&bytes).unwrap();
        assert_eq!(file.bins_dbmv, vec![-12.34, 20.0]);
        assert_eq!(file.first_center_hz, 54_000_000);
        assert_eq!(file.window, 1);

        let m = vec![Some(30.0), None];
        let bytes = encode_ccap_us_rxmer(5, 36, "ccap-1", MAC, AXIS, &m);
        assert_eq!(bytes.len(), 299);
        let file = decode_ccap_us_rxmer(&bytes).unwrap();
        assert_eq!(file.data, m);
        assert_eq!(file.if_index, 36);
        assert_eq!(file.cm_mac, MAC);

        // `08 01 18 00` is main tap 8, 1 per symbol, 24 forward [PNM-HFC §6.2.6.4.1].
        let taps = vec![(0.5, 0.0); 24];
        let bytes = encode_equalizer(8, &taps);
        assert_eq!(&bytes[..4], &[8, 1, 0x18, 0]);
        let (main, read) = decode_equalizer(&bytes).unwrap();
        assert_eq!(main, 8);
        assert!((read[3].0 - 0.5).abs() < 1e-3);
    }

    #[test]
    fn truncated_and_wrong_files_are_errors() {
        let bytes = encode_rxmer(1, 1, MAC, AXIS, &[Some(1.0); 10]);
        assert_eq!(decode_rxmer(&bytes[..20]), Err(PnmError::Truncated));
        assert_eq!(decode_rxmer(&bytes[..30]), Err(PnmError::Truncated));
        assert!(matches!(
            decode_chan_est(&bytes),
            Err(PnmError::WrongType(_, _))
        ));
        assert_eq!(decode_rxmer(&[]), Err(PnmError::Truncated));
    }
}
