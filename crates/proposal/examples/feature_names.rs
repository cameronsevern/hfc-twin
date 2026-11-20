//! Writes the feature names, so training reads them from the one place they're defined.
fn main() {
    println!(
        "{}",
        serde_json::to_string(&*proposal::FEATURE_NAMES).unwrap_or_default()
    );
}
