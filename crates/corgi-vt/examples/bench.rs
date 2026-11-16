//! Rough throughput numbers: `cargo run --release -p corgi-vt --example bench`

use std::time::Instant;

fn main() {
    let (cols, rows) = (200u16, 50u16);
    let mut vt = corgi_vt::Vt::new(cols, rows).unwrap();
    // ~10 MB of coloured log lines, like a noisy build.
    let line =
        b"\x1b[32m   Compiling\x1b[0m some-crate v1.2.3 (/path/to/workspace/crates/some-crate) \x1b[1mok\x1b[0m\r\n";
    let data: Vec<u8> = line.iter().copied().cycle().take(10 << 20).collect();
    let t = Instant::now();
    vt.write(&data);
    let w = t.elapsed();
    println!("parse: {:.0} MB/s", data.len() as f64 / w.as_secs_f64() / 1e6);

    // Full-screen styled content, then time frame capture.
    for y in 0..rows {
        vt.write(format!("\x1b[{};1H", y + 1).as_bytes());
        for x in 0..cols {
            vt.write(format!("\x1b[38;5;{}m{}", (x + y) % 256, (b'a' + (x % 26) as u8) as char).as_bytes());
        }
    }
    let n = 200;
    let time = |vt: &mut corgi_vt::Vt, each: &dyn Fn(&mut corgi_vt::Vt, u32)| {
        let t = Instant::now();
        for i in 0..n {
            each(vt, i);
            std::hint::black_box(vt.frame().unwrap());
        }
        t.elapsed() / n
    };
    // A full repaint every time (what a scrolling log does).
    let per = time(&mut vt, &|vt, _| vt.write(b"\x1b[2J"));
    println!(
        "frame {cols}x{rows}, all rows changed: {per:?} per capture ({:.0} fps single pane)",
        1.0 / per.as_secs_f64()
    );
    // Typing: one row changes.
    let per = time(&mut vt, &|vt, i| vt.write(&[b'a' + (i % 26) as u8]));
    println!("frame {cols}x{rows}, one row changed: {per:?} per capture");
    let per = time(&mut vt, &|_, _| {});
    println!("frame {cols}x{rows}, unchanged: {per:?} per capture");
}
