#[path = "../../../patches/dual-efx/src/math.rs"]
pub mod math;
#[path = "../../../patches/dual-efx/src/velvet.rs"]
pub mod velvet;
#[path = "../../../patches/dual-efx/src/plateau.rs"]
pub mod plateau;
#[path = "../../../patches/dual-efx/src/routing.rs"]
pub mod routing;
#[path = "../../../patches/dual-efx/src/parameters.rs"]
pub mod parameters;

#[cfg(test)]
mod tests {
    use super::*;

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
        zeroed.cascade_gain = 1.0;
        zeroed.return_gain_a = 0.0;
        zeroed.pre_ainr_enabled = true;
        zeroed.init();
        assert_eq!(zeroed.channel_routing[0], routing::RoutingMode::Neither);
        assert_eq!(zeroed.channel_send[0], 0.0);
        assert_eq!(zeroed.cascade_gain, 0.0);
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
    fn test_parameter_formatting() {
        let p_spread = parameters::format_parameter(parameters::EfxParamId::VndTimeSpread, 64);
        assert_eq!(p_spread.title, "VND TIME SPREAD");
        let val_str = std::str::from_utf8(&p_spread.value_str[..p_spread.value_len]).unwrap();
        assert!(val_str.ends_with("ms"), "Got string {val_str}");

        let p_freeze = parameters::format_parameter(parameters::EfxParamId::PlatFreeze, 127);
        assert_eq!(p_freeze.title, "PLATEAU FREEZE");
        let val_str = std::str::from_utf8(&p_freeze.value_str[..p_freeze.value_len]).unwrap();
        assert_eq!(val_str, "FROZEN");
    }
}
