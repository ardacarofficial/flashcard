//! `math` and `code_highlight` switches (v0.3, docs/02 §14). The backend always emits math as
//! `span.math` and code as `code.language-*`; these modules only tell the frontend whether to
//! typeset them (KaTeX) and colorize them. They have no methods.

use crate::module::ModuleDescriptor;
use crate::registry::Registry;

/// Module id: KaTeX typesetting of `$…$` / `$$…$$`.
pub const MATH: &str = "math";
/// Module id: syntax highlighting of code blocks.
pub const CODE_HIGHLIGHT: &str = "code_highlight";

fn descriptor(id: &'static str, name_key: &'static str, description_key: &'static str) -> ModuleDescriptor {
    ModuleDescriptor {
        id,
        name_key,
        description_key,
        enabled_by_default: true,
        can_disable: true,
        settings: vec![],
        ui: vec![],
    }
}

/// Registers both modules.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor(
        MATH,
        "modules.math.name",
        "modules.math.description",
    ));
    reg.module(descriptor(
        CODE_HIGHLIGHT,
        "modules.code_highlight.name",
        "modules.code_highlight.description",
    ));
}
