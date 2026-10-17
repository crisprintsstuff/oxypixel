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
}
