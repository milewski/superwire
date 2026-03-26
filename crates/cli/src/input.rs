use crate::diagnostics::CommandError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecretAssignment {
    pub name: String,
    pub value: String,
}

impl SecretAssignment {
    pub fn parse(raw_secret_assignment: &str) -> Result<Self, CommandError> {
        let maybe_assignment_parts = raw_secret_assignment.split_once('=');

        let Some((raw_name, raw_value)) = maybe_assignment_parts else {
            return Err(CommandError::invalid_workflow("secret must use NAME=VALUE format"));
        };

        let secret_name = raw_name.trim().to_owned();
        let secret_value = raw_value.trim().to_owned();

        if secret_name.is_empty() {
            return Err(CommandError::invalid_workflow("secret name cannot be empty"));
        }

        Ok(Self {
            name: secret_name,
            value: secret_value,
        })
    }
}
