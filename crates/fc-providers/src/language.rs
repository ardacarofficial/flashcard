//! Translation, dictionary and lemmatization providers.

use crate::provider::{ProviderInfo, ProviderResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Translation result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Translation {
    /// Translated text.
    pub text: String,
    /// Detected source language if `from` was `None`.
    pub detected_source: Option<String>,
    /// Alternatives (best first).
    pub alternatives: Vec<String>,
}

/// Machine translation (AI, DeepL…).
#[async_trait]
pub trait TranslationProvider: Send + Sync {
    /// Metadata.
    fn info(&self) -> ProviderInfo;
    /// Translates `text` from `from` (`None` = detect) to `to` (BCP-47).
    async fn translate(
        &self,
        text: &str,
        from: Option<&str>,
        to: &str,
    ) -> ProviderResult<Translation>;
}

/// One dictionary sense.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictionarySense {
    /// Part of speech (`noun`, `verb`…).
    pub part_of_speech: Option<String>,
    /// Definition or translation.
    pub definition: String,
    /// Example sentences.
    pub examples: Vec<String>,
}

/// A dictionary entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictionaryEntry {
    /// Headword.
    pub word: String,
    /// Pronunciation (IPA).
    pub pronunciation: Option<String>,
    /// Senses.
    pub senses: Vec<DictionarySense>,
}

/// Dictionary lookup (plugins).
#[async_trait]
pub trait DictionaryProvider: Send + Sync {
    /// Metadata.
    fn info(&self) -> ProviderInfo;
    /// Looks up `word` in `language`, with glosses in `gloss_language` if supported.
    async fn lookup(
        &self,
        word: &str,
        language: &str,
        gloss_language: Option<&str>,
    ) -> ProviderResult<Vec<DictionaryEntry>>;
}

/// A token with its lemma.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    /// Byte offset in the input.
    pub start: usize,
    /// Byte end offset.
    pub end: usize,
    /// Surface form.
    pub text: String,
    /// Lemma (dictionary form); equals `text` if unknown.
    pub lemma: String,
}

/// Tokenization + lemmatization (reading mode, v0.10). Synchronous: implementations are local.
pub trait Lemmatizer: Send + Sync {
    /// Metadata.
    fn info(&self) -> ProviderInfo;
    /// Splits `text` into word tokens with lemmas.
    fn analyze(&self, text: &str, language: &str) -> ProviderResult<Vec<Token>>;
}
