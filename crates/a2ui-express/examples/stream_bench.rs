//! Times a char-by-char stream of eui's kitchen-sink surface.
use a2ui_express::compile::Compiler;
use a2ui_express::response::{AfterRejectedBlock, ExpressParser};
use a2ui_express::schema::Catalog;
fn main() {
    let cat = Catalog::from_json_str(
        &std::fs::read_to_string("../../vendor/eui/catalog/core.schema.json").unwrap(),
    )
    .unwrap();
    let mut p = ExpressParser::new(Compiler::new(vec![cat], None).unwrap(), "main");
    if std::env::args().nth(1).as_deref() == Some("noheal") {
        p = p.with_progressive_keys(Vec::<String>::new());
    }
    let size: usize = std::env::args().nth(2).map_or(1, |v| v.parse().unwrap());
    let src = std::fs::read_to_string("tests/eui/kitchen-sink.a2ui").unwrap();
    for _ in 0..3 {
        let t = std::time::Instant::now();
        let mut s = p.open_stream_with(false, AfterRejectedBlock::End, p.new_state());
        let mut i = 0;
        while i < src.len() {
            let mut e = (i + size).min(src.len());
            while !src.is_char_boundary(e) {
                e += 1;
            }
            s.push(&src[i..e]).unwrap();
            i = e;
        }
        s.finish().unwrap();
        println!("{:?}", t.elapsed());
    }
}
