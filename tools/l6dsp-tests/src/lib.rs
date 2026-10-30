#[path = "../../../patches/dual-efx/src/math.rs"]
pub mod math;
#[path = "../../../patches/dual-efx/src/velvet.rs"]
pub mod velvet;
#[path = "../../../patches/dual-efx/src/plateau.rs"]
pub mod plateau;
#[path = "../../../patches/dual-efx/src/routing.rs"]
pub mod routing;
#[path = "../../../patches/dual-efx/src/parameter_view.rs"]
pub mod parameter_view;
#[path = "../../../patches/dual-efx/src/parameters.rs"]
pub mod parameters;
#[path = "../../../patches/dual-efx/src/controls.rs"]
pub mod controls;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn efx_select_defers_short_press_and_distinguishes_hold() {
        use controls::EfxSelectAction as Action;
        let mut gesture = controls::EfxSelectGesture::default();
        assert_eq!(gesture.edge(true, 100, 750), Action::None);
        assert_eq!(gesture.edge(true, 200, 750), Action::None);
        assert_eq!(gesture.edge(false, 249, 750), Action::SelectNext);
        assert_eq!(gesture.edge(false, 250, 750), Action::None);
        assert_eq!(gesture.edge(true, 1000, 750), Action::None);
        assert_eq!(gesture.edge(false, 1750, 750), Action::ToggleInspector);
        assert_eq!(gesture.edge(true, u32::MAX - 99, 750), Action::None);
        assert_eq!(gesture.edge(false, 650, 750), Action::ToggleInspector);
        assert_eq!(gesture.edge(true, 2000, 750), Action::None);
        gesture.cancel();
        assert_eq!(gesture.edge(false, 2100, 750), Action::None);
    }

    #[test]
    fn panel_efx_decoder_requires_complete_matching_frames() {
        let mut decoder = controls::PanelEfxDecoder::default();
        let bytes = [0x91, 4, 6, 0x90, 4, 6, 0x91, 3, 6, 0x91, 4, 0xe1,
                     6, 0x91, 4, 0x90, 4, 6, 4, 6];
        let events: Vec<_> = bytes.into_iter().filter_map(|byte| decoder.feed(byte)).collect();
        assert_eq!(events, [true, false, false]);
    }

    #[test]
    fn inspector_encoders_edit_and_display_actual_parameters() {
        use parameters::{edit_encoder, format_parameter, EfxParamId as Id, ParameterBank as Bank};
        let mut processor = Box::new(routing::DualEfxProcessor::new());
        processor.init();
        let banks = [
            (Bank::Velvet, [Id::VndAmount, Id::VndTimeSpread, Id::VndDensity,
                Id::VndDecay, Id::VndRefresh, Id::VndMode, Id::VndTransition, Id::GlobalCascade]),
            (Bank::Plateau, [Id::PlatPreDelay, Id::PlatSize, Id::PlatDecay,
                Id::PlatLowCut, Id::PlatHighCut, Id::PlatDiffusion, Id::PlatModDepth, Id::PlatFreeze]),
        ];
        for (bank, ids) in banks {
            for (encoder, id) in ids.into_iter().enumerate() {
                let before = format_parameter(id, &processor);
                let delta = if id == Id::PlatFreeze { 1 } else { -1 };
                assert_eq!(edit_encoder(&mut processor, bank, encoder, delta), Some(id));
                let after = format_parameter(id, &processor);
                assert_ne!(&before.value_str[..before.value_len], &after.value_str[..after.value_len], "{id:?}");
            }
            assert_eq!(edit_encoder(&mut processor, bank, 8, 1), None);
            assert_eq!(edit_encoder(&mut processor, bank, 0, 0), None);
        }
        assert!(!processor.velvet.current.dynamic);
        assert!(processor.plateau.params.freeze);
        edit_encoder(&mut processor, Bank::Plateau, 7, -1);
        assert!(!processor.plateau.params.freeze);
        edit_encoder(&mut processor, Bank::Velvet, 2, i32::MAX);
        assert_eq!(processor.velvet.current.density, 3000.0);
        edit_encoder(&mut processor, Bank::Plateau, 0, i32::MIN);
        assert_eq!(processor.plateau.params.predelay_ms, 0.0);
    }

    #[test]
    fn display_snapshot_does_not_borrow_live_dsp_state() {
        use parameters::{format_parameter, format_snapshot, snapshot_parameter, EfxParamId as Id, ParameterSnapshot};
        let mut processor = Box::new(routing::DualEfxProcessor::new());
        processor.init();
        let ids = [Id::VndAmount, Id::VndTimeSpread, Id::VndDensity, Id::VndDecay,
            Id::VndRefresh, Id::VndMode, Id::VndTransition, Id::PlatPreDelay,
            Id::PlatSize, Id::PlatDecay, Id::PlatLowCut, Id::PlatHighCut,
            Id::PlatDiffusion, Id::PlatModDepth, Id::PlatFreeze, Id::GlobalCascade];
        let snapshots = ids.map(|id| snapshot_parameter(id, &processor));
        let original = ids.map(|id| format_parameter(id, &processor));
        processor.velvet.current.amount = 0.0;
        processor.velvet.current.dynamic = false;
        processor.plateau.params.freeze = true;
        processor.cascade_gain = 0.0;
        for (snapshot, expected) in snapshots.into_iter().zip(original) {
            let copied = ParameterSnapshot::from_words(snapshot.id as u32, snapshot.value.to_bits()).unwrap();
            let actual = format_snapshot(copied);
            assert_eq!(actual.title, expected.title);
            assert_eq!(&actual.value_str[..actual.value_len], &expected.value_str[..expected.value_len]);
        }
        assert!(ParameterSnapshot::from_words(16, 0).is_none());
        assert!(ParameterSnapshot::from_words(Id::VndAmount as u32, f32::NAN.to_bits()).is_none());
        assert!(ParameterSnapshot::from_words(Id::PlatFreeze as u32, 0.5f32.to_bits()).is_none());
    }

    #[test]
    fn routing_display_decodes_only_valid_strips_and_routes() {
        use parameter_view::{DisplaySnapshot, format_display};
        for strip in 0..8 {
            for (mode, expected) in ["OFF", "A", "B", "A -> B"].iter().enumerate() {
                let snapshot = DisplaySnapshot::from_words(16 + strip, (mode as f32).to_bits()).unwrap();
                let formatted = format_display(snapshot);
                assert_eq!(formatted.title, format!("CH {} ROUTE", strip + 1));
                assert_eq!(&formatted.value_str[..formatted.value_len], expected.as_bytes());
            }
        }
        for (id, value) in [(25, 0.0), (24, 3.0), (16, -1.0), (16, 0.5), (16, 4.0), (16, f32::NAN)] {
            assert!(DisplaySnapshot::from_words(id, value.to_bits()).is_none());
        }
        for (raw, expected) in [(0.0f32, "PLATEAU PARAM"), (1.0f32, "ROUTING"), (2.0f32, "VELVET PARAM")] {
            let formatted = format_display(DisplaySnapshot::from_words(24, raw.to_bits()).unwrap());
            assert_eq!(formatted.title, "EFX CONTROLS");
            assert_eq!(&formatted.value_str[..formatted.value_len], expected.as_bytes());
        }
    }

    #[test]
    fn test_math_functions() {
        assert!((math::sin(0.0)).abs() < 1e-4);
        assert!((math::sin(math::PI_2) - 1.0).abs() < 1e-3);
        assert!((math::cos(0.0) - 1.0).abs() < 1e-3);
        assert!((math::cos(math::PI_2)).abs() < 1e-3);

        assert!((math::sqrt(4.0) - 2.0).abs() < 1e-4);
        assert!((math::sqrt(100.0) - 10.0).abs() < 1e-4);

        assert!((math::pow10(0.0) - 1.0).abs() < 1e-3);
        assert!((math::pow10(1.0) - 10.0).abs() < 0.1);
        assert!((math::db_to_gain(0.0) - 1.0).abs() < 1e-3);
        assert!((math::db_to_gain(-20.0) - 0.1).abs() < 0.01);
    }

    #[test]
    fn velvet_uniform_matches_double_precision_reference() {
        let mut actual_state = 0x91a2_b3c4_d5e6_f708;
        let mut reference_state = actual_state;
        for _ in 0..200_000 {
            let actual = velvet::uniform_signed(&mut actual_state);
            let bits = velvet::splitmix64(&mut reference_state) >> 11;
            let expected = (bits as f64 * (1.0 / 9007199254740992.0) * 2.0 - 1.0) as f32;
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
        // Exercise rounding boundaries and both endpoints of the 53-bit domain.
        for exponent in 0..53 {
            let base = 1u64 << exponent;
            for bits in [base - 1, base, base + 1, (1u64 << 53) - base] {
                // Invert SplitMix64 so this case exercises uniform_signed itself.
                let mut state = bits << 11;
                state ^= state >> 31 ^ state >> 62;
                state = state.wrapping_mul(0x3196_42b2_d24d_8ec3);
                state ^= state >> 27 ^ state >> 54;
                state = state.wrapping_mul(0x96de_1b17_3f11_9089);
                state ^= state >> 30 ^ state >> 60;
                state = state.wrapping_sub(0x9e37_79b9_7f4a_7c15);
                let expected = (bits as f64 * (1.0 / 9007199254740992.0) * 2.0 - 1.0) as f32;
                let actual = velvet::uniform_signed(&mut state);
                assert_eq!(actual.to_bits(), expected.to_bits(), "bits={bits}");
            }
        }
    }

    #[test]
    fn sine_range_reduction_terminates_for_extreme_inputs() {
        for x in [f32::MAX, f32::MIN, f32::INFINITY, f32::NEG_INFINITY, f32::NAN] {
            let result = math::sin(x);
            assert!(result.is_finite() && result.abs() <= 1.001, "sin({x}) = {result}");
        }
        for x in [-10.0f32, -3.0, 0.0, 3.0, 10.0] {
            assert!((math::sin(x) - x.sin()).abs() < 0.002);
        }
    }

    #[test]
    fn plateau_lowpass_has_minus_three_db_at_its_cutoff() {
        for cutoff in [20.0_f64, 1000.0, 6500.0, 16000.0] {
            let mut filter = plateau::OnePoleLP::new();
            filter.set_cutoff(cutoff as f32);
            let mut input_energy = 0.0;
            let mut output_energy = 0.0;
            for frame in 0..(48_000 + 16_384) {
                let phase = std::f64::consts::TAU * cutoff * frame as f64 / 48_000.0;
                let input = phase.sin() as f32;
                let output = filter.process(input);
                if frame >= 16_384 {
                    input_energy += (input as f64).powi(2);
                    output_energy += (output as f64).powi(2);
                }
            }
            let gain = (output_energy / input_energy).sqrt();
            assert!((gain - std::f64::consts::FRAC_1_SQRT_2).abs() < 0.015,
                    "cutoff {cutoff} Hz has gain {gain}");
        }
    }

    #[test]
    fn plateau_highpass_has_unity_passband_and_minus_three_db_cutoff() {
        for cutoff in [20.0_f64, 1000.0, 6500.0, 16000.0, 23520.0] {
            let mut filter = plateau::OnePoleHP::new();
            filter.set_cutoff(cutoff as f32);
            let mut input_energy = 0.0;
            let mut output_energy = 0.0;
            for frame in 0..(48_000 + 16_384) {
                let phase = std::f64::consts::TAU * cutoff * frame as f64 / 48_000.0;
                let input = phase.sin() as f32;
                let output = filter.process(input);
                if frame >= 16_384 {
                    input_energy += (input as f64).powi(2);
                    output_energy += (output as f64).powi(2);
                }
            }
            let gain = (output_energy / input_energy).sqrt();
            assert!((gain - std::f64::consts::FRAC_1_SQRT_2).abs() < 0.015,
                    "cutoff {cutoff} Hz has gain {gain}");
            filter.clear();
            for frame in 0..10_000 {
                let input = if frame % 2 == 0 { 1.0 } else { -1.0 };
                let output = filter.process(input);
                if frame == 9_999 {
                    assert!((output.abs() - 1.0).abs() < 0.001,
                            "Nyquist passband at {cutoff} Hz has gain {}", output.abs());
                }
            }
        }
    }

    #[test]
    fn plateau_cutoff_updates_preserve_independent_filter_histories() {
        let mut reverb = Box::new(plateau::PlateauReverb::new());
        let mut lp = [plateau::OnePoleLP::new(), plateau::OnePoleLP::new(), plateau::OnePoleLP::new()];
        let mut hp = [plateau::OnePoleHP::new(), plateau::OnePoleHP::new(), plateau::OnePoleHP::new()];
        for (low, high) in [(80.0, 6500.0), (900.0, 16000.0), (100.0, 1000.0)] {
            let mut params = reverb.params;
            params.low_cut_hz = low;
            params.high_cut_hz = high;
            reverb.set_parameters(params);
            for filter in &mut lp { filter.set_cutoff(high); }
            for filter in &mut hp { filter.set_cutoff(low); }
            for frame in 0..512 {
                let inputs = [if frame % 37 == 0 { 0.5 } else { 0.0 },
                    if frame % 53 == 0 { -0.75 } else { 0.0 },
                    if frame % 71 == 0 { 1.0 } else { 0.0 }];
                let actual_lp = [&mut reverb.in_lp, &mut reverb.l_lp, &mut reverb.r_lp];
                for (i, filter) in actual_lp.into_iter().enumerate() {
                    assert_eq!(filter.process(inputs[i]), lp[i].process(inputs[i]));
                }
                let actual_hp = [&mut reverb.in_hp, &mut reverb.l_hp, &mut reverb.r_hp];
                for (i, filter) in actual_hp.into_iter().enumerate() {
                    assert_eq!(filter.process(inputs[i]), hp[i].process(inputs[i]));
                }
            }
        }
    }

    #[test]
    fn nonfinite_input_does_not_poison_effect_state() {
        let mut actual_v = velvet::VelvetDecorrelator::new();
        let mut expected_v = velvet::VelvetDecorrelator::new();
        actual_v.init(velvet::Parameters::default());
        expected_v.init(velvet::Parameters::default());
        let mut actual_p = Box::new(plateau::PlateauReverb::new());
        let mut expected_p = Box::new(plateau::PlateauReverb::new());
        actual_p.init(plateau::PlateauParameters::default());
        expected_p.init(plateau::PlateauParameters::default());
        for i in 0..12000 {
            let signal = if i == 10 { 0.5 } else { 0.0 };
            let corrupt = if i == 0 { f32::NAN } else if i == 1 { f32::INFINITY } else { signal };
            assert_eq!(actual_v.process_sample(corrupt, corrupt), expected_v.process_sample(signal, signal));
            assert_eq!(actual_p.process_sample(corrupt, corrupt), expected_p.process_sample(signal, signal));
        }
        assert_eq!(math::clamp(f32::NAN, 0.0, 1.0), 0.0);
        assert_eq!(math::soft_clip(f32::NAN), 0.0);
    }

    #[test]
    fn plateau_tank_delays_match_their_configured_ring_lengths() {
        let mut reverb = Box::new(plateau::PlateauReverb::new());
        reverb.init(plateau::PlateauParameters::default());
        reverb.params.freeze = true;
        reverb.l_del1.length = 10;
        reverb.l_del2.length = 10;
        reverb.r_del1.length = 10;
        reverb.r_del2.length = 10;
        reverb.l_sum = 1.0;
        reverb.r_sum = 1.0;

        // With empty allpasses the first tank allpass produces an immediate
        // 0.7 impulse, then remains silent past these short test rings.
        for frame in 0..=20 {
            reverb.process_sample(0.0, 0.0);
            let l = reverb.l_apf2.delay.buffer[frame];
            let r = reverb.r_apf2.delay.buffer[frame];
            let expected = if frame == 10 { 0.7 } else { 0.0 };
            assert!((l - expected).abs() < 1e-6, "left first tank delay frame {frame}: {l}");
            assert!((r - expected).abs() < 1e-6, "right first tank delay frame {frame}: {r}");
            let feedback = if frame == 20 { -0.35 } else { 0.0 };
            assert!((reverb.l_sum - feedback).abs() < 1e-6, "left cross-feedback frame {frame}: {}", reverb.l_sum);
            assert!((reverb.r_sum - feedback).abs() < 1e-6, "right cross-feedback frame {frame}: {}", reverb.r_sum);
        }
    }

    #[test]
    fn extreme_finite_input_does_not_poison_effect_state() {
        let mut actual_v = velvet::VelvetDecorrelator::new();
        let mut expected_v = velvet::VelvetDecorrelator::new();
        actual_v.init(velvet::Parameters::default());
        expected_v.init(velvet::Parameters::default());
        let mut actual_p = Box::new(plateau::PlateauReverb::new());
        let mut expected_p = Box::new(plateau::PlateauReverb::new());
        actual_p.init(plateau::PlateauParameters::default());
        expected_p.init(plateau::PlateauParameters::default());
        for i in 0..12000 {
            let signal = if i == 10 { 0.5 } else { 0.0 };
            let corrupt = if i == 0 { f32::MAX } else if i == 1 { f32::MIN } else { signal };
            assert_eq!(actual_v.process_sample(corrupt, corrupt), expected_v.process_sample(signal, signal), "velvet frame {i}");
            assert_eq!(actual_p.process_sample(corrupt, corrupt), expected_p.process_sample(signal, signal), "plateau frame {i}");
        }
    }

    #[test]
    fn velvet_wraps_out_of_range_tap_delays() {
        let mut engine = Box::new(velvet::VelvetDecorrelator::new());
        engine.init(velvet::Parameters::default());
        engine.current.dynamic = false;
        for delay in [1, velvet::MAX_DELAY_SAMPLES as u32 + 1,
            2 * velvet::MAX_DELAY_SAMPLES as u32 + 1, u32::MAX] {
            engine.reset();
            let d = delay as usize % velvet::MAX_DELAY_SAMPLES;
            let bank = &mut engine.channels[0].banks[engine.active_bank];
            bank.count = 1;
            bank.storage[0] = velvet::Tap { delay, gain: 0.5 };
            engine.process_sample(0.25, 0.0);
            for _ in 1..d { engine.process_sample(0.0, 0.0); }
            let (left, _) = engine.process_sample(0.0, 0.0);
            assert_eq!(left.to_bits(), math::soft_clip(0.125).to_bits(), "delay={delay}");
        }
    }

    #[test]
    fn velvet_keeps_the_last_delayed_sample_through_silence() {
        let mut engine = Box::new(velvet::VelvetDecorrelator::new());
        engine.init(velvet::Parameters::default());
        engine.current.dynamic = false;
        for channel in &mut engine.channels {
            let bank = &mut channel.banks[engine.active_bank];
            bank.count = 1;
            bank.storage[0] = velvet::Tap { delay: (velvet::MAX_DELAY_SAMPLES - 1) as u32, gain: 1.0 };
        }
        assert_eq!(engine.process_sample(0.5, -0.5), (0.0, 0.0));
        for _ in 1..velvet::MAX_DELAY_SAMPLES - 1 {
            assert_eq!(engine.process_sample(0.0, 0.0), (0.0, 0.0));
        }
        let (left, right) = engine.process_sample(0.0, 0.0);
        assert_eq!(left.to_bits(), math::soft_clip(0.5).to_bits());
        assert_eq!(right.to_bits(), math::soft_clip(-0.5).to_bits());
        assert_eq!(engine.process_sample(0.0, 0.0), (0.0, 0.0));
    }

    #[test]
    fn velvet_advances_dynamic_filters_during_silence_and_resumes() {
        let mut engine = Box::new(velvet::VelvetDecorrelator::new());
        let mut params = velvet::Parameters::default();
        params.refresh_seconds = 0.1;
        params.transition_ms = 1.0;
        engine.init(params);
        for _ in 0..6_000 { assert_eq!(engine.process_sample(0.0, 0.0), (0.0, 0.0)); }
        assert!(engine.generation > 2);
        let mut peak = 0.0f32;
        for i in 0..velvet::MAX_DELAY_SAMPLES {
            let (left, right) = engine.process_sample(if i == 0 { 0.5 } else { 0.0 }, 0.0);
            peak = peak.max(left.abs()).max(right.abs());
        }
        assert!(peak > 0.0);
    }

    #[test]
    fn test_velvet_noise_decorrelator() {
        let mut vnd = velvet::VelvetDecorrelator::new();
        vnd.init(velvet::Parameters::default());

        // Feed an impulse into Left and Right
        let mut out_l = Vec::new();
        let mut out_r = Vec::new();

        // 1 sample impulse, then silence
        let (y_l, y_r) = vnd.process_sample(1.0, 1.0);
        out_l.push(y_l);
        out_r.push(y_r);

        for _ in 1..2000 {
            let (l, r) = vnd.process_sample(0.0, 0.0);
            out_l.push(l);
            out_r.push(r);
        }

        // Calculate energy of wet impulse response (sum of squares)
        let energy_l: f32 = out_l.iter().map(|&x| x * x).sum();
        let energy_r: f32 = out_r.iter().map(|&x| x * x).sum();

        // Unit energy should be close to 1.0 (with dry = 0, amount = 1.0)
        assert!(energy_l > 0.8 && energy_l < 1.2, "Energy L was {energy_l}");
        assert!(energy_r > 0.8 && energy_r < 1.2, "Energy R was {energy_r}");

        // Cross-correlation between Left and Right should be very low (decorrelated)
        let dot_product: f32 = out_l.iter().zip(out_r.iter()).map(|(&l, &r)| l * r).sum();
        let normalized_corr = dot_product / (energy_l * energy_r).sqrt();
        assert!(
            normalized_corr.abs() < 0.35,
            "Cross-correlation too high: {normalized_corr}"
        );
    }

    #[test]
    fn test_velvet_dynamic_crossfade() {
        let mut vnd = velvet::VelvetDecorrelator::new();
        let mut params = velvet::Parameters::default();
        params.refresh_seconds = 0.05; // 50 ms fast refresh for test
        params.transition_ms = 10.0;    // 10 ms transition
        vnd.init(params);

        // Run audio through multiple renewals and ensure no NaN or blowups
        for i in 0..10000 {
            let sig = ((i as f32) * 0.05).sin();
            let (l, r) = vnd.process_sample(sig, sig);
            assert!(l.is_finite(), "Sample L was not finite at {i}");
            assert!(r.is_finite(), "Sample R was not finite at {i}");
            assert!(l.abs() < 5.0, "Sample L blew up: {l}");
            assert!(r.abs() < 5.0, "Sample R blew up: {r}");
        }
    }

    #[test]
    fn velvet_gain_and_timer_edits_preserve_the_active_filter() {
        let mut reference = velvet::VelvetDecorrelator::new();
        let mut edited = velvet::VelvetDecorrelator::new();
        let mut params = velvet::Parameters::default();
        params.refresh_seconds = 0.1;
        params.transition_ms = 10.0;
        reference.init(params);
        edited.init(params);
        for i in 0..15000 {
            let signal = (i as f32 * 0.1).sin() * 0.1;
            reference.process_sample(signal, signal);
            edited.process_sample(signal, signal);
        }
        params.amount -= 0.000001;
        params.refresh_seconds = 0.2;
        edited.set_parameters(params);
        for i in 15000..16000 {
            let signal = (i as f32 * 0.1).sin() * 0.1;
            let (a_l, a_r) = reference.process_sample(signal, signal);
            let (b_l, b_r) = edited.process_sample(signal, signal);
            assert!((a_l - b_l).abs() < 0.00001);
            assert!((a_r - b_r).abs() < 0.00001);
        }
    }

    #[test]
    fn velvet_filter_edits_crossfade_even_when_dynamic_is_disabled() {
        let mut edited = velvet::VelvetDecorrelator::new();
        let mut reference = velvet::VelvetDecorrelator::new();
        let mut params = velvet::Parameters::default();
        params.dynamic = false;
        params.transition_ms = 10.0;
        edited.init(params);
        reference.init(params);
        for i in 0..2000 {
            let signal = (i as f32 * 0.1).sin() * 0.1;
            edited.process_sample(signal, signal);
            reference.process_sample(signal, signal);
        }
        params.density = 2000.0;
        edited.set_parameters(params);
        let signal = (2000.0f32 * 0.1).sin() * 0.1;
        let first = edited.process_sample(signal, signal);
        let unchanged = reference.process_sample(signal, signal);
        assert!((first.0 - unchanged.0).abs() < 0.00001);
        assert!((first.1 - unchanged.1).abs() < 0.00001);

        // A second edit during the fade must also finish, even in static mode.
        params.density = 500.0;
        edited.set_parameters(params);
        for i in 2001..4000 {
            let signal = (i as f32 * 0.1).sin() * 0.1;
            let (left, right) = edited.process_sample(signal, signal);
            assert!(left.is_finite() && right.is_finite());
        }
        assert!(!edited.transitioning);
        assert!(!edited.filter_update_pending);
        assert_eq!(edited.channels[0].banks[edited.active_bank].count, 14);
    }

    #[test]
    fn velvet_reset_applies_latest_filter_after_edits() {
        for settle in [false, true] {
            let mut edited = velvet::VelvetDecorrelator::new();
            let mut reference = velvet::VelvetDecorrelator::new();
            let mut params = velvet::Parameters::default();
            params.dynamic = false;
            params.transition_ms = 1.0;
            edited.init(params);
            params.density = 500.0;
            edited.set_parameters(params);
            if settle {
                for _ in 0..100 { edited.process_sample(0.0, 0.0); }
            }
            edited.reset();
            reference.init(params);
            let mut peak = 0.0f32;
            for i in 0..2000 {
                let input = if i == 0 { 0.5 } else { 0.0 };
                let actual = edited.process_sample(input, input);
                let expected = reference.process_sample(input, input);
                peak = peak.max(expected.0.abs()).max(expected.1.abs());
                assert_eq!(actual, expected, "settled={settle}, frame={i}");
            }
            assert!(peak > 0.001);
        }
    }

    #[test]
    fn velvet_builds_pending_filter_without_exposing_partial_taps() {
        let mut edited = velvet::VelvetDecorrelator::new();
        let mut reference = velvet::VelvetDecorrelator::new();
        let mut params = velvet::Parameters::default();
        params.dynamic = false;
        params.transition_ms = 1.0;
        edited.init(params);
        reference.init(params);
        params.length_ms = 95.0;
        params.density = 3000.0;
        edited.set_parameters(params);

        // A maximum-size bank needs time to prepare. Until it is ready the
        // old filter must produce the same output as an unedited instance.
        for i in 0..100 {
            let before = [edited.channels[0].banks[edited.pending_bank],
                          edited.channels[1].banks[edited.pending_bank]];
            let input = (i as f32 * 0.13).sin() * 0.1;
            assert_eq!(edited.process_sample(input, input), reference.process_sample(input, input));
            assert!(!edited.transitioning);
            for c in 0..2 {
                let after = &edited.channels[c].banks[edited.pending_bank];
                let changed = before[c].storage.iter().zip(after.storage.iter())
                    .filter(|(a, b)| a.delay != b.delay || a.gain.to_bits() != b.gain.to_bits())
                    .count();
                assert!(changed <= 1, "frame {i}, channel {c}: {changed} taps changed");
            }
        }

        // A newer edit replaces a bank still being built.
        params.density = 200.0;
        edited.set_parameters(params);
        for _ in 0..500 { edited.process_sample(0.0, 0.0); }
        assert!(!edited.transitioning);
        assert!(!edited.filter_update_pending);
        assert_eq!(edited.channels[0].banks[edited.active_bank].count, 19);
        assert_eq!(edited.channels[1].banks[edited.active_bank].count, 19);
    }

    #[test]
    fn plateau_predelay_delays_impulse_by_requested_samples() {
        let mut immediate = plateau::PlateauReverb::new();
        let mut delayed = plateau::PlateauReverb::new();
        let mut params = plateau::PlateauParameters::default();
        params.predelay_ms = 0.0;
        params.mod_depth = 0.0;
        immediate.init(params);
        params.predelay_ms = 1.0;
        delayed.init(params);

        let mut reference = Vec::new();
        let mut peak = 0.0f32;
        for i in 0..12000 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            let (left, right) = immediate.process_sample(input, input);
            reference.push((left, right));
            let (actual_l, actual_r) = delayed.process_sample(input, input);
            let (expected_l, expected_r) = if i < 48 { (0.0, 0.0) } else { reference[i - 48] };
            assert!((actual_l - expected_l).abs() < 1e-5, "left frame {i}");
            assert!((actual_r - expected_r).abs() < 1e-5, "right frame {i}");
            peak = peak.max(left.abs()).max(right.abs());
        }
        assert!(peak > 0.001, "test must observe a reverb response");
    }

    #[test]
    fn shrinking_plateau_size_keeps_new_samples_in_the_delay_ring() {
        let mut reverb = Box::new(plateau::PlateauReverb::new());
        reverb.init(plateau::PlateauParameters::default());
        // These are valid positions just before each old ring wraps.
        reverb.l_del1.write_pos = reverb.l_del1.length - 1;
        reverb.l_del2.write_pos = reverb.l_del2.length - 1;
        reverb.r_del1.write_pos = reverb.r_del1.length - 1;
        reverb.r_del2.write_pos = reverb.r_del2.length - 1;
        let mut params = reverb.params;
        params.size = 0.2;
        reverb.set_parameters(params);
        reverb.l_del1.write(0.75);
        reverb.l_del2.write(0.75);
        reverb.r_del1.write(0.75);
        reverb.r_del2.write(0.75);
        assert_eq!(reverb.l_del1.read(1), 0.75);
        assert_eq!(reverb.l_del2.read(1), 0.75);
        assert_eq!(reverb.r_del1.read(1), 0.75);
        assert_eq!(reverb.r_del2.read(1), 0.75);
    }

    #[test]
    fn test_plateau_reverb_decay() {
        let mut plat = plateau::PlateauReverb::new();
        plat.init(plateau::PlateauParameters::default());

        // Feed impulse
        let (y_l, y_r) = plat.process_sample(1.0, 1.0);
        let mut max_tail = y_l.abs().max(y_r.abs());

        // Process 1 second of tail
        for _ in 1..48000 {
            let (l, r) = plat.process_sample(0.0, 0.0);
            assert!(l.is_finite(), "Reverb L not finite");
            assert!(r.is_finite(), "Reverb R not finite");
            max_tail = max_tail.max(l.abs()).max(r.abs());
        }

        assert!(max_tail > 0.01, "Reverb did not produce output");

        // After 2 seconds, tail should have decayed
        let (final_l, final_r) = plat.process_sample(0.0, 0.0);
        assert!(final_l.abs() < 0.01, "Reverb did not decay: {final_l}");
        assert!(final_r.abs() < 0.01, "Reverb did not decay: {final_r}");
    }

    #[test]
    fn test_plateau_freeze() {
        let mut plat = plateau::PlateauReverb::new();
        let mut params = plateau::PlateauParameters::default();
        plat.init(params);

        // Feed audio
        for i in 0..1000 {
            let s = ((i as f32) * 0.1).sin();
            plat.process_sample(s, s);
        }

        // Turn on Freeze
        params.freeze = true;
        plat.set_parameters(params);

        // Let it run for 20,000 samples in silence
        let mut mid_energy = 0.0f32;
        for _ in 0..20000 {
            let (l, r) = plat.process_sample(0.0, 0.0);
            mid_energy += l * l + r * r;
        }

        // Energy should remain strong because it's frozen
        assert!(mid_energy > 1.0, "Frozen reverb lost energy: {mid_energy}");
    }

    #[test]
    fn processor_initialization_restores_zeroed_and_dirty_state() {
        let mut reference = Box::new(routing::DualEfxProcessor::new());
        let mut storage = Box::<routing::DualEfxProcessor>::new_uninit();
        // Model dirty raw storage without first creating a reference to the
        // processor or relying on its normal constructor.
        unsafe { std::ptr::write_bytes(storage.as_mut_ptr().cast::<u8>(), 0xa5,
            std::mem::size_of::<routing::DualEfxProcessor>()) };
        let zeroed = routing::DualEfxProcessor::initialize_in_place(&mut storage);
        reference.init();
        assert_eq!(zeroed.return_gain_a, 1.0);
        assert_eq!(zeroed.return_gain_b, 1.0);
        assert!(zeroed.plateau.lfo1.phase_inc > 0.0);
        assert!(zeroed.plateau.lfo2.phase_inc > 0.0);

        // Compare both effects after initialization, including more than one
        // full delay-buffer rotation, and verify an audible response exists.
        let mut plateau_peak = 0.0f32;
        let mut velvet_peak = 0.0f32;
        for i in 0..12000 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            let actual = zeroed.plateau.process_sample(input, input);
            let expected = reference.plateau.process_sample(input, input);
            assert_eq!(actual, expected, "plateau frame {i}");
            plateau_peak = plateau_peak.max(actual.0.abs()).max(actual.1.abs());
            let actual = zeroed.velvet.process_sample(input, input);
            let expected = reference.velvet.process_sample(input, input);
            assert_eq!(actual, expected, "velvet frame {i}");
            velvet_peak = velvet_peak.max(actual.0.abs()).max(actual.1.abs());
        }
        assert!(plateau_peak > 0.001 && velvet_peak > 0.001);

        zeroed.channel_routing[0] = routing::RoutingMode::SendA;
        zeroed.channel_send[0] = 0.7;
        zeroed.cascade_gain = 0.0;
        zeroed.return_gain_a = 0.0;
        zeroed.pre_ainr_enabled = true;
        zeroed.init();
        assert_eq!(zeroed.channel_routing[0], routing::RoutingMode::Neither);
        assert_eq!(zeroed.channel_send[0], 0.0);
        assert_eq!(zeroed.cascade_gain, 1.0);
        assert_eq!(zeroed.return_gain_a, 1.0);
        assert!(!zeroed.pre_ainr_enabled);
    }

    #[test]
    fn cascade_receives_only_channels_routed_through_it() {
        let mut isolated = Box::new(routing::DualEfxProcessor::new());
        let mut mixed = Box::new(routing::DualEfxProcessor::new());
        isolated.init();
        mixed.init();
        for processor in [&mut isolated, &mut mixed] {
            processor.cascade_gain = 1.0;
            processor.return_gain_a = 0.0;
            processor.set_channel_routing(0, routing::RoutingMode::SendAtoB);
            processor.set_channel_send(0, 1.0);
        }
        mixed.set_channel_routing(1, routing::RoutingMode::SendA);
        mixed.set_channel_send(1, 1.0);

        let mut peak = 0.0f32;
        for block in 0..300 {
            let mut cascade = [0.0f32; 64];
            let mut a_only = [0.0f32; 64];
            if block == 0 {
                cascade[0] = 1.0;
                a_only[0] = -1.0;
            }
            let mut expected_l = [0.0; 64];
            let mut expected_r = [0.0; 64];
            let mut actual_l = [0.0; 64];
            let mut actual_r = [0.0; 64];
            isolated.process_block(&[&cascade], &mut expected_l, &mut expected_r);
            mixed.process_block(&[&cascade, &a_only], &mut actual_l, &mut actual_r);
            for i in 0..64 {
                assert!((actual_l[i] - expected_l[i]).abs() < 1e-6);
                assert!((actual_r[i] - expected_r[i]).abs() < 1e-6);
                peak = peak.max(expected_l[i].abs()).max(expected_r[i].abs());
            }
        }
        assert!(peak > 0.001, "cascade must produce a reverb response");
    }

    #[test]
    fn routing_processes_long_buffers_like_successive_blocks() {
        let mut long = Box::new(routing::DualEfxProcessor::new());
        let mut split = Box::new(routing::DualEfxProcessor::new());
        for processor in [&mut long, &mut split] {
            processor.init();
            processor.set_channel_routing(0, routing::RoutingMode::SendA);
            processor.set_channel_routing(1, routing::RoutingMode::SendB);
            processor.set_channel_send(0, 1.0);
            processor.set_channel_send(1, 0.5);
        }
        let input: Vec<_> = (0..145).map(|i| (i as f32 * 0.1).sin() * 0.2).collect();
        // The second channel ends partway through the second block.
        let short = &input[..80];
        let mut actual_l = vec![42.0; 145];
        let mut actual_r = vec![42.0; 145];
        let mut expected_l = vec![42.0; 145];
        let mut expected_r = vec![42.0; 145];
        long.process_block(&[&input, short], &mut actual_l, &mut actual_r);
        for start in (0..145).step_by(64) {
            let end = (start + 64).min(145);
            let short_chunk = short.get(start..end.min(short.len())).unwrap_or(&[]);
            split.process_block(&[&input[start..end], short_chunk],
                &mut expected_l[start..end], &mut expected_r[start..end]);
        }
        assert_eq!(actual_l, expected_l);
        assert_eq!(actual_r, expected_r);
        assert!(actual_l.iter().all(|&x| x != 42.0));
    }

    #[test]
    fn preweighted_buses_match_channel_routing_and_preserve_tails() {
        let mut channels = Box::new(routing::DualEfxProcessor::new());
        let mut buses = Box::new(routing::DualEfxProcessor::new());
        channels.init();
        buses.init();
        channels.set_channel_routing(0, routing::RoutingMode::SendA);
        channels.set_channel_routing(1, routing::RoutingMode::SendB);
        channels.set_channel_routing(2, routing::RoutingMode::SendAtoB);
        for ch in 0..3 {
            channels.set_channel_send(ch, 0.5);
        }
        let mut inputs = [[0.0; 64]; 3];
        inputs[0][0] = 0.25;
        inputs[1][1] = 0.5;
        inputs[2][2] = -0.75;
        let mut peak = 0.0f32;
        for _ in 0..200 {
            let weighted = inputs.map(|channel| channel.map(|sample| sample * 0.5));
            let mut expected = [[0.0; 64]; 2];
            let mut actual = [[0.0; 64]; 2];
            let [ref mut expected_l, ref mut expected_r] = expected;
            channels.process_block(&[&inputs[0], &inputs[1], &inputs[2]], expected_l, expected_r);
            let [ref mut actual_l, ref mut actual_r] = actual;
            buses.process_buses(&weighted[0], &weighted[1], &weighted[2], actual_l, actual_r);
            assert_eq!(actual, expected);
            for (&l, &r) in actual[0].iter().zip(&actual[1]) {
                peak = peak.max(l.abs()).max(r.abs());
            }
            inputs.fill([0.0; 64]);
        }
        assert!(peak > 0.001, "the test must observe an effect response");
    }

    #[test]
    fn routed_frame_processing_matches_block_processing() {
        let mut block = Box::new(routing::DualEfxProcessor::new());
        let mut frames = Box::new(routing::DualEfxProcessor::new());
        block.init();
        frames.init();
        let mut peak = 0.0f32;
        for n in 0..150 {
            if n == 25 {
                let mut p = block.velvet.current;
                p.density = 2000.0;
                block.velvet.set_parameters(p);
                frames.velvet.set_parameters(p);
            }
            let a = [if n == 0 { 0.25 } else { 0.0 }; 64];
            let b = [if n == 1 { 0.125 } else { 0.0 }; 64];
            let c = [if n == 2 { -0.25 } else { 0.0 }; 64];
            let mut left = [0.0; 64];
            let mut right = [0.0; 64];
            block.process_buses(&a, &b, &c, &mut left, &mut right);
            frames.prepare_bus_block();
            for i in 0..64 {
                let actual = frames.process_bus_sample(a[i], b[i], c[i]);
                assert_eq!(actual, (left[i], right[i]), "block {n}, frame {i}");
                peak = peak.max(actual.0.abs()).max(actual.1.abs());
            }
        }
        assert!(peak > 0.001);
    }

    #[test]
    fn weighted_channels_route_signed_gains_and_per_frame_ramps() {
        let mut routed = Box::new(routing::DualEfxProcessor::new());
        let mut reference = Box::new(routing::DualEfxProcessor::new());
        routed.init();
        reference.init();
        routed.set_channel_routing(0, routing::RoutingMode::SendA);
        routed.set_channel_routing(1, routing::RoutingMode::SendB);
        routed.set_channel_routing(2, routing::RoutingMode::SendAtoB);
        let a: Vec<_> = (0..145).map(|i| if i == 0 || i == 70 { 0.25 } else { 0.0 }).collect();
        let b: Vec<_> = (0..145).map(|i| if i == 1 || i == 71 || i == 110 { 0.5 } else { 0.0 }).collect();
        let c: Vec<_> = (0..145).map(|i| if i == 2 || i == 72 { -0.25 } else { 0.0 }).collect();
        let ramp: Vec<_> = (0..100).map(|i| i as f32 / 100.0).collect();
        let channels = [
            routing::WeightedChannel { samples: &a, gain: 2.0, ramp: None },
            routing::WeightedChannel { samples: &b, gain: 0.75, ramp: Some(&ramp) },
            routing::WeightedChannel { samples: &c, gain: -1.0, ramp: None },
        ];
        let expected_a: Vec<_> = a.iter().map(|x| x * 2.0).collect();
        let expected_b: Vec<_> = b.iter().enumerate().map(|(i, x)| x * ramp.get(i).copied().unwrap_or(0.0) * 0.75).collect();
        let expected_c: Vec<_> = c.iter().map(|x| -x).collect();
        let mut actual = [vec![0.0; 145], vec![0.0; 145]];
        let mut expected = [vec![0.0; 145], vec![0.0; 145]];
        let [ref mut left, ref mut right] = actual;
        routed.process_weighted_block(&channels, left, right);
        let [ref mut left, ref mut right] = expected;
        reference.process_buses(&expected_a, &expected_b, &expected_c, left, right);
        assert_eq!(actual, expected);
        assert!(actual.iter().flatten().any(|x| x.abs() > 0.001));
    }

    #[test]
    fn corrupt_weighted_channel_does_not_silence_other_channels() {
        let mut actual_processor = Box::new(routing::DualEfxProcessor::new());
        let mut reference_processor = Box::new(routing::DualEfxProcessor::new());
        actual_processor.init();
        reference_processor.init();
        for processor in [&mut actual_processor, &mut reference_processor] {
            processor.set_channel_routing(0, routing::RoutingMode::SendA);
            processor.set_channel_routing(1, routing::RoutingMode::SendA);
            processor.set_channel_routing(2, routing::RoutingMode::SendA);
        }
        let mut valid = [0.0; 512];
        let mut corrupt = [0.0; 512];
        let mut ramp_input = [0.0; 512];
        let mut ramp = [1.0; 512];
        for i in [0, 64, 128] { valid[i] = 0.5; }
        corrupt[0] = f32::NAN;
        corrupt[64] = f32::INFINITY;
        ramp_input[128] = 0.25;
        ramp[128] = f32::NAN;
        let channels = [
            routing::WeightedChannel { samples: &valid, gain: 1.0, ramp: None },
            routing::WeightedChannel { samples: &corrupt, gain: 1.0, ramp: None },
            routing::WeightedChannel { samples: &ramp_input, gain: 1.0, ramp: Some(&ramp) },
        ];
        let mut actual = [[0.0; 512]; 2];
        let mut reference = [[0.0; 512]; 2];
        let [left, right] = &mut actual;
        actual_processor.process_weighted_block(&channels, left, right);
        let [left, right] = &mut reference;
        reference_processor.process_weighted_block(&channels[..1], left, right);
        assert_eq!(actual, reference);
        assert!(reference.iter().flatten().any(|sample| sample.abs() > 0.001));
    }

    #[test]
    fn strip_routing_updates_both_sides_of_paired_strips() {
        let mut processor = Box::new(routing::DualEfxProcessor::new());
        let expected: [&[usize]; 8] = [&[0], &[1], &[2], &[3], &[4, 5], &[6, 7], &[8, 9], &[10, 11]];
        for (strip, slots) in expected.iter().enumerate() {
            processor.channel_routing.fill(routing::RoutingMode::Neither);
            processor.set_strip_routing(strip, routing::RoutingMode::SendAtoB);
            for (slot, &mode) in processor.channel_routing.iter().enumerate() {
                assert_eq!(mode == routing::RoutingMode::SendAtoB, slots.contains(&slot));
            }
        }
        let before = processor.channel_routing;
        processor.set_strip_routing(8, routing::RoutingMode::SendA);
        assert_eq!(processor.channel_routing, before);
        assert_eq!(processor.cycle_strip_routing(8, 1), None);
        for (strip, slots) in expected.iter().enumerate() {
            processor.set_strip_routing(strip, routing::RoutingMode::Neither);
            for mode in [routing::RoutingMode::SendA, routing::RoutingMode::SendB,
                         routing::RoutingMode::SendAtoB, routing::RoutingMode::Neither] {
                assert_eq!(processor.cycle_strip_routing(strip, 1), Some(mode));
                for &slot in *slots { assert_eq!(processor.channel_routing[slot], mode); }
            }
            assert_eq!(processor.cycle_strip_routing(strip, -1), Some(routing::RoutingMode::SendAtoB));
            assert_eq!(processor.cycle_strip_routing(strip, i32::MIN), Some(routing::RoutingMode::SendAtoB));
            assert_eq!(processor.cycle_strip_routing(strip, i32::MAX), Some(routing::RoutingMode::SendB));
        }
    }

    #[test]
    fn stereo_send_processes_long_buffers_like_successive_blocks() {
        let mut long = Box::new(routing::DualEfxProcessor::new());
        let mut split = Box::new(routing::DualEfxProcessor::new());
        long.init();
        split.init();
        long.cascade_gain = 0.5;
        split.cascade_gain = 0.5;
        let input: Vec<_> = (0..145).map(|i| (i as f32 * 0.1).sin() * 0.2).collect();
        let mut actual_l = vec![42.0; 145];
        let mut actual_r = vec![42.0; 145];
        let mut expected_l = vec![42.0; 145];
        let mut expected_r = vec![42.0; 145];
        long.process_stereo_send(&input, &input, &mut actual_l, &mut actual_r, true, true);
        for start in (0..145).step_by(64) {
            let end = (start + 64).min(145);
            split.process_stereo_send(&input[start..end], &input[start..end],
                &mut expected_l[start..end], &mut expected_r[start..end], true, true);
        }
        assert_eq!(actual_l, expected_l);
        assert_eq!(actual_r, expected_r);
        assert!(actual_l.iter().all(|&x| x != 42.0));
    }

    #[test]
    fn stereo_send_preserves_quiet_cascade() {
        let mut off = Box::new(routing::DualEfxProcessor::new());
        let mut quiet = Box::new(routing::DualEfxProcessor::new());
        off.init();
        quiet.init();
        off.cascade_gain = 0.0;
        quiet.cascade_gain = 0.0001;
        off.return_gain_a = 0.0;
        quiet.return_gain_a = 0.0;
        let mut input = [0.0; 64];
        input[0] = 1.0;
        let mut peak_difference = 0.0f32;
        for _ in 0..400 {
            let mut off_l = [0.0; 64];
            let mut off_r = [0.0; 64];
            let mut quiet_l = [0.0; 64];
            let mut quiet_r = [0.0; 64];
            off.process_stereo_send(&input, &input, &mut off_l, &mut off_r, true, true);
            quiet.process_stereo_send(&input, &input, &mut quiet_l, &mut quiet_r, true, true);
            for i in 0..64 {
                peak_difference = peak_difference.max((quiet_l[i] - off_l[i]).abs())
                    .max((quiet_r[i] - off_r[i]).abs());
            }
            input[0] = 0.0;
        }
        assert!(peak_difference > 1e-9, "quiet cascade was lost: {peak_difference}");
    }

    #[test]
    fn stereo_send_advances_inactive_cascade_history() {
        let mut switched = Box::new(routing::DualEfxProcessor::new());
        let mut reference = Box::new(routing::DualEfxProcessor::new());
        switched.init();
        reference.init();
        let silence = [0.0; 64];
        let mut impulse = silence;
        impulse[0] = 1.0;
        let mut left = [0.0; 64];
        let mut right = [0.0; 64];
        for p in [&mut switched, &mut reference] {
            p.return_gain_a = 0.0;
            p.process_buses(&silence, &silence, &impulse, &mut left, &mut right);
            p.cascade_gain = 0.0;
        }
        // Silence the cascade while advancing the same elapsed audio time.
        for _ in 0..80 {
            switched.process_stereo_send(&silence, &silence, &mut left, &mut right, false, false);
            reference.process_buses(&silence, &silence, &silence, &mut left, &mut right);
        }
        switched.cascade_gain = 1.0;
        reference.cascade_gain = 1.0;
        for block in 0..100 {
            let mut expected_l = [0.0; 64];
            let mut expected_r = [0.0; 64];
            switched.process_buses(&silence, &silence, &silence, &mut left, &mut right);
            reference.process_buses(&silence, &silence, &silence, &mut expected_l, &mut expected_r);
            assert_eq!(left, expected_l, "left block {block}");
            assert_eq!(right, expected_r, "right block {block}");
        }
    }

    #[test]
    fn reverb_tail_continues_after_send_is_closed() {
        let mut processor = Box::new(routing::DualEfxProcessor::new());
        processor.init();
        processor.set_channel_routing(0, routing::RoutingMode::SendB);
        processor.set_channel_send(0, 1.0);
        let mut impulse = [0.0f32; 64];
        impulse[0] = 1.0;
        let mut left = [0.0; 64];
        let mut right = [0.0; 64];
        processor.process_block(&[&impulse], &mut left, &mut right);
        processor.set_channel_send(0, 0.0);
        let silence = [0.0; 64];
        let mut tail_peak = 0.0f32;
        for _ in 0..300 {
            processor.process_block(&[&silence], &mut left, &mut right);
            for (&l, &r) in left.iter().zip(&right) {
                assert!(l.is_finite() && r.is_finite());
                tail_peak = tail_peak.max(l.abs()).max(r.abs());
            }
        }
        assert!(tail_peak > 0.001, "closing a send must not cut the reverb tail");
    }

    #[test]
    fn quiet_positive_send_is_not_silenced() {
        let mut processor = Box::new(routing::DualEfxProcessor::new());
        processor.init();
        processor.set_channel_routing(0, routing::RoutingMode::SendB);
        processor.set_channel_send(0, 0.0001);
        let mut input = [0.0f32; 64];
        input[0] = 1.0;
        let mut left = [0.0; 64];
        let mut right = [0.0; 64];
        let mut peak = 0.0f32;
        for _ in 0..200 {
            processor.process_block(&[&input], &mut left, &mut right);
            for (&l, &r) in left.iter().zip(&right) {
                peak = peak.max(l.abs()).max(r.abs());
            }
            input[0] = 0.0;
        }
        assert!(peak > 0.0000001 && peak < 0.001, "quiet send peak was {peak}");
    }

    #[test]
    fn cascade_route_feeds_reverb_with_default_settings() {
        let mut processor = Box::new(routing::DualEfxProcessor::new());
        processor.init();
        processor.set_channel_routing(0, routing::RoutingMode::SendAtoB);
        processor.set_channel_send(0, 1.0);
        // Isolate the reverb return from the direct decorrelator return.
        processor.return_gain_a = 0.0;
        let mut input = [0.0f32; 64];
        input[0] = 1.0;
        let mut left = [0.0; 64];
        let mut right = [0.0; 64];
        let mut peak = 0.0f32;
        for _ in 0..200 {
            processor.process_block(&[&input], &mut left, &mut right);
            for (&l, &r) in left.iter().zip(&right) {
                peak = peak.max(l.abs()).max(r.abs());
            }
            input[0] = 0.0;
        }
        assert!(peak > 0.001, "cascade produced no reverb response: {peak}");
    }

    #[test]
    fn test_dual_routing_matrix() {
        let mut dual = routing::DualEfxProcessor::new();
        dual.init();

        // Configure channel 0: SendA (VND)
        dual.set_channel_routing(0, routing::RoutingMode::SendA);
        dual.set_channel_send(0, 1.0);

        // Configure channel 1: SendB (Plateau)
        dual.set_channel_routing(1, routing::RoutingMode::SendB);
        dual.set_channel_send(1, 1.0);

        // Configure channel 2: SendAtoB (Cascade)
        dual.set_channel_routing(2, routing::RoutingMode::SendAtoB);
        dual.set_channel_send(2, 1.0);

        let ch0_data = [1.0f32; 32];
        let ch1_data = [0.5f32; 32];
        let ch2_data = [0.25f32; 32];
        let channels: [&[f32]; 3] = [&ch0_data, &ch1_data, &ch2_data];

        let mut out_l = [0.0f32; 32];
        let mut out_r = [0.0f32; 32];

        dual.process_block(&channels, &mut out_l, &mut out_r);

        for i in 0..32 {
            assert!(out_l[i].is_finite());
            assert!(out_r[i].is_finite());
        }
    }

    #[test]
    fn stereo_disabled_effects_advance_without_accepting_input() {
        for (enable_a, enable_b) in [(true, false), (false, true), (true, true)] {
            let mut disabled = Box::new(routing::DualEfxProcessor::new());
            let mut reference = Box::new(routing::DualEfxProcessor::new());
            disabled.init();
            reference.init();
            disabled.cascade_gain = 0.0;
            reference.cascade_gain = 0.0;
            let silence = [0.0; 64];
            let mut impulse = silence;
            impulse[0] = 1.0;
            let mut left = silence;
            let mut right = silence;
            let mut expected_left = silence;
            let mut expected_right = silence;
            disabled.process_stereo_send(&impulse, &impulse, &mut left, &mut right, enable_a, enable_b);
            reference.process_stereo_send(&impulse, &impulse, &mut expected_left, &mut expected_right, enable_a, enable_b);
            // The disabled effects must receive silence even when the bus carries
            // signal, and their output must remain muted while their history ages.
            for _ in 0..4 {
                disabled.process_stereo_send(&impulse, &impulse, &mut left, &mut right, false, false);
                reference.process_stereo_send(&silence, &silence, &mut expected_left, &mut expected_right, enable_a, enable_b);
                assert!(left.iter().chain(&right).all(|&sample| sample == 0.0));
            }
            let mut peak = 0.0f32;
            for _ in 0..200 {
                disabled.process_stereo_send(&silence, &silence, &mut left, &mut right, enable_a, enable_b);
                reference.process_stereo_send(&silence, &silence, &mut expected_left, &mut expected_right, enable_a, enable_b);
                assert_eq!(left, expected_left);
                assert_eq!(right, expected_right);
                for &sample in left.iter().chain(&right) { peak = peak.max(sample.abs()); }
            }
            assert!(peak > 0.0001, "comparison must include a reverb tail");
        }
    }

    #[test]
    fn test_parameter_formatting() {
        use parameters::EfxParamId as Id;
        let mut processor = Box::new(routing::DualEfxProcessor::new());
        processor.init();
        let mut velvet = processor.velvet.current;
        velvet.length_ms = 999.0;
        velvet.decay_db = -0.5;
        velvet.transition_ms = 17.5;
        velvet.dynamic = false;
        processor.velvet.set_parameters(velvet);
        let mut plateau = processor.plateau.params;
        plateau.predelay_ms = 999.0;
        plateau.size = 1.2;
        plateau.decay = 0.91;
        plateau.low_cut_hz = 920.0;
        plateau.high_cut_hz = 12450.0;
        plateau.mod_depth = 11.5;
        plateau.freeze = true;
        processor.plateau.set_parameters(plateau);
        for (id, expected) in [
            (Id::VndAmount, "100.0 %"),
            (Id::VndTimeSpread, "95.0 ms"),
            (Id::VndDensity, "1088.4 pps"),
            (Id::VndDecay, "-0.5 dB"),
            (Id::VndRefresh, "5.0 s"),
            (Id::VndTransition, "17.5 ms"),
            (Id::VndMode, "STATIC"),
            (Id::PlatPreDelay, "190.0 ms"),
            (Id::PlatSize, "120.0 %"),
            (Id::PlatDecay, "91.0 %"),
            (Id::PlatLowCut, "920.0 Hz"),
            (Id::PlatHighCut, "12450.0 Hz"),
            (Id::PlatDiffusion, "75.0 %"),
            (Id::PlatModDepth, "11.5 samples"),
            (Id::PlatFreeze, "FROZEN"),
            (Id::GlobalCascade, "100.0 %"),
        ] {
            let formatted = parameters::format_parameter(id, &processor);
            let value = std::str::from_utf8(&formatted.value_str[..formatted.value_len]).unwrap();
            assert_eq!(value, expected, "{id:?}");
        }
    }

    #[test]
    fn plateau_reports_the_cutoffs_its_filters_apply() {
        let mut processor = Box::new(routing::DualEfxProcessor::new());
        processor.init();
        let mut params = processor.plateau.params;
        params.low_cut_hz = f32::NAN;
        params.high_cut_hz = f32::INFINITY;
        processor.plateau.set_parameters(params);
        assert_eq!(processor.plateau.params.low_cut_hz, 20.0);
        assert_eq!(processor.plateau.params.high_cut_hz, plateau::SAMPLE_RATE * 0.49);
        let formatted = parameters::format_parameter(parameters::EfxParamId::PlatLowCut, &processor);
        assert_eq!(std::str::from_utf8(&formatted.value_str[..formatted.value_len]).unwrap(), "20.0 Hz");
        let formatted = parameters::format_parameter(parameters::EfxParamId::PlatHighCut, &processor);
        assert_eq!(std::str::from_utf8(&formatted.value_str[..formatted.value_len]).unwrap(), "23520.0 Hz");
    }
}
