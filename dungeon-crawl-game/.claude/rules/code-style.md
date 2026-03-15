Prefer no comments or terse comments. Only use comments when the code is actually complicated.

Prefer returning `Result` or `Option` over `unwrap` or `expect`. If no obvious error type already exists use `anyhow::Result`.