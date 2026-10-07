//! Registry types: error and warning codes, the only constructors of error text, and the
//! loaded record (contract C10). No constructor takes a raw field value, so a secret or a
//! prompt cannot reach a message by type.
use std::fmt;
use yaml_serde::Value;

/// Error codes of contract C10. `InternalCheckFailed` is an addition of story 125 for a
/// check that panicked; it fails the load closed (see `validate.rs`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ErrorCode {
    FileUnreadable,
    FileUnparsable,
    MissingKey,
    UnknownField,
    BadVersion,
    BadType,
    DuplicateName,
    UnknownRef,
    EmptyAlias,
    NoCommonProtocol,
    BadUrl,
    BadSlots,
    ContextMissing,
    BadPatch,
    InlineSecret,
    BadKeyRef,
    SecretUnresolved,
    SecretPermissions,
    HostedNeedsAuth,
    HostedNeedsClientTokens,
    BadSettings,
    BadResponsesFlag,
    BadWarmCapacity,
    InternalCheckFailed,
}

impl ErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCode::FileUnreadable => "file_unreadable",
            ErrorCode::FileUnparsable => "file_unparsable",
            ErrorCode::MissingKey => "missing_key",
            ErrorCode::UnknownField => "unknown_field",
            ErrorCode::BadVersion => "bad_version",
            ErrorCode::BadType => "bad_type",
            ErrorCode::DuplicateName => "duplicate_name",
            ErrorCode::UnknownRef => "unknown_ref",
            ErrorCode::EmptyAlias => "empty_alias",
            ErrorCode::NoCommonProtocol => "no_common_protocol",
            ErrorCode::BadUrl => "bad_url",
            ErrorCode::BadSlots => "bad_slots",
            ErrorCode::ContextMissing => "context_missing",
            ErrorCode::BadPatch => "bad_patch",
            ErrorCode::InlineSecret => "inline_secret",
            ErrorCode::BadKeyRef => "bad_key_ref",
            ErrorCode::SecretUnresolved => "secret_unresolved",
            ErrorCode::SecretPermissions => "secret_permissions",
            ErrorCode::HostedNeedsAuth => "hosted_needs_auth",
            ErrorCode::HostedNeedsClientTokens => "hosted_needs_client_tokens",
            ErrorCode::BadSettings => "bad_settings",
            ErrorCode::BadResponsesFlag => "bad_responses_flag",
            ErrorCode::BadWarmCapacity => "bad_warm_capacity",
            ErrorCode::InternalCheckFailed => "internal_check_failed",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WarningCode {
    EngineVersionUntested,
    FlagMismatch,
    SlotsAboveMeasured,
    ResponsesEngineMismatch,
    HostedIgnoresSlots,
    VllmNoPromptTokensDetails,
    HoldLimitAboveBudget,
    RestartRequired,
    ProfileDamaged,
}

impl WarningCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            WarningCode::EngineVersionUntested => "engine_version_untested",
            WarningCode::FlagMismatch => "flag_mismatch",
            WarningCode::SlotsAboveMeasured => "slots_above_measured",
            WarningCode::ResponsesEngineMismatch => "responses_engine_mismatch",
            WarningCode::HostedIgnoresSlots => "hosted_ignores_slots",
            WarningCode::VllmNoPromptTokensDetails => "vllm_no_prompt_tokens_details",
            WarningCode::HoldLimitAboveBudget => "hold_limit_above_budget",
            WarningCode::RestartRequired => "restart_required",
            WarningCode::ProfileDamaged => "profile_damaged",
        }
    }
}

/// Fixed texts of contract C10.
pub const TEXT_UNKNOWN_FIELD: &str = "Field is not part of the schema.";
pub const TEXT_MISSING_KEY: &str = "Required field is missing.";
pub const TEXT_BAD_VERSION: &str = "Schema version 1 is the only version.";
pub const TEXT_FILE_MISSING: &str = "File does not exist.";
pub const TEXT_FILE_NOT_READABLE: &str = "File is not readable.";
pub const TEXT_FILE_CANNOT_READ: &str = "File cannot be read.";
pub const TEXT_FILE_NOT_UTF8: &str = "File is not valid UTF-8 text.";
pub const TEXT_FILE_NOT_YAML: &str = "File is not valid structured text.";
pub const TEXT_CHECK_PANICKED: &str = "A check failed internally, so the file was not accepted.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistryError {
    pub code: ErrorCode,
    pub path: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistryWarning {
    pub code: WarningCode,
    pub path: String,
    pub text: String,
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "registry error {} at {}: {}", self.code.as_str(), self.path, self.text)
    }
}

impl fmt::Display for RegistryWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "warning {} at {}: {}", self.code.as_str(), self.path, self.text)
    }
}

/// The kind of a found value. Error text names the kind and never the value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FoundKind {
    Missing,
    Null,
    Bool,
    Int,
    Float,
    Str,
    List,
    Map,
}

impl FoundKind {
    pub fn name(&self) -> &'static str {
        match self {
            FoundKind::Missing => "nothing",
            FoundKind::Null => "null",
            FoundKind::Bool => "boolean",
            FoundKind::Int => "integer",
            FoundKind::Float => "number",
            FoundKind::Str => "string",
            FoundKind::List => "list",
            FoundKind::Map => "map",
        }
    }

    pub fn of(value: &Value) -> FoundKind {
        match value {
            Value::Null => FoundKind::Null,
            Value::Bool(_) => FoundKind::Bool,
            Value::Number(n) if n.is_i64() || n.is_u64() => FoundKind::Int,
            Value::Number(_) => FoundKind::Float,
            Value::String(_) => FoundKind::Str,
            Value::Sequence(_) => FoundKind::List,
            Value::Mapping(_) => FoundKind::Map,
            Value::Tagged(tagged) => FoundKind::of(&tagged.value),
        }
    }
}

/// A secret reference name that passed the grammar of story 135. The field is private and
/// only `from_valid_ref` builds one, so a pasted secret cannot become a `SafeRefName`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SafeRefName(String);

impl SafeRefName {
    #[allow(dead_code, reason = "story 135 builds the reference name")]
    pub(crate) fn from_valid_ref(text: &str) -> SafeRefName {
        SafeRefName(text.to_string())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub fn error_wrong_type(path: &str, expected: &'static str, found: FoundKind) -> RegistryError {
    RegistryError { code: ErrorCode::BadType, path: path.to_string(), text: format!("Expected {expected}, found {}.", found.name()) }
}

pub fn error_fixed(code: ErrorCode, path: &str, text: &'static str) -> RegistryError {
    RegistryError { code, path: path.to_string(), text: text.to_string() }
}

pub fn error_with_ref(code: ErrorCode, path: &str, text: &'static str, r: &SafeRefName) -> RegistryError {
    RegistryError { code, path: path.to_string(), text: format!("{text} Reference: {}.", r.as_str()) }
}

/// Error whose text ends with the name of another setting. The name is a `&'static str` taken
/// from the settings catalogue, so a value from the file cannot reach it by type.
pub fn error_names_setting(code: ErrorCode, path: &str, text: &'static str, other: &'static str) -> RegistryError {
    RegistryError { code, path: path.to_string(), text: format!("{text} {other}.") }
}

pub fn warning_fixed(code: WarningCode, path: &str, text: &'static str) -> RegistryWarning {
    RegistryWarning { code, path: path.to_string(), text: text.to_string() }
}

/// The checked value tree: the generic value of the parser, wrapped so a check cannot take
/// any other value.
#[derive(Clone, Debug)]
pub struct RawRegistry(pub Value);

#[derive(Debug, Default)]
pub struct ValidationReport {
    pub errors: Vec<RegistryError>,
    pub warnings: Vec<RegistryWarning>,
}

#[derive(Debug)]
pub struct LoadedRegistry {
    pub raw: RawRegistry,
    pub warnings: Vec<RegistryWarning>,
    /// Typed nodes and machines; story 121 builds the `Registry` from them.
    pub nodes: Vec<crate::config::node::NodeSpec>,
    pub machines: Vec<crate::config::node::MachineSpec>,
    /// Name to alias; empty when the file lists no alias.
    pub aliases: crate::config::alias::AliasTable,
    /// The values in use: the file value or the catalogue default for every setting.
    pub settings: crate::config::settings::EffectiveSettings,
    pub routes: Vec<crate::config::routes::RouteSpec>,
}
