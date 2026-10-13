// SPDX-License-Identifier: MIT OR Apache-2.0
//! Finder-Tags (nur macOS): erweitertes Attribut `com.apple.metadata:_kMDItemUserTags`, eine binäre plist mit
//! einer Liste von Namen. Vorhandene Tags des Nutzers bleiben erhalten. Spotlight und der Finder lesen sie direkt.

use std::path::Path;

#[cfg(target_os = "macos")]
const ATTR: &str = "com.apple.metadata:_kMDItemUserTags";

#[cfg(target_os = "macos")]
pub fn add_finder_tags(path: &Path, tags: &[String]) -> std::io::Result<()> {
    let mut all: Vec<String> = match xattr::get(path, ATTR)? {
        Some(bytes) => match plist::from_bytes::<plist::Value>(&bytes) {
            Ok(plist::Value::Array(items)) => items.into_iter().filter_map(|v| v.into_string()).collect(),
            _ => Vec::new(),
        },
        None => Vec::new(),
    };
    for t in tags {
        // Tags können eine Farbe tragen („Name\n6“): nur den Namen vergleichen
        if !all.iter().any(|a| a.split('\n').next() == Some(t.as_str())) {
            all.push(t.clone());
        }
    }
    let value = plist::Value::Array(all.into_iter().map(plist::Value::String).collect());
    let mut buf = Vec::new();
    plist::to_writer_binary(&mut buf, &value).map_err(std::io::Error::other)?;
    xattr::set(path, ATTR, &buf)
}

#[cfg(not(target_os = "macos"))]
pub fn add_finder_tags(_path: &Path, _tags: &[String]) -> std::io::Result<()> {
    Ok(())
}
