//! Dumps every tone of a VAB as a WAV at its centre key: `vab2wav <x.VH> <x.VB> <out_dir>`.
use stackers_formats::vab::VabHeader;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let [_, vh, vb, out] = &a[..] else { panic!("usage: vab2wav <VH> <VB> <out_dir>") };
    let h = VabHeader::parse(&std::fs::read(vh).unwrap()).unwrap();
    let body = std::fs::read(vb).unwrap();
    std::fs::create_dir_all(out).unwrap();
    for (p, prog) in h.programs.iter().enumerate() {
        for (t, tone) in prog.tones.iter().enumerate() {
            let s = h.vag(&body, tone.vag).unwrap();
            let rate = tone.rate_for_note(tone.center).round() as u32;
            let peak = s.pcm.iter().map(|v| v.unsigned_abs()).max().unwrap_or(0);
            let clip = s.pcm.iter().filter(|v| v.unsigned_abs() >= 0x7FFF).count();
            println!("p{p:02} t{t:02} vag{:3} keys {:3}-{:3} centre {:3} {:6} samples peak {peak:5} clipped {clip} loop {:?}",
                tone.vag, tone.min, tone.max, tone.center, s.pcm.len(), s.loop_start);
            let mut w = Vec::new();
            let data = s.pcm.len() as u32 * 2;
            w.extend_from_slice(b"RIFF");
            w.extend_from_slice(&(36 + data).to_le_bytes());
            w.extend_from_slice(b"WAVEfmt ");
            for v in [16u32] { w.extend_from_slice(&v.to_le_bytes()); }
            for v in [1u16, 1] { w.extend_from_slice(&v.to_le_bytes()); }
            for v in [rate, rate * 2] { w.extend_from_slice(&v.to_le_bytes()); }
            for v in [2u16, 16] { w.extend_from_slice(&v.to_le_bytes()); }
            w.extend_from_slice(b"data");
            w.extend_from_slice(&data.to_le_bytes());
            for v in &s.pcm { w.extend_from_slice(&v.to_le_bytes()); }
            std::fs::write(format!("{out}/p{p:02}_t{t:02}.wav"), w).unwrap();
        }
    }
}
