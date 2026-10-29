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
    let scaled = (val * 10.0 + 0.5) as i32;
    let whole = scaled / 10;
    let frac = (scaled % 10).abs();
    let mut len = format_int(buf, whole);
    if len < buf.len() - 2 {
        buf[len] = b'.';
        buf[len + 1] = b'0' + frac as u8;
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

/// Formats a parameter for the L6max OLED notification popup.
pub fn format_parameter(id: EfxParamId, raw: u8) -> FormattedParam {
    match id {
        EfxParamId::VndAmount => {
            let mut p = FormattedParam::new("VND AMOUNT");
            let pct = (raw as u32 * 100 / 127) as i32;
            let len = format_int(&mut p.value_str, pct);
            p.value_len = append_str(&mut p.value_str, len, b" %");
            p
        }
        EfxParamId::VndTimeSpread => {
            let mut p = FormattedParam::new("VND TIME SPREAD");
            // raw 0-127 -> 5.0 ms to 80.0 ms
            let ms = 5.0 + (raw as f32 / 127.0) * 75.0;
            let len = format_float_1dp(&mut p.value_str, ms);
            p.value_len = append_str(&mut p.value_str, len, b" ms");
            p
        }
        EfxParamId::VndDensity => {
            let mut p = FormattedParam::new("VND DENSITY");
            // raw 0-127 -> 200 to 2500 pulses/sec
            let density = 200 + (raw as i32 * 2300 / 127);
            let len = format_int(&mut p.value_str, density);
            p.value_len = append_str(&mut p.value_str, len, b" pps");
            p
        }
        EfxParamId::VndDecay => {
            let mut p = FormattedParam::new("VND DECAY");
            // raw 0-127 -> -80 dB to -10 dB
            let db = -80 + (raw as i32 * 70 / 127);
            let len = format_int(&mut p.value_str, db);
            p.value_len = append_str(&mut p.value_str, len, b" dB");
            p
        }
        EfxParamId::VndRefresh => {
            let mut p = FormattedParam::new("VND REFRESH");
            // raw 0-127 -> 0.5 s to 10.0 s
            let sec = 0.5 + (raw as f32 / 127.0) * 9.5;
            let len = format_float_1dp(&mut p.value_str, sec);
            p.value_len = append_str(&mut p.value_str, len, b" s");
            p
        }
        EfxParamId::VndMode => {
            let mut p = FormattedParam::new("VND MODE");
            if raw < 64 {
                p.value_len = append_str(&mut p.value_str, 0, b"DYNAMIC");
            } else {
                p.value_len = append_str(&mut p.value_str, 0, b"STATIC");
            }
            p
        }
        EfxParamId::PlatPreDelay => {
            let mut p = FormattedParam::new("PLATEAU PREDELAY");
            // raw 0-127 -> 0 to 150 ms
            let ms = raw as i32 * 150 / 127;
            let len = format_int(&mut p.value_str, ms);
            p.value_len = append_str(&mut p.value_str, len, b" ms");
            p
        }
        EfxParamId::PlatSize => {
            let mut p = FormattedParam::new("PLATEAU SIZE");
            let pct = (raw as u32 * 100 / 127) as i32;
            let len = format_int(&mut p.value_str, pct);
            p.value_len = append_str(&mut p.value_str, len, b" %");
            p
        }
        EfxParamId::PlatDecay => {
            let mut p = FormattedParam::new("PLATEAU DECAY");
            let pct = (raw as u32 * 100 / 127) as i32;
            let len = format_int(&mut p.value_str, pct);
            p.value_len = append_str(&mut p.value_str, len, b" %");
            p
        }
        EfxParamId::PlatLowCut => {
            let mut p = FormattedParam::new("PLATEAU LOW CUT");
            let hz = 20 + (raw as i32 * 480 / 127);
            let len = format_int(&mut p.value_str, hz);
            p.value_len = append_str(&mut p.value_str, len, b" Hz");
            p
        }
        EfxParamId::PlatHighCut => {
            let mut p = FormattedParam::new("PLATEAU HIGH CUT");
            let khz = 1.0 + (raw as f32 / 127.0) * 15.0;
            let len = format_float_1dp(&mut p.value_str, khz);
            p.value_len = append_str(&mut p.value_str, len, b" kHz");
            p
        }
        EfxParamId::PlatDiffusion => {
            let mut p = FormattedParam::new("PLATEAU DIFFUSION");
            let pct = (raw as u32 * 100 / 127) as i32;
            let len = format_int(&mut p.value_str, pct);
            p.value_len = append_str(&mut p.value_str, len, b" %");
            p
        }
        EfxParamId::PlatModDepth => {
            let mut p = FormattedParam::new("PLATEAU MOD DEPTH");
            let pct = (raw as u32 * 100 / 127) as i32;
            let len = format_int(&mut p.value_str, pct);
            p.value_len = append_str(&mut p.value_str, len, b" %");
            p
        }
        EfxParamId::PlatFreeze => {
            let mut p = FormattedParam::new("PLATEAU FREEZE");
            if raw < 64 {
                p.value_len = append_str(&mut p.value_str, 0, b"OFF");
            } else {
                p.value_len = append_str(&mut p.value_str, 0, b"FROZEN");
            }
            p
        }
        EfxParamId::GlobalCascade => {
            let mut p = FormattedParam::new("A -> B CASCADE");
            let pct = (raw as u32 * 100 / 127) as i32;
            let len = format_int(&mut p.value_str, pct);
            p.value_len = append_str(&mut p.value_str, len, b" %");
            p
        }
    }
}
