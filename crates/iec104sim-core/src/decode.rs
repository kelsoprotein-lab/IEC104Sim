//! Standalone IEC 60870-5-104 frame parser for the **报文解析** UI tool.
//!
//! Produces a fully-structured, serde-friendly view of one APDU — APCI header,
//! ASDU header, and every information object (IOA / value / quality / timestamp).
//! Independent of the receive path in `master.rs` / `slave.rs` so it can be
//! invoked on arbitrary user-pasted bytes without touching connection state.

use crate::data_point::DataPointValue;
use crate::frame::{parse_apci, FrameType, UFrameKind};
use crate::types::{AsduTypeId, CauseOfTransmission, QualityFlags};
use serde::{Deserialize, Serialize};

/// Top-level result of parsing one APDU.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedFrame {
    pub raw_hex: String,
    pub length: usize,
    pub start_byte: u8,
    pub apdu_length: u8,
    pub control_field: [u8; 4],
    pub apci: ParsedApci,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asdu: Option<ParsedAsdu>,
    pub warnings: Vec<String>,
}

/// APCI-layer view (Information / Supervisory / Unnumbered).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "frame_type", rename_all = "snake_case")]
pub enum ParsedApci {
    I { send_seq: u16, recv_seq: u16 },
    S { recv_seq: u16 },
    U { kind: UFrameKind, name: String },
}

/// ASDU header + decoded information objects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedAsdu {
    pub type_id: u8,
    pub type_name: String,
    pub sq: bool,
    pub num_objects: u8,
    pub cot: u8,
    pub cot_name: String,
    pub negative: bool,
    pub test: bool,
    pub originator: u8,
    pub common_address: u16,
    pub objects: Vec<ParsedObject>,
}

/// One information object inside the ASDU.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedObject {
    pub ioa: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<DataPointValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<QualityFlags>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<Cp56Time2a>,
    /// Raw element bytes (after IOA), shown in the UI for diagnostics.
    pub raw_hex: String,
}

/// Decoded CP56Time2a timestamp (7-byte IEC time format).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Cp56Time2a {
    pub year: u16,    // full year, e.g. 2026
    pub month: u8,    // 1..=12
    pub day: u8,      // 1..=31
    pub day_of_week: u8, // 1..=7, 0 = unused
    pub hour: u8,     // 0..=23
    pub minute: u8,   // 0..=59
    pub millisecond: u16, // 0..=59999
    pub invalid: bool,
    pub summer_time: bool,
}

/// Element layout (excluding IOA): (value_bytes_excluding_timestamp, has_cp56time2a).
/// Mirrors `master::asdu_element_size` exactly.
fn asdu_element_size(asdu_type: u8) -> Option<(usize, bool)> {
    match asdu_type {
        1  => Some((1, false)),  // M_SP_NA_1: SIQ
        30 => Some((1, true)),   // M_SP_TB_1
        3  => Some((1, false)),  // M_DP_NA_1: DIQ
        31 => Some((1, true)),   // M_DP_TB_1
        5  => Some((2, false)),  // M_ST_NA_1: VTI + QDS
        32 => Some((2, true)),   // M_ST_TB_1
        7  => Some((5, false)),  // M_BO_NA_1: BSI(4) + QDS
        33 => Some((5, true)),   // M_BO_TB_1
        9  => Some((3, false)),  // M_ME_NA_1: NVA(2) + QDS
        34 => Some((3, true)),
        11 => Some((3, false)),  // M_ME_NB_1: SVA(2) + QDS
        35 => Some((3, true)),
        13 => Some((5, false)),  // M_ME_NC_1: float(4) + QDS
        36 => Some((5, true)),
        15 => Some((5, false)),  // M_IT_NA_1: BCR(4+1)
        37 => Some((5, true)),
        45 => Some((1, false)),  // C_SC_NA_1: SCO(1)
        46 => Some((1, false)),  // C_DC_NA_1: DCO(1)
        47 => Some((1, false)),  // C_RC_NA_1: RCO(1)
        48 => Some((3, false)),  // C_SE_NA_1: NVA(2) + QOS(1)
        49 => Some((3, false)),  // C_SE_NB_1: SVA(2) + QOS(1)
        50 => Some((5, false)),  // C_SE_NC_1: float(4) + QOS(1)
        100 => Some((1, false)), // C_IC_NA_1: QOI
        101 => Some((1, false)), // C_CI_NA_1: QCC
        103 => Some((7, false)), // C_CS_NA_1: CP56Time2a as value
        _ => None,
    }
}

fn quality_from_qds(qds: u8) -> QualityFlags {
    QualityFlags {
        ov: qds & 0x01 != 0,
        bl: qds & 0x10 != 0,
        sb: qds & 0x20 != 0,
        nt: qds & 0x40 != 0,
        iv: qds & 0x80 != 0,
    }
}

/// Decode a 7-byte CP56Time2a buffer.
fn decode_cp56time2a(buf: &[u8]) -> Option<Cp56Time2a> {
    if buf.len() < 7 { return None; }
    let ms = u16::from_le_bytes([buf[0], buf[1]]);
    let minute = buf[2] & 0x3F;
    let invalid = buf[2] & 0x80 != 0;
    let hour = buf[3] & 0x1F;
    let summer_time = buf[3] & 0x80 != 0;
    let day = buf[4] & 0x1F;
    let day_of_week = (buf[4] >> 5) & 0x07;
    let month = buf[5] & 0x0F;
    let year_raw = buf[6] & 0x7F;
    // CP56Time2a: 2-digit year, conventionally interpreted as 2000+yy
    let year = 2000u16 + year_raw as u16;
    Some(Cp56Time2a {
        year, month, day, day_of_week, hour, minute,
        millisecond: ms, invalid, summer_time,
    })
}

/// Format a slice of bytes as space-separated uppercase hex.
fn hex_of(buf: &[u8]) -> String {
    buf.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ")
}

/// Decode one information element (excluding the IOA prefix).
/// Returns (value, quality, timestamp). Any field may be None depending on type.
fn decode_element(
    asdu_type: u8,
    elem: &[u8],
) -> (Option<DataPointValue>, Option<QualityFlags>, Option<Cp56Time2a>) {
    match asdu_type {
        // ---- Monitor: SIQ / DIQ (quality embedded in value byte) ----
        1 | 30 => {
            let siq = elem[0];
            let value = DataPointValue::SinglePoint { value: siq & 0x01 != 0 };
            let q = QualityFlags {
                ov: false,
                bl: siq & 0x10 != 0,
                sb: siq & 0x20 != 0,
                nt: siq & 0x40 != 0,
                iv: siq & 0x80 != 0,
            };
            let ts = if asdu_type == 30 { decode_cp56time2a(&elem[1..]) } else { None };
            (Some(value), Some(q), ts)
        }
        3 | 31 => {
            let diq = elem[0];
            let value = DataPointValue::DoublePoint { value: diq & 0x03 };
            let q = QualityFlags {
                ov: false,
                bl: diq & 0x10 != 0,
                sb: diq & 0x20 != 0,
                nt: diq & 0x40 != 0,
                iv: diq & 0x80 != 0,
            };
            let ts = if asdu_type == 31 { decode_cp56time2a(&elem[1..]) } else { None };
            (Some(value), Some(q), ts)
        }
        // ---- Monitor: VTI + QDS ----
        5 | 32 => {
            let vti = elem[0];
            let raw = vti & 0x7F;
            // VTI is a signed 7-bit number (-64..+63)
            let v_signed: i8 = if raw >= 64 { (raw as i16 - 128) as i8 } else { raw as i8 };
            let transient = vti & 0x80 != 0;
            let value = DataPointValue::StepPosition { value: v_signed, transient };
            let q = quality_from_qds(elem[1]);
            let ts = if asdu_type == 32 { decode_cp56time2a(&elem[2..]) } else { None };
            (Some(value), Some(q), ts)
        }
        // ---- Monitor: BSI(4) + QDS ----
        7 | 33 => {
            let bsi = u32::from_le_bytes([elem[0], elem[1], elem[2], elem[3]]);
            let value = DataPointValue::Bitstring { value: bsi };
            let q = quality_from_qds(elem[4]);
            let ts = if asdu_type == 33 { decode_cp56time2a(&elem[5..]) } else { None };
            (Some(value), Some(q), ts)
        }
        // ---- Monitor: NVA(2) + QDS ----
        9 | 34 => {
            let nva = i16::from_le_bytes([elem[0], elem[1]]);
            let value = DataPointValue::Normalized { value: nva as f32 / 32767.0 };
            let q = quality_from_qds(elem[2]);
            let ts = if asdu_type == 34 { decode_cp56time2a(&elem[3..]) } else { None };
            (Some(value), Some(q), ts)
        }
        // ---- Monitor: SVA(2) + QDS ----
        11 | 35 => {
            let sva = i16::from_le_bytes([elem[0], elem[1]]);
            let value = DataPointValue::Scaled { value: sva };
            let q = quality_from_qds(elem[2]);
            let ts = if asdu_type == 35 { decode_cp56time2a(&elem[3..]) } else { None };
            (Some(value), Some(q), ts)
        }
        // ---- Monitor: float(4) + QDS ----
        13 | 36 => {
            let f = f32::from_le_bytes([elem[0], elem[1], elem[2], elem[3]]);
            let value = DataPointValue::ShortFloat { value: f };
            let q = quality_from_qds(elem[4]);
            let ts = if asdu_type == 36 { decode_cp56time2a(&elem[5..]) } else { None };
            (Some(value), Some(q), ts)
        }
        // ---- Monitor: BCR(4+1) ----
        15 | 37 => {
            let counter = i32::from_le_bytes([elem[0], elem[1], elem[2], elem[3]]);
            let bcr = elem[4];
            let carry = bcr & 0x20 != 0;
            let sequence = bcr & 0x1F;
            let value = DataPointValue::IntegratedTotal { value: counter, carry, sequence };
            // BCR also carries IV/CY/CA flags but we only have IV worth surfacing
            let q = QualityFlags { iv: bcr & 0x80 != 0, ..Default::default() };
            let ts = if asdu_type == 37 { decode_cp56time2a(&elem[5..]) } else { None };
            (Some(value), Some(q), ts)
        }
        // ---- Control: SCO/DCO/RCO are 1-byte command qualifiers ----
        45 => {
            let sco = elem[0];
            let value = DataPointValue::SinglePoint { value: sco & 0x01 != 0 };
            (Some(value), None, None)
        }
        46 => {
            let dco = elem[0];
            let value = DataPointValue::DoublePoint { value: dco & 0x03 };
            (Some(value), None, None)
        }
        47 => {
            let rco = elem[0];
            let v = (rco & 0x03) as i8;
            let value = DataPointValue::StepPosition { value: v, transient: false };
            (Some(value), None, None)
        }
        // ---- Control set-points: NVA / SVA / float + QOS ----
        48 => {
            let nva = i16::from_le_bytes([elem[0], elem[1]]);
            let value = DataPointValue::Normalized { value: nva as f32 / 32767.0 };
            (Some(value), None, None)
        }
        49 => {
            let sva = i16::from_le_bytes([elem[0], elem[1]]);
            let value = DataPointValue::Scaled { value: sva };
            (Some(value), None, None)
        }
        50 => {
            let f = f32::from_le_bytes([elem[0], elem[1], elem[2], elem[3]]);
            let value = DataPointValue::ShortFloat { value: f };
            (Some(value), None, None)
        }
        // ---- System commands: QOI / QCC are single qualifier bytes ----
        100 | 101 => {
            let q = elem[0];
            let value = DataPointValue::Bitstring { value: q as u32 };
            (Some(value), None, None)
        }
        // ---- Clock sync: 7-byte CP56Time2a as the payload ----
        103 => {
            let ts = decode_cp56time2a(elem);
            (None, None, ts)
        }
        _ => (None, None, None),
    }
}

/// Parse a complete IEC 104 APDU (APCI + optional ASDU + objects).
///
/// Always returns a `ParsedFrame`; soft errors (length mismatch, unknown
/// ASDU type, truncated objects) are accumulated in `warnings` rather than
/// short-circuiting, so the UI can still show whatever was decoded.
pub fn parse_frame_full(data: &[u8]) -> Result<ParsedFrame, String> {
    if data.len() < 6 {
        return Err(format!("frame too short: {} bytes, need ≥ 6", data.len()));
    }
    if data[0] != 0x68 {
        return Err(format!("invalid start byte 0x{:02X} (expected 0x68)", data[0]));
    }

    let mut warnings = Vec::new();
    let apdu_length = data[1];
    let expected_total = 2 + apdu_length as usize;
    if expected_total != data.len() {
        warnings.push(format!(
            "APDU length field says {} bytes ({} total), but got {} bytes",
            apdu_length, expected_total, data.len()
        ));
    }

    let control_field = [data[2], data[3], data[4], data[5]];

    let frame = parse_apci(data).map_err(|e| e.to_string())?;
    let apci = match frame {
        FrameType::IFrame { send_seq, recv_seq, .. } => ParsedApci::I { send_seq, recv_seq },
        FrameType::SFrame { recv_seq } => ParsedApci::S { recv_seq },
        FrameType::UFrame { kind } => ParsedApci::U { kind, name: kind.name().to_string() },
    };

    let asdu = if matches!(apci, ParsedApci::I { .. }) {
        Some(parse_asdu(data, &mut warnings))
    } else {
        None
    };

    Ok(ParsedFrame {
        raw_hex: hex_of(data),
        length: data.len(),
        start_byte: data[0],
        apdu_length,
        control_field,
        apci,
        asdu,
        warnings,
    })
}

/// Parse the ASDU portion of an I-frame, starting at byte 6.
fn parse_asdu(data: &[u8], warnings: &mut Vec<String>) -> ParsedAsdu {
    // ASDU header is 6 bytes: type / VSQ / COT / OA / CA(2)
    if data.len() < 12 {
        warnings.push(format!("ASDU header truncated: only {} bytes after APCI", data.len() - 6));
        return ParsedAsdu {
            type_id: data.get(6).copied().unwrap_or(0),
            type_name: String::from("(truncated)"),
            sq: false, num_objects: 0,
            cot: 0, cot_name: String::new(),
            negative: false, test: false,
            originator: 0, common_address: 0,
            objects: Vec::new(),
        };
    }

    let type_id = data[6];
    let vsq = data[7];
    let sq = vsq & 0x80 != 0;
    let num_objects = vsq & 0x7F;
    let cause_byte = data[8];
    let cot = cause_byte & 0x3F;
    let negative = cause_byte & 0x40 != 0;
    let test = cause_byte & 0x80 != 0;
    let originator = data[9];
    let common_address = u16::from_le_bytes([data[10], data[11]]);

    let type_name = AsduTypeId::from_u8(type_id)
        .map(|t| t.name().to_string())
        .unwrap_or_else(|| format!("Type{}", type_id));
    let cot_name = CauseOfTransmission::from_u8(cot)
        .map(|c| c.name().to_string())
        .unwrap_or_else(|| format!("COT{}", cot));

    let objects = decode_objects(type_id, sq, num_objects as usize, &data[12..], warnings);

    ParsedAsdu {
        type_id, type_name,
        sq, num_objects,
        cot, cot_name,
        negative, test,
        originator, common_address,
        objects,
    }
}

fn decode_objects(
    type_id: u8,
    sq: bool,
    num: usize,
    body: &[u8],
    warnings: &mut Vec<String>,
) -> Vec<ParsedObject> {
    let elem_layout = match asdu_element_size(type_id) {
        Some(layout) => layout,
        None => {
            warnings.push(format!("unsupported ASDU type {} — objects not decoded", type_id));
            return Vec::new();
        }
    };
    let elem_size = elem_layout.0 + if elem_layout.1 { 7 } else { 0 };

    let mut out = Vec::with_capacity(num);
    let mut offset: usize = 0;
    let mut base_ioa: u32 = 0;

    for i in 0..num {
        let need_ioa = !sq || i == 0;
        if need_ioa {
            if offset + 3 > body.len() {
                warnings.push(format!("truncated at object #{}: missing IOA", i));
                break;
            }
            base_ioa = u32::from_le_bytes([body[offset], body[offset + 1], body[offset + 2], 0]);
            offset += 3;
        }
        let ioa = if sq { base_ioa + i as u32 } else { base_ioa };

        if offset + elem_size > body.len() {
            warnings.push(format!("truncated at object #{} (IOA={}): need {} more bytes", i, ioa, elem_size));
            break;
        }
        let elem = &body[offset..offset + elem_size];
        let (value, quality, timestamp) = decode_element(type_id, elem);
        out.push(ParsedObject {
            ioa,
            value,
            quality,
            timestamp,
            raw_hex: hex_of(elem),
        });
        offset += elem_size;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_u_frame_startdt_act() {
        let bytes = [0x68, 0x04, 0x07, 0x00, 0x00, 0x00];
        let p = parse_frame_full(&bytes).unwrap();
        assert!(p.asdu.is_none());
        match p.apci {
            ParsedApci::U { kind, .. } => assert_eq!(kind, UFrameKind::StartDtAct),
            _ => panic!("expected U-frame"),
        }
    }

    #[test]
    fn test_s_frame() {
        // recv_seq = 5
        let bytes = [0x68, 0x04, 0x01, 0x00, 0x0A, 0x00];
        let p = parse_frame_full(&bytes).unwrap();
        match p.apci {
            ParsedApci::S { recv_seq } => assert_eq!(recv_seq, 5),
            _ => panic!("expected S-frame"),
        }
    }

    #[test]
    fn test_i_frame_m_me_nc_1() {
        // I-frame: send=0 recv=0; ASDU: type=13(M_ME_NC_1) VSQ=1 COT=3 OA=0 CA=1
        // IOA=1 (3 bytes LE) + float 1.5 (4 bytes LE) + QDS=0
        let mut bytes = vec![0x68, 0x10, 0x00, 0x00, 0x00, 0x00];
        bytes.extend_from_slice(&[0x0D, 0x01, 0x03, 0x00, 0x01, 0x00]);
        bytes.extend_from_slice(&[0x01, 0x00, 0x00]);
        bytes.extend_from_slice(&1.5f32.to_le_bytes());
        bytes.push(0x00);
        let p = parse_frame_full(&bytes).unwrap();
        let asdu = p.asdu.expect("I-frame must have ASDU");
        assert_eq!(asdu.type_id, 13);
        assert_eq!(asdu.type_name, "M_ME_NC_1");
        assert_eq!(asdu.cot, 3);
        assert_eq!(asdu.common_address, 1);
        assert_eq!(asdu.objects.len(), 1);
        let obj = &asdu.objects[0];
        assert_eq!(obj.ioa, 1);
        match obj.value.as_ref().unwrap() {
            DataPointValue::ShortFloat { value } => assert!((value - 1.5).abs() < 1e-6),
            _ => panic!("expected ShortFloat"),
        }
        assert!(obj.quality.is_some());
        assert!(obj.timestamp.is_none());
    }

    #[test]
    fn test_i_frame_sq_multiple_points() {
        // M_SP_NA_1 (type=1) SQ=1 with 3 single-points, base IOA=10
        // SIQ bytes: 0x01, 0x00, 0x01
        let mut bytes = vec![0x68, 0x0E, 0x00, 0x00, 0x00, 0x00];
        bytes.extend_from_slice(&[0x01, 0x83, 0x14, 0x00, 0x01, 0x00]); // VSQ = 0x80|3
        bytes.extend_from_slice(&[0x0A, 0x00, 0x00]); // IOA=10
        bytes.extend_from_slice(&[0x01, 0x00, 0x01]); // 3 SIQ values
        let p = parse_frame_full(&bytes).unwrap();
        let asdu = p.asdu.unwrap();
        assert!(asdu.sq);
        assert_eq!(asdu.cot, 0x14);
        assert_eq!(asdu.objects.len(), 3);
        assert_eq!(asdu.objects[0].ioa, 10);
        assert_eq!(asdu.objects[1].ioa, 11);
        assert_eq!(asdu.objects[2].ioa, 12);
        assert!(matches!(asdu.objects[0].value, Some(DataPointValue::SinglePoint { value: true })));
        assert!(matches!(asdu.objects[1].value, Some(DataPointValue::SinglePoint { value: false })));
    }

    #[test]
    fn test_i_frame_with_cp56time2a() {
        // M_ME_TF_1 (type=36): float + QDS + CP56Time2a
        // year 2026 → raw 26 (0x1A), month 4, day 29, hour 12, min 30, ms 1000
        let mut bytes = vec![0x68, 0x17, 0x00, 0x00, 0x00, 0x00];
        bytes.extend_from_slice(&[0x24, 0x01, 0x03, 0x00, 0x01, 0x00]); // type 36, VSQ=1, COT=3, CA=1
        bytes.extend_from_slice(&[0x05, 0x00, 0x00]); // IOA=5
        bytes.extend_from_slice(&3.14f32.to_le_bytes());
        bytes.push(0x00); // QDS
        // CP56Time2a: ms=1000(0x03 0xE8), min=30, hour=12, day=29, month=4, year=26
        bytes.extend_from_slice(&[0xE8, 0x03, 30, 12, 29, 4, 26]);
        let p = parse_frame_full(&bytes).unwrap();
        let asdu = p.asdu.unwrap();
        assert_eq!(asdu.objects.len(), 1);
        let ts = asdu.objects[0].timestamp.unwrap();
        assert_eq!(ts.year, 2026);
        assert_eq!(ts.month, 4);
        assert_eq!(ts.day, 29);
        assert_eq!(ts.hour, 12);
        assert_eq!(ts.minute, 30);
        assert_eq!(ts.millisecond, 1000);
    }

    #[test]
    fn test_i_frame_c_sc_na_1_control() {
        // C_SC_NA_1 type 45, COT=7 (act-con) negative=0
        let mut bytes = vec![0x68, 0x0E, 0x00, 0x00, 0x00, 0x00];
        bytes.extend_from_slice(&[0x2D, 0x01, 0x07, 0x00, 0x01, 0x00]);
        bytes.extend_from_slice(&[0x64, 0x00, 0x00]); // IOA=100
        bytes.push(0x01); // SCO: ON
        let p = parse_frame_full(&bytes).unwrap();
        let asdu = p.asdu.unwrap();
        assert_eq!(asdu.type_id, 45);
        assert_eq!(asdu.cot, 7);
        assert_eq!(asdu.cot_name, "激活确认");
        assert!(!asdu.negative);
        assert_eq!(asdu.objects.len(), 1);
        assert_eq!(asdu.objects[0].ioa, 100);
        assert!(matches!(asdu.objects[0].value, Some(DataPointValue::SinglePoint { value: true })));
    }

    #[test]
    fn test_invalid_start_byte() {
        let bytes = [0x69, 0x04, 0x07, 0x00, 0x00, 0x00];
        let err = parse_frame_full(&bytes).unwrap_err();
        assert!(err.contains("start byte"));
    }

    #[test]
    fn test_too_short() {
        let bytes = [0x68, 0x04, 0x07];
        assert!(parse_frame_full(&bytes).is_err());
    }

    #[test]
    fn test_unknown_asdu_type_warns() {
        // I-frame with type 99 (unknown), VSQ=1
        let bytes = vec![
            0x68, 0x0E, 0x00, 0x00, 0x00, 0x00,
            0x63, 0x01, 0x03, 0x00, 0x01, 0x00,
            0x01, 0x00, 0x00, 0xFF,
        ];
        let p = parse_frame_full(&bytes).unwrap();
        let asdu = p.asdu.unwrap();
        assert_eq!(asdu.type_id, 99);
        assert!(asdu.type_name.starts_with("Type"));
        assert!(asdu.objects.is_empty());
        assert!(p.warnings.iter().any(|w| w.contains("unsupported ASDU type 99")));
    }

    #[test]
    fn test_apdu_length_mismatch_warns() {
        // length field says 0x10 but we only give 8 bytes
        let bytes = [0x68, 0x10, 0x07, 0x00, 0x00, 0x00];
        let p = parse_frame_full(&bytes).unwrap();
        assert!(p.warnings.iter().any(|w| w.contains("APDU length")));
    }
}
