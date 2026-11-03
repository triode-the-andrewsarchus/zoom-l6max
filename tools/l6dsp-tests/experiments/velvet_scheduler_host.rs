#![allow(dead_code)]
mod math;
mod original;
mod velvet;
#[cfg(test)]
mod cross_ring_tests {
    use super::{original, velvet};
    #[test]
    fn expanded_ring_matches_original_with_edits_and_wraps() {
        let mut a = Box::new(original::VelvetDecorrelator::new());
        let mut b = Box::new(velvet::VelvetDecorrelator::new());
        let mut pa = original::Parameters::default();
        pa.length_ms = 100.0;
        pa.density = 4000.0;
        pa.refresh_seconds = 0.1;
        pa.transition_ms = 90.0;
        let mut pb = velvet::Parameters::default();
        pb.length_ms = pa.length_ms;
        pb.density = pa.density;
        pb.refresh_seconds = pa.refresh_seconds;
        pb.transition_ms = pa.transition_ms;
        a.init(pa);
        b.init(pb);
        let mut ah = Box::new(original::MonoHistory::new());
        let mut bh = Box::new(velvet::MonoHistory::new());
        let mut at = 0usize;
        while at < 30000 {
            if [0, 4979, 10031, 17003, 22001].contains(&at) {
                pa.amount = if at % 2 == 0 { 0.25 } else { 0.9 };
                pa.decay_db = if at % 2 == 0 { -36.0 } else { -63.0 };
                pa.transition_ms = if at % 2 == 0 { 30.0 } else { 90.0 };
                pb.amount = pa.amount;
                pb.decay_db = pa.decay_db;
                pb.transition_ms = pa.transition_ms;
                a.set_parameters(pa);
                b.set_parameters(pb);
            }
            let boundary = [4979, 10031, 17003, 22001, 30000]
                .into_iter()
                .find(|&x| x > at)
                .unwrap();
            let n = (boundary - at).min(64);
            let mut x = [0.0f32; 64];
            let mut y = x;
            for i in 0..n {
                x[i] = (((at + i) * 37 % 101) as f32 - 50.0) / 200.0;
                y[i] = (((at + i) * 71 % 103) as f32 - 51.0) / 210.0;
            }
            let mut actual = [[0.0f32; 4]; 64];
            b.experimental_shared_block(&mut bh, &x[..n], &y[..n], &mut actual[..n]);
            for i in 0..n {
                let ((al, ar), (bl, br)) = a.process_shared_mono(&mut ah, x[i], y[i]);
                let expected = [al, ar, bl, br];
                for j in 0..4 {
                    assert_eq!(
                        actual[i][j].to_bits(),
                        expected[j].to_bits(),
                        "frame={} output={j}",
                        at + i
                    );
                }
            }
            assert_eq!(
                (
                    a.active_bank,
                    a.pending_bank,
                    a.transitioning,
                    a.transition_position,
                    a.generation
                ),
                (
                    b.active_bank,
                    b.pending_bank,
                    b.transitioning,
                    b.transition_position,
                    b.generation
                )
            );
            at += n;
        }
    }
}
