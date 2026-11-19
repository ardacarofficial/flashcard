//! Attributes (classifications).

use crate::v1::attributes::*;

methods! {
    // ---- attributes (classifications) ----
    /// Attribute definitions.
    AttributesList = "attributes.list", Query, "attributes", "attributes:read", Public, undo: false, ListAttributes => AttributeDefList;
    /// Definitions applicable to a deck (inheritance resolved).
    AttributesEffective = "attributes.effective", Query, "attributes", "attributes:read", Public, undo: false, EffectiveAttributes => EffectiveAttributeList;
    /// Built-in templates.
    AttributesTemplates = "attributes.templates", Query, "attributes", "attributes:read", Public, undo: false, ListAttributeTemplates => AttributeTemplateList;
    /// One definition.
    AttributesGet = "attributes.get", Query, "attributes", "attributes:read", Public, undo: false, AttributeRef => AttributeDefDto;
    /// Defines a classification.
    AttributesDefine = "attributes.define", Command, "attributes", "attributes:write", Public, undo: true, DefineAttribute => AttributeDefDto;
    /// Defines a classification from a built-in template.
    AttributesDefineFromTemplate = "attributes.define_from_template", Command, "attributes", "attributes:write", Public, undo: true, DefineAttributeFromTemplate => AttributeDefDto;
    /// Updates name/required/default/position.
    AttributesUpdate = "attributes.update", Command, "attributes", "attributes:write", Public, undo: true, UpdateAttribute => AttributeDefDto;
    /// Moves a definition (and hides its values) to the trash.
    AttributesDelete = "attributes.delete", Command, "attributes", "attributes:write", Public, undo: true, AttributeRef => AttributeDefDto;
    /// Restores a definition from the trash.
    AttributesRestore = "attributes.restore", Command, "attributes", "attributes:write", Public, undo: true, AttributeRef => AttributeDefDto;
    /// Adds an option.
    AttributesOptionAdd = "attributes.option.add", Command, "attributes", "attributes:write", Public, undo: true, AddAttributeOption => AttributeDefDto;
    /// Renames an option (id unchanged).
    AttributesOptionRename = "attributes.option.rename", Command, "attributes", "attributes:write", Public, undo: true, RenameAttributeOption => AttributeDefDto;
    /// Sets an option color.
    AttributesOptionSetColor = "attributes.option.set_color", Command, "attributes", "attributes:write", Public, undo: true, SetAttributeOptionColor => AttributeDefDto;
    /// Reorders options.
    AttributesOptionReorder = "attributes.option.reorder", Command, "attributes", "attributes:write", Public, undo: true, ReorderAttributeOptions => AttributeDefDto;
    /// Merges one option into another (values move).
    AttributesOptionMerge = "attributes.option.merge", Command, "attributes", "attributes:write", Public, undo: true, MergeAttributeOptions => OptionChangeResult;
    /// Deletes an option (values move to a replacement or are removed).
    AttributesOptionDelete = "attributes.option.delete", Command, "attributes", "attributes:write", Public, undo: true, DeleteAttributeOption => OptionChangeResult;
    /// Internal inverse of option delete/merge: re-inserts the option (id, index) and values.
    AttributesOptionRestore = "attributes.option.restore", Command, "attributes", "attributes:write", Internal, undo: true, RestoreAttributeOption => AttributeDefDto;
    /// Values on a note/card.
    AttributesValuesGet = "attributes.values.get", Query, "attributes", "attributes:read", Public, undo: false, GetAttributeValues => AttributeValues;
    /// Sets/clears values on a note/card.
    AttributesValuesSet = "attributes.values.set", Command, "attributes", "attributes:write", Public, undo: true, SetAttributeValues => AttributeValues;
}
