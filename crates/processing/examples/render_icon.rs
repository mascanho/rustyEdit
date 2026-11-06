//! Regenerates the app icon's PNGs from `assets/icon.svg` (e.g. after
//! editing the design). Not part of the app; run with
//! `cargo run -p rustyedit-processing --example render_icon`, then
//! rebuild `assets/icon.icns` (used by `scripts/bundle-mac.sh`) with:
//!
//! ```sh
//! cd assets && mkdir icon.iconset
//! cp icon-16.png icon.iconset/icon_16x16.png
//! cp icon-32.png icon.iconset/icon_16x16@2x.png
//! cp icon-32.png icon.iconset/icon_32x32.png
//! cp icon-64.png icon.iconset/icon_32x32@2x.png
//! cp icon-128.png icon.iconset/icon_128x128.png
//! cp icon-256.png icon.iconset/icon_128x128@2x.png
//! cp icon-256.png icon.iconset/icon_256x256.png
//! cp icon-512.png icon.iconset/icon_256x256@2x.png
//! cp icon-512.png icon.iconset/icon_512x512.png
//! cp icon-1024.png icon.iconset/icon_512x512@2x.png
//! iconutil -c icns icon.iconset -o icon.icns
//! cp icon-512.png icon.png  # the README's copy
//! rm -rf icon.iconset icon-*.png
//! ```

use resvg::{tiny_skia, usvg};

const SIZES: &[u32] = &[16, 32, 64, 128, 256, 512, 1024];

fn main() {
    let svg_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/icon.svg");
    let svg = std::fs::read_to_string(&svg_path).expect("read icon.svg");
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_str(&svg, &options).expect("parse icon.svg");
    let out_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");

    for &size in SIZES {
        let mut pixmap = tiny_skia::Pixmap::new(size, size).expect("pixmap");
        let scale = size as f32 / tree.size().width();
        let transform = tiny_skia::Transform::from_scale(scale, scale);
        resvg::render(&tree, transform, &mut pixmap.as_mut());
        let path = out_dir.join(format!("icon-{size}.png"));
        pixmap.save_png(&path).expect("save png");
        println!("wrote {}", path.display());
    }
}
