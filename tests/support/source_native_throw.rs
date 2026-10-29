//! Private creation provenance. Public legacy exception DTOs remain unchanged.
use crate::source_rich_value::{NativeKind, Realm};
use crate::validity_source::SourceException;
use std::ops::Deref;
use std::sync::Arc;

/// Identity is owned by the actual invocation's error, with no global serial,
/// display-text lookup or address exposed in a receipt. Arc equality is used
/// only to preserve clones/rethrows; output IDs are assigned by the arena.
#[derive(Debug)]
pub(crate) struct NativeCreation {
    constructor: NativeKind,
}
impl NativeCreation {
    pub(crate) fn constructor(&self) -> &NativeKind {
        &self.constructor
    }
}

#[derive(Clone)]
pub(crate) struct CreatedThrown {
    legacy: SourceException,
    creation: Option<Arc<NativeCreation>>,
    realm: Option<Realm>,
}

impl CreatedThrown {
    pub(crate) fn native(
        constructor: NativeKind,
        realm: Realm,
        message: impl Into<String>,
    ) -> Self {
        Self::unscoped_native(constructor, message).at_audited_creation_boundary(realm)
    }
    pub(crate) fn unscoped_native(constructor: NativeKind, message: impl Into<String>) -> Self {
        // This match is the actual constructor factory, never a read of the
        // mutable name property on an already-created or foreign exception.
        let name = match constructor {
            NativeKind::Error => "Error",
            NativeKind::TypeError => "TypeError",
            NativeKind::RangeError => "RangeError",
            NativeKind::SyntaxError => "SyntaxError",
            NativeKind::ReferenceError => "ReferenceError",
            NativeKind::UriError => "URIError",
            NativeKind::EvalError => "EvalError",
            NativeKind::AggregateError => "AggregateError",
        };
        Self {
            legacy: SourceException {
                name: name.into(),
                message: message.into(),
            },
            creation: Some(Arc::new(NativeCreation { constructor })),
            realm: None,
        }
    }
    /// Only an audited actual creation boundary may supply this origin. This
    /// returns a new value and never mutates a shared token or rollback clone.
    /// Generic catches/target names must not call this to guess a realm.
    pub(crate) fn at_audited_creation_boundary(mut self, realm: Realm) -> Self {
        if self.creation.is_some() && self.realm.is_none() {
            self.realm = Some(realm);
        }
        self
    }
    pub(crate) fn realm(&self) -> Option<&Realm> {
        self.realm.as_ref()
    }
    pub(crate) fn creation(&self) -> Option<&Arc<NativeCreation>> {
        self.creation.as_ref()
    }
    pub(crate) fn legacy(&self) -> &SourceException {
        &self.legacy
    }
}

impl From<SourceException> for CreatedThrown {
    fn from(legacy: SourceException) -> Self {
        Self {
            legacy,
            creation: None,
            realm: None,
        }
    }
}
impl From<CreatedThrown> for SourceException {
    fn from(value: CreatedThrown) -> Self {
        value.legacy
    }
}
impl Deref for CreatedThrown {
    type Target = SourceException;
    fn deref(&self) -> &Self::Target {
        &self.legacy
    }
}
// Legacy diagnostics preserve the exact old two-field representation. This
// value is transient, not hidden extra Core state or a pending-job side table.
impl std::fmt::Debug for CreatedThrown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.legacy.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_creation_identity_survives_rethrow_without_text_merging() {
        let first = CreatedThrown::native(NativeKind::Error, Realm::SourceVm, "same display");
        let rethrow = first.clone();
        let second = CreatedThrown::native(NativeKind::Error, Realm::SourceVm, "same display");
        assert!(Arc::ptr_eq(
            first.creation().unwrap(),
            rethrow.creation().unwrap()
        ));
        assert!(!Arc::ptr_eq(
            first.creation().unwrap(),
            second.creation().unwrap()
        ));
        assert_eq!(first.legacy(), second.legacy());
    }
    #[test]
    fn typed_foreign_names_never_create_native_authority() {
        for name in ["Error", "TypeError", "RangeError", "Function", "Object"] {
            let foreign = CreatedThrown::from(SourceException {
                name: name.into(),
                message: "same display".into(),
            });
            assert!(foreign.creation().is_none());
        }
    }
    #[test]
    fn native_constructor_realm_and_legacy_projection_are_explicit() {
        let host = CreatedThrown::native(NativeKind::Error, Realm::HostIo, "recorded");
        let source = CreatedThrown::native(NativeKind::TypeError, Realm::SourceVm, "recorded");
        assert_eq!(host.creation().unwrap().constructor, NativeKind::Error);
        assert_eq!(host.realm(), Some(&Realm::HostIo));
        assert_eq!(
            source.creation().unwrap().constructor,
            NativeKind::TypeError
        );
        assert_eq!(source.realm(), Some(&Realm::SourceVm));
        assert_eq!(
            format!("{host:?}"),
            format!("{:?}", SourceException::from(host.clone()))
        );
        assert_eq!(
            SourceException::from(source),
            SourceException {
                name: "TypeError".into(),
                message: "recorded".into()
            }
        );
    }
    #[test]
    fn local_refusal_cannot_bind_the_retained_before_state_through_an_alias() {
        let before = CreatedThrown::unscoped_native(NativeKind::Error, "same display");
        let attempted = before.clone().at_audited_creation_boundary(Realm::HostIo);
        assert!(Arc::ptr_eq(
            before.creation().unwrap(),
            attempted.creation().unwrap()
        ));
        assert_eq!(attempted.realm(), Some(&Realm::HostIo));
        // Simulated local refusal discards only the attempted invocation value.
        drop(attempted);
        assert_eq!(before.realm(), None);
        let retained = before.clone();
        let bound = before.at_audited_creation_boundary(Realm::SourceVm);
        assert_eq!(retained.realm(), None);
        assert_eq!(bound.realm(), Some(&Realm::SourceVm));
        let rethrow = bound.clone().at_audited_creation_boundary(Realm::HostIo);
        assert_eq!(rethrow.realm(), Some(&Realm::SourceVm));
        assert!(Arc::ptr_eq(
            bound.creation().unwrap(),
            rethrow.creation().unwrap()
        ));
        let opaque = CreatedThrown::from(SourceException {
            name: "TypeError".into(),
            message: "same display".into(),
        })
        .at_audited_creation_boundary(Realm::SourceVm);
        assert!(opaque.creation().is_none());
        assert_eq!(opaque.realm(), None);
    }
}
