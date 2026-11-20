//! Profile rules (docs/02 §28). A profile is a separate collection (its own database, media,
//! settings and plugin permissions); the registry of profiles lives outside every collection.

/// Longest profile name, in characters.
pub const MAX_NAME_CHARS: usize = 64;

/// Most profiles one installation keeps.
pub const MAX_PROFILES: usize = 32;

/// Id of the profile every installation starts with (its collection is the one older
/// versions used, so upgrading keeps the data where it was).
pub const DEFAULT_PROFILE_ID: &str = "default";

/// Why a profile name was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileNameError {
    /// Blank after trimming.
    Empty,
    /// Longer than [`MAX_NAME_CHARS`].
    TooLong,
    /// Holds control characters.
    Control,
}

impl ProfileNameError {
    /// Stable reason code (`details.reason` of the API error).
    pub fn code(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::TooLong => "too_long",
            Self::Control => "control",
        }
    }
}

/// Trims and checks a profile name.
pub fn normalize_name(raw: &str) -> Result<String, ProfileNameError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(ProfileNameError::Empty);
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(ProfileNameError::TooLong);
    }
    if name.chars().any(char::is_control) {
        return Err(ProfileNameError::Control);
    }
    Ok(name.to_owned())
}

/// Whether `id` may name a profile directory: the default id, or lowercase hex/`-` only (the
/// app makes ids from UUIDs). Anything else (`..`, separators, drive letters) is refused, so
/// a hand-edited registry can never point outside the profiles folder.
pub fn valid_id(id: &str) -> bool {
    id == DEFAULT_PROFILE_ID
        || (!id.is_empty()
            && id.len() <= 64
            && id
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c) || c == '-'))
}

/// Whether two names clash (case-insensitive, Unicode lowercase).
pub fn same_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_trimmed_and_checked() {
        assert_eq!(normalize_name("  Arda ").as_deref(), Ok("Arda"));
        assert_eq!(normalize_name("   "), Err(ProfileNameError::Empty));
        assert_eq!(
            normalize_name(&"ş".repeat(64)).map(|n| n.chars().count()),
            Ok(64)
        );
        assert_eq!(
            normalize_name(&"ş".repeat(65)),
            Err(ProfileNameError::TooLong)
        );
        assert_eq!(normalize_name("a\u{7}b"), Err(ProfileNameError::Control));
        assert!(same_name("İpek", " i̇pek"));
        assert!(!same_name("Ali", "Veli"));
    }

    #[test]
    fn ids_cannot_escape_the_profiles_folder() {
        assert!(valid_id("default"));
        assert!(valid_id("01900000-0000-7000-8000-0000000000aa"));
        for bad in [
            "",
            "..",
            "../x",
            "a/b",
            "a\\b",
            "C:",
            "Default",
            "x".repeat(65).as_str(),
        ] {
            assert!(!valid_id(bad), "{bad}");
        }
    }
}
