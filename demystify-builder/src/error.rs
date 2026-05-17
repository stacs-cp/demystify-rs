use thiserror::Error;

#[derive(Debug, Error)]
pub enum BuildError {
    #[error("variable name '{0}' is already declared")]
    DuplicateVarName(String),

    #[error("$#CON family '{0}' has no description registered")]
    UnknownFamily(String),

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
