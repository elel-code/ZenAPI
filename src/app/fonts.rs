use slint::fontique_08::{fontique, shared_collection};
use std::sync::{Arc, Once};

static REGISTER: Once = Once::new();

pub(super) fn register_app_fonts() {
    REGISTER.call_once(|| {
        register_font(
            include_bytes!("../../ui/fonts/Inter-VariableFont.ttf"),
            "Zen Sans",
        );
        register_font(
            include_bytes!("../../ui/fonts/NotoSansMono-Regular.ttf"),
            "Zen Mono",
        );
    });
}

fn register_font(bytes: &'static [u8], family_name: &'static str) {
    let data = fontique::Blob::new(Arc::new(bytes.to_vec()));
    let mut collection = shared_collection();
    collection.register_fonts(
        data,
        Some(fontique::FontInfoOverride {
            family_name: Some(family_name),
            ..Default::default()
        }),
    );
}
