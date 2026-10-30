//! Parameter modeling, scaling, and OLED string formatting for Dual-EFX.

pub use super::parameter_view::{EfxParamId, FormattedParam, ParameterSnapshot, format_snapshot};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParameterBank {
    Velvet,
    Plateau,
}

/// Applies one inspector encoder turn to the DSP state and returns the value
/// to present. The caller must provide exclusive access to the processor;
/// firmware event delivery and synchronization are not implemented here.
pub fn edit_encoder(
    processor: &mut super::routing::DualEfxProcessor,
    bank: ParameterBank,
    encoder: usize,
    delta: i32,
) -> Option<EfxParamId> {
    use super::math::clamp;
    use EfxParamId::*;
    if delta == 0 || encoder >= 8 {
        return None;
    }
    let step = delta as f32;
    let id = match bank {
        ParameterBank::Velvet => {
            let mut p = processor.velvet.current;
            let id = match encoder {
                0 => { p.amount += step * 0.02; VndAmount }
                1 => { p.length_ms += step; VndTimeSpread }
                2 => { p.density = clamp(p.density + step * 50.0, 100.0, 3000.0); VndDensity }
                3 => { p.decay_db += step * 2.0; VndDecay }
                4 => { p.refresh_seconds = clamp(p.refresh_seconds + step * 0.5, 0.1, 30.0); VndRefresh }
                5 => { p.dynamic = delta > 0; VndMode }
                6 => { p.transition_ms += step * 25.0; VndTransition }
                _ => {
                    processor.cascade_gain = clamp(processor.cascade_gain + step * 0.1, 0.0, 1.0);
                    return Some(GlobalCascade);
                }
            };
            processor.velvet.set_parameters(p);
            id
        }
        ParameterBank::Plateau => {
            let mut p = processor.plateau.params;
            let id = match encoder {
                0 => { p.predelay_ms = clamp(p.predelay_ms + step * 2.0, 0.0, 150.0); PlatPreDelay }
                1 => { p.size += step * 0.02; PlatSize }
                2 => { p.decay += step * 0.02; PlatDecay }
                3 => { p.low_cut_hz = clamp(p.low_cut_hz + step * 20.0, 20.0, 1000.0); PlatLowCut }
                4 => { p.high_cut_hz = clamp(p.high_cut_hz + step * 250.0, 1000.0, 16000.0); PlatHighCut }
                5 => { p.diffusion += step * 0.05; PlatDiffusion }
                6 => { p.mod_depth += step * 0.25; PlatModDepth }
                _ => { p.freeze = delta > 0; PlatFreeze }
            };
            processor.plateau.set_parameters(p);
            id
        }
    };
    Some(id)
}

/// Copies the applied parameter value for later presentation on another task.
pub fn snapshot_parameter(id: EfxParamId, processor: &super::routing::DualEfxProcessor) -> ParameterSnapshot {
    use EfxParamId::*;
    let velvet = processor.velvet.current;
    let plateau = processor.plateau.params;
    let value = match id {
        VndAmount => velvet.amount * 100.0,
        VndTimeSpread => velvet.length_ms,
        VndDensity => velvet.density,
        VndDecay => velvet.decay_db,
        VndRefresh => velvet.refresh_seconds,
        VndTransition => velvet.transition_ms,
        VndMode => if velvet.dynamic { 1.0 } else { 0.0 },
        PlatPreDelay => plateau.predelay_ms,
        PlatSize => plateau.size * 100.0,
        PlatDecay => plateau.decay * 100.0,
        PlatLowCut => plateau.low_cut_hz,
        PlatHighCut => plateau.high_cut_hz,
        PlatDiffusion => plateau.diffusion * 100.0,
        PlatModDepth => plateau.mod_depth,
        PlatFreeze => if plateau.freeze { 1.0 } else { 0.0 },
        GlobalCascade => processor.cascade_gain * 100.0,
    };
    ParameterSnapshot { id, value }
}

/// Formats the current DSP value for the L6max OLED notification popup.
pub fn format_parameter(id: EfxParamId, processor: &super::routing::DualEfxProcessor) -> FormattedParam {
    format_snapshot(snapshot_parameter(id, processor))
}
