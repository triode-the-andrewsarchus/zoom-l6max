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
