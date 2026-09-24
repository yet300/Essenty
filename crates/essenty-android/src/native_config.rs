//! `NativeActivity` `android:configChanges` model and host diagnostic.
//!
//! The hosting Activity declares which configuration changes it handles in
//! place. For every declared change Android keeps the `NativeActivity`
//! running, updates its resources, and reports `MainEvent::ConfigChanged`
//! instead of destroying and recreating the Activity. The Rust
//! `android_main` invocation, its thread, and its local `InstanceKeeper`
//! therefore survive; no retained value crosses a thread boundary.
//!
//! This module is the single source of truth for the recommended categories:
//! [`NativeConfigCategory::baseline_manifest_value`] and
//! [`NativeConfigCategory::extended_manifest_value`] render the exact strings
//! mirrored in `docs/ANDROID.md`. [`HostConfigurationReport`] inspects a
//! declared bitmask without panicking, and the Android-only
//! `inspect_host_configuration` reads the live host value through
//! `PackageManager`.

use std::fmt::{self, Display, Formatter};

/// One `android:configChanges` category.
///
/// The bit values match `android.content.pm.ActivityInfo.CONFIG_*`; the
/// manifest names match `android:configChanges` attribute values.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NativeConfigCategory {
    /// Screen orientation change (API 1).
    Orientation,
    /// Current screen size change (API 13).
    ScreenSize,
    /// Smallest screen width change (API 13).
    SmallestScreenSize,
    /// Screen layout change, including multi-window (API 4).
    ScreenLayout,
    /// UI mode change such as night mode or desk/car dock (API 8).
    UiMode,
    /// Display density change (manifest value from API 24).
    Density,
    /// Font scale change (API 1).
    FontScale,
    /// Locale change (API 1).
    Locale,
    /// Layout direction change (API 17).
    LayoutDirection,
    /// Hardware keyboard type change (API 1).
    Keyboard,
    /// Keyboard availability change (API 1).
    KeyboardHidden,
    /// Navigation type change (API 1).
    Navigation,
    /// Touchscreen type change (API 1).
    Touchscreen,
    /// Mobile country code change (API 1).
    Mcc,
    /// Mobile network code change (API 1).
    Mnc,
    /// Color mode change such as wide gamut or HDR (API 26).
    ColorMode,
    /// System font weight adjustment change (API 31).
    FontWeightAdjustment,
    /// Grammatical gender change (API 34).
    GrammaticalGender,
}

impl NativeConfigCategory {
    /// Every modelled category in canonical manifest order.
    #[must_use]
    pub const fn all() -> [Self; 18] {
        [
            Self::Orientation,
            Self::ScreenSize,
            Self::SmallestScreenSize,
            Self::ScreenLayout,
            Self::UiMode,
            Self::Density,
            Self::FontScale,
            Self::Locale,
            Self::LayoutDirection,
            Self::Keyboard,
            Self::KeyboardHidden,
            Self::Navigation,
            Self::Touchscreen,
            Self::Mcc,
            Self::Mnc,
            Self::ColorMode,
            Self::FontWeightAdjustment,
            Self::GrammaticalGender,
        ]
    }

    /// Categories a self-drawn Rust UI commonly must handle to stay correct:
    /// display geometry, theme, density, font, and locale/direction.
    #[must_use]
    pub const fn baseline() -> [Self; 9] {
        [
            Self::Orientation,
            Self::ScreenSize,
            Self::SmallestScreenSize,
            Self::ScreenLayout,
            Self::UiMode,
            Self::Density,
            Self::FontScale,
            Self::Locale,
            Self::LayoutDirection,
        ]
    }

    /// Opt-in categories for hardware, telephony, and display refinements.
    /// Declare these only when the app actually reacts to the new state.
    #[must_use]
    pub const fn extended() -> [Self; 9] {
        [
            Self::Keyboard,
            Self::KeyboardHidden,
            Self::Navigation,
            Self::Touchscreen,
            Self::Mcc,
            Self::Mnc,
            Self::ColorMode,
            Self::FontWeightAdjustment,
            Self::GrammaticalGender,
        ]
    }

    /// The `android:configChanges` attribute value for this category.
    #[must_use]
    pub const fn manifest_value(self) -> &'static str {
        match self {
            Self::Orientation => "orientation",
            Self::ScreenSize => "screenSize",
            Self::SmallestScreenSize => "smallestScreenSize",
            Self::ScreenLayout => "screenLayout",
            Self::UiMode => "uiMode",
            Self::Density => "density",
            Self::FontScale => "fontScale",
            Self::Locale => "locale",
            Self::LayoutDirection => "layoutDirection",
            Self::Keyboard => "keyboard",
            Self::KeyboardHidden => "keyboardHidden",
            Self::Navigation => "navigation",
            Self::Touchscreen => "touchscreen",
            Self::Mcc => "mcc",
            Self::Mnc => "mnc",
            Self::ColorMode => "colorMode",
            Self::FontWeightAdjustment => "fontWeightAdjustment",
            Self::GrammaticalGender => "grammaticalGender",
        }
    }

    /// The `ActivityInfo.CONFIG_*` bit for this category.
    #[must_use]
    pub const fn bit(self) -> u32 {
        match self {
            Self::Mcc => 0x0001,
            Self::Mnc => 0x0002,
            Self::Locale => 0x0004,
            Self::Touchscreen => 0x0008,
            Self::Keyboard => 0x0010,
            Self::KeyboardHidden => 0x0020,
            Self::Navigation => 0x0040,
            Self::Orientation => 0x0080,
            Self::ScreenLayout => 0x0100,
            Self::UiMode => 0x0200,
            Self::ScreenSize => 0x0400,
            Self::SmallestScreenSize => 0x0800,
            Self::Density => 0x1000,
            Self::LayoutDirection => 0x2000,
            Self::ColorMode => 0x4000,
            Self::GrammaticalGender => 0x8000,
            Self::FontWeightAdjustment => 0x1000_0000,
            Self::FontScale => 0x4000_0000,
        }
    }

    /// First API level whose manifest accepts this category string.
    ///
    /// Older releases ignore unknown manifest values, so declaring a newer
    /// category is harmless on them; it simply does not prevent recreation
    /// there. `density` is the notable case: the `CONFIG_DENSITY` constant
    /// predates it, but the manifest string is honored from API 24.
    #[must_use]
    pub const fn since_api(self) -> u32 {
        match self {
            Self::Orientation
            | Self::FontScale
            | Self::Locale
            | Self::Keyboard
            | Self::KeyboardHidden
            | Self::Navigation
            | Self::Touchscreen
            | Self::Mcc
            | Self::Mnc => 1,
            Self::ScreenLayout => 4,
            Self::UiMode => 8,
            Self::ScreenSize | Self::SmallestScreenSize => 13,
            Self::LayoutDirection => 17,
            Self::Density => 24,
            Self::ColorMode => 26,
            Self::FontWeightAdjustment => 31,
            Self::GrammaticalGender => 34,
        }
    }

    /// What changes, what the Rust host is expected to update, and the cost
    /// of declaring the flag without reacting.
    #[must_use]
    pub const fn summary(self) -> &'static str {
        match self {
            Self::Orientation => {
                "Rotation between portrait and landscape. Re-read window size and configuration, then re-lay out. Declaring without re-layout leaves the old layout on the new orientation."
            }
            Self::ScreenSize => {
                "Current screen size changed (rotation, fold, resize, multi-window). Redraw from the current window and configuration. Ignoring it shows a stale layout."
            }
            Self::SmallestScreenSize => {
                "Smallest screen width changed (fold, large resize). Re-evaluate size-class breakpoints. Ignoring it keeps the previous size class."
            }
            Self::ScreenLayout => {
                "Screen layout changed (long/notlong, layout size, multi-window). Re-read the configuration. Ignoring it misses layout-class transitions."
            }
            Self::UiMode => {
                "UI mode changed (night mode, desk or car dock). Refresh theme-dependent colors and resources. Ignoring it keeps the previous theme."
            }
            Self::Density => {
                "Display density changed (display scaling, moving across displays). Recompute pixel metrics. Ignoring it renders at the wrong scale."
            }
            Self::FontScale => {
                "System font scale changed. Refresh text metrics and layout. Ignoring it keeps the previous text size."
            }
            Self::Locale => {
                "Locale changed. Reload localized Rust strings and resources. App-localized data is not updated automatically."
            }
            Self::LayoutDirection => {
                "Layout direction changed (LTR/RTL with the locale). Mirror direction-dependent UI. Ignoring it keeps the previous direction."
            }
            Self::Keyboard => {
                "Hardware keyboard type changed. Inspect input configuration if the UI adapts to keyboards. Otherwise nothing visible changes."
            }
            Self::KeyboardHidden => {
                "Keyboard availability changed. Update keyboard-dependent layout if any. Otherwise nothing visible changes."
            }
            Self::Navigation => {
                "Navigation type changed. Inspect navigation configuration if the UI adapts to it. Otherwise nothing visible changes."
            }
            Self::Touchscreen => {
                "Touchscreen type changed. Relevant only when touch hardware actually changes. Otherwise nothing visible changes."
            }
            Self::Mcc => {
                "SIM country code changed. Refresh carrier-dependent data if any. Otherwise nothing visible changes."
            }
            Self::Mnc => {
                "SIM network code changed. Refresh carrier-dependent data if any. Otherwise nothing visible changes."
            }
            Self::ColorMode => {
                "Color mode changed (wide gamut, HDR). Update color assumptions for color-sensitive rendering. Otherwise nothing visible changes."
            }
            Self::FontWeightAdjustment => {
                "System font weight adjustment changed (API 31+). Refresh typography. Ignoring it keeps the previous weight."
            }
            Self::GrammaticalGender => {
                "Grammatical gender changed (API 34+). Refresh localized text. Ignoring it keeps the previous inflection."
            }
        }
    }

    /// Pipe-joined manifest string for the baseline set.
    #[must_use]
    pub fn baseline_manifest_value() -> String {
        NativeConfigSet::baseline().manifest_value()
    }

    /// Pipe-joined manifest string for the extended set.
    #[must_use]
    pub fn extended_manifest_value() -> String {
        NativeConfigSet::extended().manifest_value()
    }

    /// Pipe-joined manifest string for baseline plus extended.
    #[must_use]
    pub fn recommended_manifest_value() -> String {
        NativeConfigSet::all_recommended().manifest_value()
    }
}

/// A set of [`NativeConfigCategory`] values backed by `ActivityInfo` bits.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NativeConfigSet {
    bits: u32,
}

impl NativeConfigSet {
    /// An empty set.
    #[must_use]
    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    /// Baseline plus extended: the full recommendation.
    #[must_use]
    pub const fn all_recommended() -> Self {
        Self::baseline().union(Self::extended())
    }

    /// Commonly required categories for a self-drawn Rust UI.
    #[must_use]
    pub const fn baseline() -> Self {
        Self::from_categories(&NativeConfigCategory::baseline())
    }

    /// Opt-in categories for hardware, telephony, and display refinements.
    #[must_use]
    pub const fn extended() -> Self {
        Self::from_categories(&NativeConfigCategory::extended())
    }

    const fn from_categories(categories: &[NativeConfigCategory]) -> Self {
        let mut bits = 0;
        let mut index = 0;
        while index < categories.len() {
            bits |= categories[index].bit();
            index += 1;
        }
        Self { bits }
    }

    const fn union(self, other: Self) -> Self {
        Self { bits: self.bits | other.bits }
    }

    /// Builds a set from raw `ActivityInfo.configChanges` bits.
    #[must_use]
    pub const fn from_bits(bits: u32) -> Self {
        Self { bits }
    }

    /// The raw bitmask.
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.bits
    }

    /// Returns `true` when the category is present.
    #[must_use]
    pub const fn contains(self, category: NativeConfigCategory) -> bool {
        self.bits & category.bit() != 0
    }

    /// Adds a category.
    pub fn insert(&mut self, category: NativeConfigCategory) {
        self.bits |= category.bit();
    }

    /// Categories present in this set, in canonical manifest order.
    #[must_use]
    pub fn categories(self) -> Vec<NativeConfigCategory> {
        NativeConfigCategory::all().into_iter().filter(|item| self.contains(*item)).collect()
    }

    /// Bits set that match no modelled category (future platform values).
    /// These are reported, never misclassified as recommended or missing.
    #[must_use]
    pub const fn unknown_bits(self) -> u32 {
        self.bits & !Self::all_recommended().bits
    }

    /// Pipe-joined manifest string in canonical order (empty for an empty set).
    #[must_use]
    pub fn manifest_value(self) -> String {
        self.categories()
            .iter()
            .map(|item: &NativeConfigCategory| item.manifest_value())
            .collect::<Vec<_>>()
            .join("|")
    }
}

/// Outcome of parsing an `android:configChanges` manifest string.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ManifestParse {
    /// Categories recognized by this library.
    pub set: NativeConfigSet,
    /// Tokens with no modelled category (for example future values).
    pub unknown: Vec<String>,
}

/// Parses a pipe-separated `android:configChanges` string.
///
/// Unknown tokens are collected into [`ManifestParse::unknown`] rather than
/// failing the whole parse, so future platform values never break inspection.
#[must_use]
pub fn parse_manifest_value(value: &str) -> ManifestParse {
    let mut set = NativeConfigSet::empty();
    let mut unknown = Vec::new();
    for token in value.split('|') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        if let Some(category) =
            NativeConfigCategory::all().into_iter().find(|item| item.manifest_value() == token)
        {
            set.insert(category);
        } else {
            unknown.push(token.to_owned());
        }
    }
    ManifestParse { set, unknown }
}

/// Structured diagnosis of a host Activity's declared `configChanges`.
///
/// Missing flags are an unsupported retention configuration, not memory
/// unsafety: undeclared changes recreate the Activity, destroy the local
/// keeper, and restore only serialized `StateKeeper` bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostConfigurationReport {
    declared_bits: i32,
    declared: NativeConfigSet,
    unknown_bits: u32,
    missing_baseline: Vec<NativeConfigCategory>,
    missing_extended: Vec<NativeConfigCategory>,
}

impl HostConfigurationReport {
    /// Builds a report from raw `ActivityInfo.configChanges` bits.
    #[must_use]
    pub fn from_bits(bits: i32) -> Self {
        let raw = u32::from_ne_bytes(bits.to_ne_bytes());
        let declared = NativeConfigSet::from_bits(raw);
        let missing_baseline = NativeConfigCategory::baseline()
            .into_iter()
            .filter(|item| !declared.contains(*item))
            .collect();
        let missing_extended = NativeConfigCategory::extended()
            .into_iter()
            .filter(|item| !declared.contains(*item))
            .collect();
        Self {
            declared_bits: bits,
            declared,
            unknown_bits: declared.unknown_bits(),
            missing_baseline,
            missing_extended,
        }
    }

    /// The raw `ActivityInfo.configChanges` value.
    #[must_use]
    pub const fn declared_bits(&self) -> i32 {
        self.declared_bits
    }

    /// Declared categories known to this library.
    #[must_use]
    pub const fn declared(&self) -> NativeConfigSet {
        self.declared
    }

    /// Declared bits matching no modelled category (future platform values).
    #[must_use]
    pub const fn unknown_bits(&self) -> u32 {
        self.unknown_bits
    }

    /// Baseline categories the host does not declare. When non-empty,
    /// ordinary configuration changes can recreate the Activity and reset
    /// the `InstanceKeeper`.
    #[must_use]
    pub fn missing_baseline(&self) -> &[NativeConfigCategory] {
        &self.missing_baseline
    }

    /// Extended categories the host does not declare. These narrow the
    /// retention guarantee for hardware, telephony, and display refinements.
    #[must_use]
    pub fn missing_extended(&self) -> &[NativeConfigCategory] {
        &self.missing_extended
    }

    /// Returns `true` when every baseline category is declared.
    #[must_use]
    pub fn meets_baseline(&self) -> bool {
        self.missing_baseline.is_empty()
    }

    /// Returns `true` when baseline and extended categories are all declared.
    #[must_use]
    pub fn is_fully_declared(&self) -> bool {
        self.missing_baseline.is_empty() && self.missing_extended.is_empty()
    }

    /// A one-line warning when baseline flags are missing, otherwise `None`.
    ///
    /// The message names the missing categories so the application knows
    /// which manifest entries to add. It is rate-limited by construction:
    /// call it once after inspection, not on every configuration event.
    #[must_use]
    pub fn retention_warning(&self) -> Option<String> {
        if self.missing_baseline.is_empty() {
            return None;
        }
        let missing = self
            .missing_baseline
            .iter()
            .map(|item: &NativeConfigCategory| item.manifest_value())
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!(
            "Essenty Android: Activity recreation may reset InstanceKeeper because {missing} {} not handled by this Activity.",
            if self.missing_baseline.len() == 1 { "is" } else { "are" }
        ))
    }
}

impl Display for HostConfigurationReport {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let declared = self.declared.manifest_value();
        let declared = if declared.is_empty() { "(none)".to_owned() } else { declared };
        let missing_baseline = self
            .missing_baseline
            .iter()
            .map(|item: &NativeConfigCategory| item.manifest_value())
            .collect::<Vec<_>>()
            .join("|");
        let missing_extended = self
            .missing_extended
            .iter()
            .map(|item: &NativeConfigCategory| item.manifest_value())
            .collect::<Vec<_>>()
            .join("|");
        writeln!(formatter, "declared: {declared}")?;
        writeln!(
            formatter,
            "missing baseline: {}",
            if missing_baseline.is_empty() { "(none)" } else { &missing_baseline }
        )?;
        writeln!(
            formatter,
            "missing extended: {}",
            if missing_extended.is_empty() { "(none)" } else { &missing_extended }
        )?;
        write!(formatter, "unknown bits: 0x{:x}", self.unknown_bits)
    }
}

/// Typed failure from the host configuration inspection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeConfigError {
    /// Operation that failed.
    pub operation: &'static str,
    /// JNI or platform error text.
    pub detail: String,
}

impl Display for NativeConfigError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "Android host configuration {} failed: {}", self.operation, self.detail)
    }
}

impl std::error::Error for NativeConfigError {}

/// Reads the hosting Activity's `ActivityInfo.configChanges` through
/// `PackageManager` and reports it against the recommendation.
///
/// This performs JNI calls on the calling thread and must be invoked from a
/// thread attached to the JVM (for example the `android_main` thread via
/// `JavaVM::attach_current_thread`, or the Java main thread). Missing flags
/// are reported in the returned [`HostConfigurationReport`]; only JNI and
/// lookup failures produce an error.
///
/// # Errors
///
/// Returns [`NativeConfigError`] when the JVM, Activity, package manager, or
/// `configChanges` field cannot be read.
#[cfg(all(target_os = "android", feature = "native-activity"))]
#[allow(unsafe_code)]
pub fn inspect_host_configuration(
    app: &android_activity::AndroidApp,
) -> Result<HostConfigurationReport, NativeConfigError> {
    use jni::{
        JavaVM,
        objects::{Global, JObject, JValue},
        signature::RuntimeMethodSignature,
    };

    // SAFETY: `android-activity` owns the process `JavaVM` and exposes its
    // pointer for the lifetime of the host. The pointer is only dereferenced
    // here to attach the calling thread.
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) };
    let bits = vm
        .attach_current_thread(|env| {
            let raw_activity = app.activity_as_ptr() as jni::sys::jobject;
            // SAFETY: `activity_as_ptr` is valid while the host Activity is
            // alive; the local wrapper never escapes this closure.
            let activity = unsafe { env.as_cast_raw::<Global<JObject<'_>>>(&raw_activity)? };
            let component = env
                .call_method(
                    activity.as_ref(),
                    jni::jni_str!("getComponentName"),
                    RuntimeMethodSignature::from_str("()Landroid/content/ComponentName;")?
                        .method_signature(),
                    &[],
                )?
                .l()?;
            let package_manager = env
                .call_method(
                    activity.as_ref(),
                    jni::jni_str!("getPackageManager"),
                    RuntimeMethodSignature::from_str("()Landroid/content/pm/PackageManager;")?
                        .method_signature(),
                    &[],
                )?
                .l()?;
            let info = env
                .call_method(
                    &package_manager,
                    jni::jni_str!("getActivityInfo"),
                    RuntimeMethodSignature::from_str(
                        "(Landroid/content/ComponentName;I)Landroid/content/pm/ActivityInfo;",
                    )?
                    .method_signature(),
                    &[JValue::Object(&component), JValue::Int(0)],
                )?
                .l()?;
            env.get_field(&info, jni::jni_str!("configChanges"), jni::jni_sig!("I"))?.i()
        })
        .map_err(|cause| NativeConfigError { operation: "inspect", detail: cause.to_string() })?;
    clear_pending_exception()?;
    Ok(HostConfigurationReport::from_bits(bits))
}

/// Emits the [`HostConfigurationReport::retention_warning`] once through
/// `log`, if a baseline gap was diagnosed. Call once after inspection, not
/// on every configuration event.
#[cfg(all(target_os = "android", feature = "native-activity"))]
pub fn log_host_configuration_warning(report: &HostConfigurationReport) {
    if let Some(warning) = report.retention_warning() {
        log::warn!("{warning}");
    }
}

#[cfg(all(target_os = "android", feature = "native-activity"))]
fn clear_pending_exception() -> Result<(), NativeConfigError> {
    jni_min_helper::jni_with_env(|env| {
        if env.exception_check() {
            env.exception_clear();
        }
        Ok::<_, jni::errors::Error>(())
    })
    .map_err(|cause| NativeConfigError { operation: "inspect", detail: cause.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_values_round_trip() {
        let parsed = parse_manifest_value(&NativeConfigCategory::recommended_manifest_value());
        assert!(parsed.unknown.is_empty());
        assert_eq!(parsed.set, NativeConfigSet::all_recommended());
        assert_eq!(parsed.set.categories().len(), 18);
    }

    #[test]
    fn baseline_and_extended_partition_the_recommendation() {
        let baseline = NativeConfigSet::baseline();
        let extended = NativeConfigSet::extended();
        assert_eq!(baseline.bits() & extended.bits(), 0);
        assert_eq!(baseline.bits() | extended.bits(), NativeConfigSet::all_recommended().bits());
        assert_eq!(NativeConfigCategory::baseline().len(), 9);
        assert_eq!(NativeConfigCategory::extended().len(), 9);
    }

    #[test]
    fn manifest_strings_are_stable_single_source_of_truth() {
        assert_eq!(
            NativeConfigCategory::baseline_manifest_value(),
            "orientation|screenSize|smallestScreenSize|screenLayout|uiMode|density|fontScale|locale|layoutDirection"
        );
        assert_eq!(
            NativeConfigCategory::extended_manifest_value(),
            "keyboard|keyboardHidden|navigation|touchscreen|mcc|mnc|colorMode|fontWeightAdjustment|grammaticalGender"
        );
        assert_eq!(
            NativeConfigCategory::recommended_manifest_value(),
            format!(
                "{}|{}",
                NativeConfigCategory::baseline_manifest_value(),
                NativeConfigCategory::extended_manifest_value()
            )
        );
    }

    #[test]
    fn unknown_manifest_tokens_are_collected_not_dropped() {
        let parsed = parse_manifest_value("orientation|someFutureFlag|screenSize|");
        assert_eq!(parsed.unknown, vec!["someFutureFlag".to_owned()]);
        assert!(parsed.set.contains(NativeConfigCategory::Orientation));
        assert!(parsed.set.contains(NativeConfigCategory::ScreenSize));
    }

    #[test]
    fn full_declaration_has_no_warning() {
        let report = HostConfigurationReport::from_bits(i32::from_ne_bytes(
            NativeConfigSet::all_recommended().bits().to_ne_bytes(),
        ));
        assert!(report.meets_baseline());
        assert!(report.is_fully_declared());
        assert!(report.retention_warning().is_none());
        assert_eq!(report.unknown_bits(), 0);
    }

    #[test]
    fn missing_baseline_warns_and_names_categories() {
        let mut set = NativeConfigSet::all_recommended();
        set.bits &= !NativeConfigCategory::Orientation.bit();
        set.bits &= !NativeConfigCategory::ScreenSize.bit();
        let report =
            HostConfigurationReport::from_bits(i32::from_ne_bytes(set.bits().to_ne_bytes()));
        assert!(!report.meets_baseline());
        assert_eq!(report.missing_baseline().len(), 2);
        let warning = report.retention_warning().expect("baseline gap warns");
        assert!(warning.contains("orientation"));
        assert!(warning.contains("screenSize"));
        assert!(warning.contains("InstanceKeeper"));
    }

    #[test]
    fn missing_extended_only_is_informational() {
        let report = HostConfigurationReport::from_bits(i32::from_ne_bytes(
            NativeConfigSet::baseline().bits().to_ne_bytes(),
        ));
        assert!(report.meets_baseline());
        assert!(!report.is_fully_declared());
        assert!(report.retention_warning().is_none());
        assert_eq!(report.missing_extended().len(), 9);
    }

    #[test]
    fn report_renders_documentation_friendly_output() {
        let report = HostConfigurationReport::from_bits(0);
        let rendered = format!("{report}");
        assert!(rendered.contains("declared: (none)"));
        assert!(rendered.contains("missing baseline: "));
        assert!(rendered.contains("orientation"));
        assert!(rendered.contains("unknown bits: 0x0"));
    }
}
