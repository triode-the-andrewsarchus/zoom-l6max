//! Parameter modeling, scaling, and OLED string formatting for Dual-EFX.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum EfxParamId {
    // Velvet Noise Decorrelator (Effect A)
    VndAmount = 0,
    VndTimeSpread = 1,
    VndDensity = 2,
    VndDecay = 3,
    VndRefresh = 4,
    VndMode = 5,

    // Plateau Plate Reverb (Effect B)
    PlatPreDelay = 6,
    PlatSize = 7,
    PlatDecay = 8,
    PlatLowCut = 9,
    PlatHighCut = 10,
    PlatDiffusion = 11,
    PlatModDepth = 12,
    PlatFreeze = 13,

    // Global / Routing
    GlobalCascade = 14,
}

pub struct FormattedParam {
    pub title: &'static str,
    pub value_str: [u8; 16],
    pub value_len: usize,
}

impl FormattedParam {
    pub fn new(title: &'static str) -> Self {
        Self {
            title,
            value_str: [0; 16],
            value_len: 0,
        }
    }
}

/// Helper to write an integer into a byte buffer.
fn format_int(buf: &mut [u8], mut val: i32) -> usize {
    if val == 0 {
        buf[0] = b'0';
        return 1;
    }
    let negative = val < 0;
    if negative {
        val = -val;
    }
    let mut temp = [0u8; 10];
    let mut i = 0;
    while val > 0 {
        temp[i] = b'0' + (val % 10) as u8;
        val /= 10;
        i += 1;
    }
    let mut out_len = 0;
    if negative {
        buf[out_len] = b'-';
        out_len += 1;
    }
    while i > 0 {
        i -= 1;
        buf[out_len] = temp[i];
        out_len += 1;
    }
    out_len
}

/// Helper to write a float with 1 decimal place.
fn format_float_1dp(buf: &mut [u8], val: f32) -> usize {
    let negative = val < 0.0;
    let magnitude = if negative { -val } else { val };
    let scaled = (magnitude * 10.0 + 0.5) as i32;
    let mut len = 0;
    if negative && scaled != 0 {
        buf[0] = b'-';
        len = 1;
    }
    len += format_int(&mut buf[len..], scaled / 10);
    if len + 2 <= buf.len() {
        buf[len] = b'.';
        buf[len + 1] = b'0' + (scaled % 10) as u8;
        len += 2;
    }
    len
}

fn append_str(buf: &mut [u8], start: usize, s: &[u8]) -> usize {
    let mut idx = start;
    for &b in s {
        if idx >= buf.len() {
            break;
        }
        buf[idx] = b;
        idx += 1;
    }
    idx
}

/// Formats the current DSP value for the L6max OLED notification popup.
/// Values come from the effect state, so the display follows the applied
/// parameters rather than a separate control-range approximation.
pub fn format_parameter(id: EfxParamId, processor: &super::routing::DualEfxProcessor) -> FormattedParam {
    let velvet = processor.velvet.current;
    let plateau = processor.plateau.params;
    let (title, value, unit): (&str, f32, &[u8]) = match id {
        EfxParamId::VndAmount => ("VND AMOUNT", velvet.amount * 100.0, b" %"),
        EfxParamId::VndTimeSpread => ("VND TIME SPREAD", velvet.length_ms, b" ms"),
        EfxParamId::VndDensity => ("VND DENSITY", velvet.density, b" pps"),
        EfxParamId::VndDecay => ("VND DECAY", velvet.decay_db, b" dB"),
        EfxParamId::VndRefresh => ("VND REFRESH", velvet.refresh_seconds, b" s"),
        EfxParamId::VndMode => {
            let mut p = FormattedParam::new("VND MODE");
            p.value_len = append_str(&mut p.value_str, 0, if velvet.dynamic { b"DYNAMIC" } else { b"STATIC" });
            return p;
        }
        EfxParamId::PlatPreDelay => ("PLATEAU PREDELAY", plateau.predelay_ms, b" ms"),
        EfxParamId::PlatSize => ("PLATEAU SIZE", plateau.size * 100.0, b" %"),
        EfxParamId::PlatDecay => ("PLATEAU DECAY", plateau.decay * 100.0, b" %"),
        EfxParamId::PlatLowCut => ("PLATEAU LOW CUT", plateau.low_cut_hz, b" Hz"),
        EfxParamId::PlatHighCut => ("PLATEAU HIGH CUT", plateau.high_cut_hz, b" Hz"),
        EfxParamId::PlatDiffusion => ("PLATEAU DIFFUSION", plateau.diffusion * 100.0, b" %"),
        EfxParamId::PlatModDepth => ("PLATEAU MOD DEPTH", plateau.mod_depth, b" samples"),
        EfxParamId::PlatFreeze => {
            let mut p = FormattedParam::new("PLATEAU FREEZE");
            p.value_len = append_str(&mut p.value_str, 0, if plateau.freeze { b"FROZEN" } else { b"OFF" });
            return p;
        }
        EfxParamId::GlobalCascade => ("A -> B CASCADE", processor.cascade_gain * 100.0, b" %"),
    };
    let mut p = FormattedParam::new(title);
    let len = format_float_1dp(&mut p.value_str, value);
    p.value_len = append_str(&mut p.value_str, len, unit);
    p
}
