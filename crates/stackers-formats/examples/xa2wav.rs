//! Decodes every XA table entry to a WAV: `xa2wav <track01.bin> <SLUS_003.15> <out_dir>`.
use stackers_formats::{iso::Disc, xa};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let [_, bin, exe, out] = &a[..] else { panic!("usage: xa2wav <track01.bin> <exe> <out_dir>") };
    let raw = std::fs::read(bin).unwrap();
    let disc = Disc::open(&raw).unwrap();
    let table = xa::table_from_exe(&std::fs::read(exe).unwrap()).unwrap();
    std::fs::create_dir_all(out).unwrap();
    for (i, e) in table.iter().enumerate() {
        let pcm = xa::decode_entry(&disc, e).unwrap();
        let peak = pcm.iter().map(|v| v.unsigned_abs()).max().unwrap_or(0);
        let clip = pcm.iter().filter(|v| v.unsigned_abs() >= 0x7FFF).count();
        let rms = (pcm.iter().map(|&v| f64::from(v).powi(2)).sum::<f64>() / pcm.len() as f64).sqrt();
        println!("{i}: {e:?} {:.2}s peak {peak} rms {rms:.0} clipped {clip}", pcm.len() as f64 / 2.0 / f64::from(xa::RATE));
        let data = pcm.len() as u32 * 2;
        let mut w = Vec::new();
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&(36 + data).to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        for v in [1u16, 2] { w.extend_from_slice(&v.to_le_bytes()); }
        for v in [xa::RATE, xa::RATE * 4] { w.extend_from_slice(&v.to_le_bytes()); }
        for v in [4u16, 16] { w.extend_from_slice(&v.to_le_bytes()); }
        w.extend_from_slice(b"data");
        w.extend_from_slice(&data.to_le_bytes());
        for v in &pcm { w.extend_from_slice(&v.to_le_bytes()); }
        std::fs::write(format!("{out}/xa{i}.wav"), w).unwrap();
    }
}
