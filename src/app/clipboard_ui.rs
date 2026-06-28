use anyhow::{Result, anyhow};
use copypasta::{ClipboardContext, ClipboardProvider};

pub(super) fn copy_text_to_clipboard(text: &str) -> Result<()> {
    let mut clipboard =
        ClipboardContext::new().map_err(|error| anyhow!("failed to access clipboard: {error}"))?;
    clipboard
        .set_contents(text.to_string())
        .map_err(|error| anyhow!("failed to write clipboard: {error}"))
}

pub(super) fn read_text_from_clipboard() -> Result<String> {
    let mut clipboard =
        ClipboardContext::new().map_err(|error| anyhow!("failed to access clipboard: {error}"))?;
    clipboard
        .get_contents()
        .map_err(|error| anyhow!("failed to read clipboard: {error}"))
}
