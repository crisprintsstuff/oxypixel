//! Native file dialogs and error boxes. Windows uses the system dialogs.
//! Other targets keep the in-app path field, which does not need a GUI toolkit.

use std::path::PathBuf;

#[cfg(windows)]
pub fn pick_path(save: bool, title: &str, filter: &str, extensions: &[&str]) -> Option<PathBuf> {
    let dialog = rfd::FileDialog::new()
        .set_title(title)
        .add_filter(filter, extensions);
    if save {
        dialog.save_file()
    } else {
        dialog.pick_file()
    }
}

#[cfg(not(windows))]
pub fn pick_path(save: bool, title: &str, filter: &str, extensions: &[&str]) -> Option<PathBuf> {
    let _ = (save, title, filter, extensions);
    None
}

pub fn alert(message: &str) {
    eprintln!("oxypixel: {message}");
    #[cfg(windows)]
    {
        let _ = rfd::MessageDialog::new()
            .set_title("OxyPixel")
            .set_description(message)
            .set_level(rfd::MessageLevel::Error)
            .show();
    }
}
