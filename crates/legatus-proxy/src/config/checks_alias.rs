//! The alias check (story 153).
use crate::config::alias::read_aliases;
use crate::config::node::read_nodes;
use crate::config::registry::*;
use crate::config::validate::RegistryCheck;

/// Empty alias, unknown node, repeated node and an empty protocol union.
pub struct AliasCheck;

impl RegistryCheck for AliasCheck {
    fn check(&self, doc: &RawRegistry, out: &mut ValidationReport) {
        let mut scratch = ValidationReport::default();
        let typed = read_nodes(&doc.0, &mut scratch);
        read_aliases(&doc.0, &typed, out);
    }
}
