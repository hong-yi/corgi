//! Replay captured terminal output and print the resulting screen as text.
//! `cargo run -p corgi-vt --example dump -- <file> <cols> <rows>`

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, file, cols, rows] = args.as_slice() else {
        eprintln!("usage: dump <file> <cols> <rows>");
        std::process::exit(2);
    };
    let bytes = std::fs::read(file).expect("read file");
    let mut vt = corgi_vt::Vt::new(cols.parse().unwrap(), rows.parse().unwrap()).unwrap();
    vt.write(&bytes);
    let f = vt.frame().unwrap();
    for y in 0..f.rows {
        let line: String = (0..f.cols)
            .map(|x| {
                let c = f.cell(x, y);
                if c.wide == corgi_vt::Wide::Tail {
                    String::new()
                } else if c.text.is_empty() {
                    " ".into()
                } else {
                    c.text.clone()
                }
            })
            .collect();
        println!("{:2}|{}", y, line.trim_end());
    }
    println!("cursor: {:?}", f.cursor);
    // DUMP_BG="x,y x,y": also print those cells' background colours.
    // DUMP_NOBG=y0: list cells from row y0 down that have no background.
    if let Ok(y0) = std::env::var("DUMP_NOBG") {
        let y0: u16 = y0.parse().unwrap();
        let missing: Vec<(u16, u16)> = (y0..f.rows)
            .flat_map(|y| (0..f.cols).map(move |x| (x, y)))
            .filter(|&(x, y)| f.cell(x, y).bg.is_none())
            .collect();
        println!(
            "{} cells without background: {:?}",
            missing.len(),
            &missing[..missing.len().min(40)]
        );
    }
    for xy in std::env::var("DUMP_BG").unwrap_or_default().split_whitespace() {
        let (x, y) = xy.split_once(',').unwrap();
        let (x, y): (u16, u16) = (x.parse().unwrap(), y.parse().unwrap());
        println!("bg({x},{y}) = {:?}", f.cell(x, y).bg);
    }
}
