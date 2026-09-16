# Signs the room session token every websocket member is handed on `Joined` (see
# `server/src/sessions.rs`) -- generated once and held in Terraform state, so it's stable across
# `apply`s rather than a fresh value (and every outstanding session invalidated) on every run.
# `deploy.sh`'s server half pushes it into the `exalted-server-session` Secret; see CLAUDE.md's
# "Server hosting" section for why it can't just be minted in-process at startup instead.
resource "random_password" "session_secret" {
  length  = 64
  special = false
}
