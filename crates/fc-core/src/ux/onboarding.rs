//! First-run guide (docs/02 §32): "What do you want to learn?" → a deck with the matching
//! classification template (CEFR for English, YKS for the Turkish university exam…), the
//! note type that fits, a study preset and the starter decks worth offering. Short and
//! skippable; [`plan`] is the whole decision, the app only carries it out.

use super::starter::{self, StarterDeck};
use crate::i18n::LocalizedText;
use serde::{Deserialize, Serialize};

/// What the user studies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum StudyGoal {
    /// A language.
    Language,
    /// An exam.
    Exam,
    /// School lessons.
    School,
    /// Anything else.
    General,
}

/// The specific subject of a goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum StudySubject {
    /// English (CEFR levels).
    English,
    /// Japanese (JLPT levels).
    Japanese,
    /// Chinese (HSK levels).
    Chinese,
    /// Another language (CEFR levels).
    OtherLanguage,
    /// YKS (TYT/AYT/YDT).
    Yks,
    /// Another exam (difficulty).
    OtherExam,
    /// School (grades 1–12).
    School,
    /// General knowledge (difficulty).
    General,
}

impl StudySubject {
    /// The goal a subject belongs to.
    pub fn goal(self) -> StudyGoal {
        match self {
            Self::English | Self::Japanese | Self::Chinese | Self::OtherLanguage => {
                StudyGoal::Language
            }
            Self::Yks | Self::OtherExam => StudyGoal::Exam,
            Self::School => StudyGoal::School,
            Self::General => StudyGoal::General,
        }
    }

    /// Every subject, in display order.
    pub const ALL: [Self; 8] = [
        Self::English,
        Self::Japanese,
        Self::Chinese,
        Self::OtherLanguage,
        Self::Yks,
        Self::OtherExam,
        Self::School,
        Self::General,
    ];
}

/// Study preset the guide creates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresetPlan {
    /// Preset name (reused if one with this name exists).
    pub name: LocalizedText,
    /// New cards per day.
    pub new_per_day: u32,
}

/// What the guide sets up for a subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnboardingPlan {
    /// Subject.
    pub subject: StudySubject,
    /// Name of the deck to create (the user may override it).
    pub deck_name: LocalizedText,
    /// Classification template defined on the deck.
    pub template: Option<&'static str>,
    /// Built-in note type suggested for new notes in the deck.
    pub notetype: &'static str,
    /// Preset assigned to the deck (`None` = the defaults).
    pub preset: Option<PresetPlan>,
    /// Starter decks offered (ids, in order); the app guide is offered to everyone.
    pub starters: Vec<&'static str>,
}

const LANGUAGE_PRESET: PresetPlan = PresetPlan {
    name: LocalizedText {
        en: "Language learning",
        tr: "Dil öğrenme",
    },
    new_per_day: 15,
};

const EXAM_PRESET: PresetPlan = PresetPlan {
    name: LocalizedText {
        en: "Exam preparation",
        tr: "Sınav hazırlığı",
    },
    new_per_day: 30,
};

/// The setup for `subject`.
pub fn plan(subject: StudySubject) -> OnboardingPlan {
    use StudySubject as S;
    let lt = |en, tr| LocalizedText { en, tr };
    let (deck_name, template, notetype, preset) = match subject {
        S::English => (
            lt("English", "İngilizce"),
            Some("cefr"),
            "vocabulary",
            Some(LANGUAGE_PRESET),
        ),
        S::Japanese => (
            lt("Japanese", "Japonca"),
            Some("jlpt"),
            "vocabulary",
            Some(LANGUAGE_PRESET),
        ),
        S::Chinese => (
            lt("Chinese", "Çince"),
            Some("hsk"),
            "vocabulary",
            Some(LANGUAGE_PRESET),
        ),
        S::OtherLanguage => (
            lt("Language", "Dil"),
            Some("cefr"),
            "vocabulary",
            Some(LANGUAGE_PRESET),
        ),
        S::Yks => (lt("YKS", "YKS"), Some("yks"), "classic", Some(EXAM_PRESET)),
        S::OtherExam => (
            lt("Exam", "Sınav"),
            Some("difficulty"),
            "classic",
            Some(EXAM_PRESET),
        ),
        S::School => (lt("School", "Okul"), Some("school_grade"), "classic", None),
        S::General => (lt("General", "Genel"), Some("difficulty"), "classic", None),
    };
    let starters = starter::all()
        .iter()
        .filter(|d| fits(d, subject))
        .map(|d| d.id)
        .collect();
    OnboardingPlan {
        subject,
        deck_name,
        template,
        notetype,
        preset,
        starters,
    }
}

/// Whether a starter deck suits the subject: same classification template **and** goal, or
/// the app guide (everyone).
fn fits(d: &StarterDeck, subject: StudySubject) -> bool {
    use starter::StarterGoal as G;
    match d.goal {
        G::App => true,
        G::Language => subject == StudySubject::English && d.template == Some("cefr"),
        G::Exam => subject == StudySubject::Yks && d.template == Some("yks"),
        G::General => subject == StudySubject::General,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attribute::find_template;
    use crate::content::builtin;

    #[test]
    fn every_subject_has_a_valid_plan() {
        for s in StudySubject::ALL {
            let p = plan(s);
            assert_eq!(p.subject.goal(), s.goal());
            assert!(builtin::find(p.notetype).is_some(), "{s:?}");
            if let Some(t) = p.template {
                assert!(find_template(t).is_some(), "{s:?}: {t}");
            }
            assert!(p.starters.contains(&"app_guide"));
            for id in &p.starters {
                assert!(starter::find(id).is_some());
            }
        }
    }

    #[test]
    fn english_uses_cefr_and_yks_uses_the_exam_template() {
        let en = plan(StudySubject::English);
        assert_eq!(en.template, Some("cefr"));
        assert_eq!(en.notetype, "vocabulary");
        assert_eq!(en.starters, vec!["english_basics", "app_guide"]);
        let yks = plan(StudySubject::Yks);
        assert_eq!(yks.template, Some("yks"));
        assert_eq!(yks.starters, vec!["yks_tyt_math", "app_guide"]);
        assert_eq!(yks.preset.map(|p| p.new_per_day), Some(30));
        // Japanese learners are not offered English words.
        assert_eq!(plan(StudySubject::Japanese).starters, vec!["app_guide"]);
        assert_eq!(StudySubject::Yks.goal(), StudyGoal::Exam);
    }
}
