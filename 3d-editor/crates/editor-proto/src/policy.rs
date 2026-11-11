use editor_cmd::{CommandError, OpInfo, Request, Safety, required_safety};
use serde_json::Value;

/// Which op classes a client may run. Destructive access implies mutate access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    mutate: bool,
    destructive: bool,
}

impl Policy {
    pub fn new(mutate: bool, destructive: bool) -> Self {
        Self {
            mutate: mutate || destructive,
            destructive,
        }
    }

    pub fn read_only() -> Self {
        Self::new(false, false)
    }

    pub fn allow_all() -> Self {
        Self::new(true, true)
    }

    pub fn allows(&self, safety: Safety) -> bool {
        match safety {
            Safety::Read => true,
            Safety::Mutate => self.mutate,
            Safety::Destructive => self.destructive,
        }
    }

    pub fn allows_op(&self, info: &OpInfo) -> bool {
        self.allows(info.safety)
    }

    fn check_op(&self, op: &str, args: &Value) -> Result<(), CommandError> {
        let Some(needed) = required_safety(op, args) else {
            return Ok(());
        };
        if self.allows(needed) {
            Ok(())
        } else {
            Err(CommandError::NotPermitted {
                op: op.to_string(),
                needs: needed.as_str().to_string(),
            })
        }
    }

    /// Checks the op and, for a batch, every command inside it.
    pub fn check(&self, request: &Request) -> Result<(), CommandError> {
        self.check_op(&request.op, &request.args)?;
        if request.op == "batch"
            && let Some(Value::Array(items)) = request.args.get("commands")
        {
            for item in items {
                if let Some(op) = item.get("op").and_then(Value::as_str) {
                    let args = item.get("args").unwrap_or(&Value::Null);
                    self.check_op(op, args)?;
                }
            }
        }
        Ok(())
    }
}
