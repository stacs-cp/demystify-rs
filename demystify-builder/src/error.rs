use thiserror::Error;

#[derive(Debug, Error)]
pub enum BuildError {
    #[error("variable name '{0}' is already declared")]
    DuplicateVarName(String),

    #[error("$#CON family '{0}' has no description registered")]
    UnknownFamily(String),

    #[error(
        "guard atom does not belong to family '{expected}' \
         (atom's actual family: {actual:?})"
    )]
    GuardFromWrongFamily {
        expected: String,
        actual: Option<String>,
    },

    #[error(
        "constraint description '{0}' was used for two different constraints — \
         each $#CON description must be unique"
    )]
    DuplicateConstraintDescription(String),

    #[error(
        "a constraint references {got} index dimensions but its declared matrix \
         '{name}' has {expected}"
    )]
    IndexArityMismatch {
        name: String,
        expected: usize,
        got: usize,
    },

    #[error("index {got} is out of range for axis {axis} of '{name}' (range {range})")]
    IndexOutOfRange {
        name: String,
        axis: usize,
        got: i64,
        range: String,
    },

    #[error(
        "model has no `$#SHOW <var> main` directive — required for renderers, \
         add `builder.show(\"<var>\", ShowRole::Main)`"
    )]
    MissingMainShow,

    #[error("$#VAR '{0}' was declared but never referenced in any constraint")]
    UnusedVar(String),

    #[error("$#CON family '{0}' was declared but never used")]
    UnusedFamily(String),

    #[error("$#REVEAL source '{0}' is not a previously declared $#VAR")]
    UnknownRevealSource(String),

    #[error(
        "$#REVEAL target '{0}' is not a previously declared reveal target \
         (call `reveal_bool_matrix` first)"
    )]
    UnknownRevealTarget(String),

    #[error("reveal-target matrix '{0}' was declared but never wired up via `set_reveal`")]
    UnusedRevealTarget(String),

    #[error(
        "reveal cascade for {src_lit} expects atom {target_lit} but no such atom exists — \
         check that the reveal-target matrix's dims are `src.dims ++ [0..=1]`"
    )]
    RevealTargetMissing { src_lit: String, target_lit: String },

    #[error(
        "table tuple has {got} values but the column list has {expected} cells — \
         each forbidden tuple must give one value per column"
    )]
    TableArityMismatch { expected: usize, got: usize },

    #[error("table references value {value} for cell '{cell}', whose domain is {domain}")]
    TableValueNotInDomain {
        value: i64,
        cell: String,
        domain: String,
    },

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<rustsat::OutOfMemory> for BuildError {
    fn from(_: rustsat::OutOfMemory) -> Self {
        BuildError::Other(anyhow::anyhow!("rustsat ran out of memory while encoding"))
    }
}

impl From<rustsat::encodings::EnforceError> for BuildError {
    fn from(e: rustsat::encodings::EnforceError) -> Self {
        BuildError::Other(anyhow::anyhow!("rustsat enforce error: {:?}", e))
    }
}
