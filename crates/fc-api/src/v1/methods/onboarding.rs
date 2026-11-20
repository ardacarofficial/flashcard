//! First-run guide and starter decks (v1.0).

use crate::v1::common::Empty;
use crate::v1::onboarding::*;

methods! {
    /// Whether to show the first-run guide.
    OnboardingStatus = "onboarding.status", Query, "onboarding", "decks:read", Public, undo: false, Empty => OnboardingState;
    /// Subjects with what each sets up, and the starter decks.
    OnboardingOptions = "onboarding.options", Query, "onboarding", "decks:read", Public, undo: false, LocaleRequest => OnboardingChoices;
    /// Sets up the chosen subject (deck, classification template, preset, starter decks) in
    /// one undo step (the nested calls are recorded) and finishes the guide.
    OnboardingApply = "onboarding.apply", Command, "onboarding", "decks:write", Public, undo: false, ApplyOnboarding => OnboardingResult;
    /// Finishes the guide without setting anything up.
    OnboardingSkip = "onboarding.skip", Command, "onboarding", "decks:write", Public, undo: false, Empty => OnboardingState;
    /// Built-in starter decks.
    StarterList = "starter.list", Query, "onboarding", "decks:read", Public, undo: false, LocaleRequest => StarterDeckList;
    /// Installs a starter deck (one undo step: its nested calls are recorded).
    StarterInstall = "starter.install", Command, "onboarding", "decks:write", Public, undo: false, InstallStarter => StarterInstalled;
}
