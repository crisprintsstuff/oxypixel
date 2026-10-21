//! Runtime window icon. The bytes are produced by `scripts/gen_icon.py`.

use eframe::egui::IconData;

pub fn icon_data() -> IconData {
    let rgba = include_bytes!("../assets/icon32.rgba");
    IconData {
        rgba: rgba.to_vec(),
        width: 32,
        height: 32,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn icon_is_32_square_rgba() {
        let rgba = include_bytes!("../assets/icon32.rgba");
        assert_eq!(rgba.len(), 32 * 32 * 4);
    }

    #[test]
    fn hicolor_png_matches_runtime_icon() {
        let rgba = include_bytes!("../assets/icon32.rgba");
        let png = include_bytes!("../../../docs/linux/oxypixel.png");
        let image = image::load_from_memory(png)
            .expect("hicolor icon is a png")
            .to_rgba8();
        assert_eq!(image.dimensions(), (32, 32));
        assert_eq!(image.as_raw().as_slice(), rgba.as_slice());
    }
}
