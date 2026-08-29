//! DAL for `turns`, and the writer side of `messages`/`tool_calls`.
//!
//! See `lib::db`'s module doc for the two invariants this file exists to
//! uphold: `messages` never holds an unresolved `tool_use`, and a suspended
//! turn's only resume point is a byte-exact `AgentTurn` JSON round trip.

use std::collections::HashMap;

use shared::agent::turn::{
    AgentTurn, DecidedBy, Decision, ToolApprovalRequest, ToolDecision, ToolDecisionRecord, TurnStop,
};
use shared::error::ErrorReport;
use shared::ids::{ConversationId, MessageId, TurnId};
use shared::llm::message::{ContentBlock, Message, Role};
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::error::DbError;
use super::{now_iso8601, write_tx, Db, Result};

/// Open a new turn: write `user_message` (if any) and a `state='running'`
/// row, in one transaction. Errors [`DbError::ConversationBusy`] if a turn is
/// already `running`/`awaiting_approval` for this conversation —
/// `turns_one_open_per_conversation` is the durable backstop behind this
/// check; this query is what turns that into a clean typed error instead of
/// a raw constraint violation.
pub async fn begin(db: &Db, conversation_id: ConversationId, user_message: Option<&Message>) -> Result<TurnId> {
    let now = now_iso8601();
    let mut tx = write_tx(db.pool()).await?;

    let already_open: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM turns WHERE conversation_id = ? AND state IN ('running','awaiting_approval')",
    )
    .bind(conversation_id.get())
    .fetch_optional(&mut *tx)
    .await?;
    if already_open.is_some() {
        return Err(DbError::ConversationBusy {
            conversation_id: conversation_id.get(),
        });
    }

    if let Some(message) = user_message {
        append_message(&mut tx, conversation_id, None, message, &now).await?;
    }

    let ordinal: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(ordinal), 0) + 1 FROM turns WHERE conversation_id = ?")
        .bind(conversation_id.get())
        .fetch_one(&mut *tx)
        .await?;

    let turn_id: i64 = sqlx::query_scalar(
        "INSERT INTO turns (conversation_id, ordinal, state, steps, started_at) \
         VALUES (?, ?, 'running', 0, ?) RETURNING id",
    )
    .bind(conversation_id.get())
    .bind(ordinal)
    .bind(now.clone())
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query("UPDATE conversations SET updated_at = ?, last_message_at = ? WHERE id = ?")
        .bind(now.clone())
        .bind(now)
        .bind(conversation_id.get())
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(TurnId(turn_id))
}

/// The terminal write for a turn, in one transaction.
///
/// `Done`/`MaxSteps`: append every message `turn.added` holds beyond what's
/// already persisted, with contiguous `seq`; settle the turn row; rebuild
/// this turn's `tool_calls` from the newly-real message ids; bump the
/// conversation's timestamps.
///
/// `AwaitingApproval`: write **nothing** to `messages` — see this module's
/// doc — only the turn row (with its `turn_json` resume point) and a
/// `tool_calls` projection built from `pending`/`completed` directly, since
/// there are no message rows to derive it from yet.
pub async fn settle(db: &Db, turn_id: TurnId, turn: &AgentTurn) -> Result<()> {
    let conversation_id = conversation_id_for_turn(db.pool(), turn_id).await?;
    let now = now_iso8601();
    let mut tx = write_tx(db.pool()).await?;

    match &turn.stop {
        TurnStop::Done { stop_reason } => {
            let stop_reason_json = serde_json::to_string(stop_reason).expect("StopReason always serializes");
            let written = append_new_messages(&mut tx, conversation_id, turn_id, &turn.conversation.messages, &now).await?;
            update_turn_terminal(&mut tx, turn_id, "done", turn, Some(&stop_reason_json), &now).await?;
            rebuild_tool_calls(&mut tx, turn_id, conversation_id, ToolCallSource::Messages(&written)).await?;
            write_decisions(&mut tx, turn_id, &turn.decisions).await?;
            touch_conversation(&mut tx, conversation_id, &now, true).await?;
        }
        TurnStop::MaxSteps => {
            let written = append_new_messages(&mut tx, conversation_id, turn_id, &turn.conversation.messages, &now).await?;
            update_turn_terminal(&mut tx, turn_id, "max_steps", turn, None, &now).await?;
            rebuild_tool_calls(&mut tx, turn_id, conversation_id, ToolCallSource::Messages(&written)).await?;
            write_decisions(&mut tx, turn_id, &turn.decisions).await?;
            touch_conversation(&mut tx, conversation_id, &now, true).await?;
        }
        TurnStop::AwaitingApproval { pending, completed } => {
            let turn_json = serde_json::to_string(turn).expect("AgentTurn always serializes");
            sqlx::query(
                "UPDATE turns SET state = 'awaiting_approval', turn_json = ?, steps = ?, \
                 input_tokens = ?, output_tokens = ?, cache_read_tokens = ?, cache_write_tokens = ? \
                 WHERE id = ?",
            )
            .bind(turn_json)
            .bind(turn.steps as i64)
            .bind(turn.usage.input_tokens as i64)
            .bind(turn.usage.output_tokens as i64)
            .bind(turn.usage.cache_read_input_tokens.map(|v| v as i64))
            .bind(turn.usage.cache_creation_input_tokens.map(|v| v as i64))
            .bind(turn_id.get())
            .execute(&mut *tx)
            .await?;

            let assistant = turn.added.last().expect(
                "a turn suspended on AwaitingApproval always ends with the assistant \
                 message that requested it",
            );
            rebuild_tool_calls(
                &mut tx,
                turn_id,
                conversation_id,
                ToolCallSource::Pending {
                    assistant,
                    pending,
                    completed,
                },
            )
            .await?;
            // Only reaches rows `rebuild_tool_calls` above just created or
            // already had — a decision from an earlier, now-superseded step
            // may still have no row at all (see this file's module doc);
            // it's a harmless zero-row UPDATE here and self-heals the next
            // time `settle` runs over the full decision log, which it
            // always carries forward in its entirety, never just the delta.
            write_decisions(&mut tx, turn_id, &turn.decisions).await?;
            touch_conversation(&mut tx, conversation_id, &now, false).await?;
        }
    }

    tx.commit().await?;
    Ok(())
}

/// Mirrors [`shared::agent::AgentTurn::decisions`] onto `tool_calls`. Also
/// (re-)writes `decision` itself, not just `decided_by`/`decision_reason`/
/// `decided_at`: a *policy* decision never goes through [`reopen`] — there is
/// no suspend/resume for a call the policy resolved on its own — so this is
/// the only writer `decision` ever gets for one. For a *user* decision,
/// `reopen` already wrote `decision` the moment they answered; writing it
/// again here is an idempotent no-op with the same value, not a second
/// disagreeing writer (see `sql/0005_tool_decisions.sql`'s doc for why the
/// other three columns stay single-writer, here alone). A decision naming a
/// call that has no `tool_calls` row yet updates zero rows and is silently
/// skipped; it is re-applied, from the same still-accumulating `decisions`
/// list, on every later `settle` for this turn.
async fn write_decisions(
    tx: &mut Transaction<'_, Sqlite>,
    turn_id: TurnId,
    decisions: &[ToolDecisionRecord],
) -> Result<()> {
    for record in decisions {
        let decision_text = match record.decision {
            Decision::Approve => "approve",
            Decision::Deny { .. } => "deny",
        };
        let decided_by = match &record.decided_by {
            DecidedBy::User => "user",
            DecidedBy::Policy { .. } => "policy",
        };
        let reason = match &record.decided_by {
            DecidedBy::User => None,
            DecidedBy::Policy { reason } => Some(reason.clone()),
        };
        sqlx::query(
            "UPDATE tool_calls SET decision = ?, decided_by = ?, decision_reason = ?, decided_at = ? \
             WHERE turn_id = ? AND tool_use_id = ?",
        )
        .bind(decision_text)
        .bind(decided_by)
        .bind(reason)
        .bind(record.decided_at.clone())
        .bind(turn_id.get())
        .bind(record.tool_use_id.clone())
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// The turn failed outright. No message from it is ever written — a
/// truncated assistant message is not replayable (it can end mid-`ToolUse`,
/// or carry a `Thinking` block whose `signature` never arrived), so
/// persisting it would leave a conversation that 400s on the next send with
/// nothing in the schema marking it as poisoned.
pub async fn fail(db: &Db, turn_id: TurnId, error: &ErrorReport) -> Result<()> {
    let now = now_iso8601();
    let error_json = serde_json::to_string(error).expect("ErrorReport always serializes");
    let mut tx = write_tx(db.pool()).await?;
    sqlx::query(
        "UPDATE turns SET state = 'failed', turn_json = NULL, error_json = ?, finished_at = ? WHERE id = ?",
    )
    .bind(error_json)
    .bind(now)
    .bind(turn_id.get())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn load_pending(db: &Db, conversation_id: ConversationId) -> Result<Option<(TurnId, AgentTurn)>> {
    let row: Option<(i64, Option<String>)> = sqlx::query_as(
        "SELECT id, turn_json FROM turns WHERE conversation_id = ? AND state = 'awaiting_approval'",
    )
    .bind(conversation_id.get())
    .fetch_optional(db.pool())
    .await?;
    let Some((id, turn_json)) = row else {
        return Ok(None);
    };
    let turn = decode_turn_json(id, turn_json)?;
    Ok(Some((TurnId(id), turn)))
}

/// Every gated call this conversation has ever resolved, across every turn —
/// for display, never for control flow (see `sql/0001_init.sql`'s doc on
/// `tool_calls`: `ConversationView.pending` alone decides what's still
/// awaiting approval). Ordered by `created_at` so a turn's calls come back in
/// the order the model asked for them.
pub async fn decisions_for_conversation(
    db: &Db,
    conversation_id: ConversationId,
) -> Result<Vec<shared::conversation::ToolDecisionView>> {
    let rows: Vec<(i64, String, String, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT turn_id, tool_use_id, decision, decided_by, decision_reason, decided_at \
         FROM tool_calls WHERE conversation_id = ? AND decision IS NOT NULL \
         ORDER BY turn_id, created_at",
    )
    .bind(conversation_id.get())
    .fetch_all(db.pool())
    .await?;

    rows.into_iter()
        .map(|(turn_id, tool_use_id, decision_text, decided_by, decision_reason, decided_at)| {
            let decision = match decision_text.as_str() {
                "approve" => Decision::Approve,
                _ => Decision::Deny { reason: decision_reason.clone() },
            };
            let decided_by = decided_by.map(|by| match by.as_str() {
                "policy" => DecidedBy::Policy {
                    reason: decision_reason.clone().unwrap_or_default(),
                },
                _ => DecidedBy::User,
            });
            Ok(shared::conversation::ToolDecisionView {
                turn_id: TurnId(turn_id),
                tool_use_id,
                decision,
                decided_by,
                decided_at,
            })
        })
        .collect()
}

/// Flip a suspended turn back to `running` and record each decision on its
/// `tool_calls` row. `turn_json` is deliberately **left in place**: it is the
/// only resume point, and clearing it before the resumed stream settles
/// would mean a crash mid-resume discarded the whole turn, tool results it
/// already holds included. [`super::reap_orphaned`] reverts such a row back
/// to `awaiting_approval`.
pub async fn reopen(
    db: &Db,
    conversation_id: ConversationId,
    decisions: &[ToolDecision],
) -> Result<(TurnId, AgentTurn)> {
    let mut tx = write_tx(db.pool()).await?;

    let row: Option<(i64, Option<String>)> = sqlx::query_as(
        "SELECT id, turn_json FROM turns WHERE conversation_id = ? AND state = 'awaiting_approval'",
    )
    .bind(conversation_id.get())
    .fetch_optional(&mut *tx)
    .await?;
    let Some((id, turn_json)) = row else {
        return Err(DbError::NotFound {
            entity: "pending_turn",
            id: conversation_id.to_string(),
        });
    };
    let turn = decode_turn_json(id, turn_json)?;

    sqlx::query("UPDATE turns SET state = 'running' WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    for decision in decisions {
        let decision_text = match decision.decision {
            Decision::Approve => "approve",
            Decision::Deny { .. } => "deny",
        };
        sqlx::query("UPDATE tool_calls SET decision = ? WHERE turn_id = ? AND tool_use_id = ?")
            .bind(decision_text)
            .bind(id)
            .bind(decision.tool_use_id.clone())
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok((TurnId(id), turn))
}

/// Discard a suspended turn. Needs no repair: nothing from it was ever
/// written to `messages`, so the conversation is exactly what it was before
/// the turn started. Cascades to the turn's (temporary, `message_id IS NULL`)
/// `tool_calls` rows.
pub async fn abandon(db: &Db, conversation_id: ConversationId) -> Result<()> {
    let result = sqlx::query("DELETE FROM turns WHERE conversation_id = ? AND state = 'awaiting_approval'")
        .bind(conversation_id.get())
        .execute(db.pool())
        .await?;
    if result.rows_affected() == 0 {
        return Err(DbError::NotFound {
            entity: "pending_turn",
            id: conversation_id.to_string(),
        });
    }
    Ok(())
}

fn decode_turn_json(turn_id: i64, turn_json: Option<String>) -> Result<AgentTurn> {
    let turn_json = turn_json.ok_or_else(|| DbError::NotFound {
        entity: "pending_turn_json",
        id: turn_id.to_string(),
    })?;
    serde_json::from_str(&turn_json).map_err(|source| DbError::Json {
        table: "turns",
        column: "turn_json",
        entity: "turn",
        id: turn_id.to_string(),
        source,
    })
}

async fn conversation_id_for_turn(pool: &SqlitePool, turn_id: TurnId) -> Result<ConversationId> {
    let id: Option<i64> = sqlx::query_scalar("SELECT conversation_id FROM turns WHERE id = ?")
        .bind(turn_id.get())
        .fetch_optional(pool)
        .await?;
    id.map(ConversationId).ok_or_else(|| DbError::NotFound {
        entity: "turn",
        id: turn_id.to_string(),
    })
}

async fn append_message(
    tx: &mut Transaction<'_, Sqlite>,
    conversation_id: ConversationId,
    turn_id: Option<TurnId>,
    message: &Message,
    now: &str,
) -> Result<MessageId> {
    let seq: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(seq), -1) + 1 FROM messages WHERE conversation_id = ?")
        .bind(conversation_id.get())
        .fetch_one(&mut **tx)
        .await?;
    let role = match message.role {
        Role::User => "user",
        Role::Assistant => "assistant",
    };
    let content_json = serde_json::to_string(&message.content).expect("Vec<ContentBlock> always serializes");
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO messages (conversation_id, seq, turn_id, role, content, created_at) \
         VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(conversation_id.get())
    .bind(seq)
    .bind(turn_id.map(TurnId::get))
    .bind(role)
    .bind(content_json)
    .bind(now.to_string())
    .fetch_one(&mut **tx)
    .await?;
    Ok(MessageId(id))
}

/// Appends every message in `all_messages` beyond however many are already
/// persisted for this conversation. `Agent` only ever appends to
/// `conversation.messages`, so "already persisted" is just a count.
async fn append_new_messages(
    tx: &mut Transaction<'_, Sqlite>,
    conversation_id: ConversationId,
    turn_id: TurnId,
    all_messages: &[Message],
    now: &str,
) -> Result<Vec<(MessageId, Message)>> {
    let persisted_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE conversation_id = ?")
        .bind(conversation_id.get())
        .fetch_one(&mut **tx)
        .await?;
    let mut written = Vec::new();
    for message in &all_messages[persisted_count as usize..] {
        let id = append_message(tx, conversation_id, Some(turn_id), message, now).await?;
        written.push((id, message.clone()));
    }
    Ok(written)
}

async fn update_turn_terminal(
    tx: &mut Transaction<'_, Sqlite>,
    turn_id: TurnId,
    state: &str,
    turn: &AgentTurn,
    stop_reason_json: Option<&str>,
    now: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE turns SET state = ?, turn_json = NULL, stop_reason = ?, steps = ?, \
         input_tokens = ?, output_tokens = ?, cache_read_tokens = ?, cache_write_tokens = ?, \
         finished_at = ? WHERE id = ?",
    )
    .bind(state.to_string())
    .bind(stop_reason_json.map(str::to_string))
    .bind(turn.steps as i64)
    .bind(turn.usage.input_tokens as i64)
    .bind(turn.usage.output_tokens as i64)
    .bind(turn.usage.cache_read_input_tokens.map(|v| v as i64))
    .bind(turn.usage.cache_creation_input_tokens.map(|v| v as i64))
    .bind(now.to_string())
    .bind(turn_id.get())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn touch_conversation(
    tx: &mut Transaction<'_, Sqlite>,
    conversation_id: ConversationId,
    now: &str,
    bump_last_message_at: bool,
) -> Result<()> {
    if bump_last_message_at {
        sqlx::query("UPDATE conversations SET updated_at = ?, last_message_at = ? WHERE id = ?")
            .bind(now.to_string())
            .bind(now.to_string())
            .bind(conversation_id.get())
            .execute(&mut **tx)
            .await?;
    } else {
        sqlx::query("UPDATE conversations SET updated_at = ? WHERE id = ?")
            .bind(now.to_string())
            .bind(conversation_id.get())
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

/// Where [`rebuild_tool_calls`] draws its entries from.
pub(crate) enum ToolCallSource<'a> {
    /// A turn that just settled `Done`/`MaxSteps`: every message this settle
    /// call appended, each already assigned a real [`MessageId`], in order.
    /// Both the requesting `ToolUse` and its `ToolResult` are somewhere in
    /// this list, since a terminal settle only ever runs after every step of
    /// the turn (suspend/resume cycles included) has produced its messages.
    Messages(&'a [(MessageId, Message)]),
    /// A turn that just suspended: nothing is in `messages` yet. `assistant`
    /// is `turn.added`'s last message — the one carrying every `ToolUse`
    /// block this step asked for, both the ones now `pending` and the ones
    /// already run (`completed`).
    Pending {
        assistant: &'a Message,
        pending: &'a [ToolApprovalRequest],
        completed: &'a [ContentBlock],
    },
}

struct ToolCallEntry {
    tool_use_id: String,
    message_id: Option<MessageId>,
    block_index: Option<i64>,
    name: String,
    input_json: String,
    state: &'static str,
    result_json: Option<String>,
}

/// Recompute one turn's `tool_calls` rows from its authoritative sources
/// (`messages.content` once terminal, `pending`/`completed` while
/// suspended). Upserts by `(turn_id, tool_use_id)`, and its `DO UPDATE SET`
/// deliberately omits **five** columns so they always survive a rebuild:
/// `decision` (set only by [`reopen`]), `decided_by`/`decision_reason`/
/// `decided_at` (set only by [`write_decisions`], called from [`settle`]
/// right after this function), and `created_at` (so the first time a call is
/// seen is when it's dated). Completing that `SET` list to "cover every
/// column" would silently delete every gated call's provenance — see
/// `sql/0001_init.sql`'s doc on `tool_calls`.
pub(crate) async fn rebuild_tool_calls(
    tx: &mut Transaction<'_, Sqlite>,
    turn_id: TurnId,
    conversation_id: ConversationId,
    source: ToolCallSource<'_>,
) -> Result<()> {
    let entries = match source {
        ToolCallSource::Messages(messages) => entries_from_messages(messages),
        ToolCallSource::Pending {
            assistant,
            pending,
            completed,
        } => entries_from_pending(assistant, pending, completed),
    };
    let now = now_iso8601();
    for entry in entries {
        let resolved_at = entry.result_json.as_ref().map(|_| now.clone());
        sqlx::query(
            "INSERT INTO tool_calls \
                (turn_id, tool_use_id, conversation_id, message_id, block_index, name, input_json, \
                 state, result_json, created_at, resolved_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(turn_id, tool_use_id) DO UPDATE SET \
                message_id = excluded.message_id, \
                block_index = excluded.block_index, \
                name = excluded.name, \
                input_json = excluded.input_json, \
                state = excluded.state, \
                result_json = excluded.result_json, \
                resolved_at = excluded.resolved_at",
        )
        .bind(turn_id.get())
        .bind(entry.tool_use_id)
        .bind(conversation_id.get())
        .bind(entry.message_id.map(MessageId::get))
        .bind(entry.block_index)
        .bind(entry.name)
        .bind(entry.input_json)
        .bind(entry.state)
        .bind(entry.result_json)
        .bind(now.clone())
        .bind(resolved_at)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

fn entries_from_messages(messages: &[(MessageId, Message)]) -> Vec<ToolCallEntry> {
    let mut by_id: HashMap<String, ToolCallEntry> = HashMap::new();
    for (message_id, message) in messages {
        if message.role != Role::Assistant {
            continue;
        }
        for (index, block) in message.content.iter().enumerate() {
            if let ContentBlock::ToolUse { id, name, input } = block {
                by_id.insert(
                    id.clone(),
                    ToolCallEntry {
                        tool_use_id: id.clone(),
                        message_id: Some(*message_id),
                        block_index: Some(index as i64),
                        name: name.clone(),
                        input_json: serde_json::to_string(input).expect("tool input always serializes"),
                        state: "pending",
                        result_json: None,
                    },
                );
            }
        }
    }
    for (_message_id, message) in messages {
        for block in &message.content {
            if let ContentBlock::ToolResult {
                tool_use_id,
                is_error,
                ..
            } = block
            {
                if let Some(entry) = by_id.get_mut(tool_use_id) {
                    entry.state = if *is_error { "error" } else { "ok" };
                    entry.result_json =
                        Some(serde_json::to_string(block).expect("ContentBlock always serializes"));
                }
            }
        }
    }
    by_id.into_values().collect()
}

fn entries_from_pending(
    assistant: &Message,
    pending: &[ToolApprovalRequest],
    completed: &[ContentBlock],
) -> Vec<ToolCallEntry> {
    let mut tool_use_lookup: HashMap<&str, (i64, &str, &serde_json::Value)> = HashMap::new();
    for (index, block) in assistant.content.iter().enumerate() {
        if let ContentBlock::ToolUse { id, name, input } = block {
            tool_use_lookup.insert(id.as_str(), (index as i64, name.as_str(), input));
        }
    }

    let mut entries = Vec::with_capacity(pending.len() + completed.len());
    for p in pending {
        entries.push(ToolCallEntry {
            tool_use_id: p.tool_use_id.clone(),
            message_id: None,
            block_index: tool_use_lookup.get(p.tool_use_id.as_str()).map(|(i, ..)| *i),
            name: p.name.clone(),
            input_json: serde_json::to_string(&p.input).expect("tool input always serializes"),
            state: "pending",
            result_json: None,
        });
    }
    for block in completed {
        let ContentBlock::ToolResult {
            tool_use_id,
            is_error,
            ..
        } = block
        else {
            continue;
        };
        let (block_index, name, input) = tool_use_lookup
            .get(tool_use_id.as_str())
            .copied()
            .unwrap_or((-1, "unknown", &serde_json::Value::Null));
        entries.push(ToolCallEntry {
            tool_use_id: tool_use_id.clone(),
            message_id: None,
            block_index: (block_index >= 0).then_some(block_index),
            name: name.to_string(),
            input_json: serde_json::to_string(input).expect("tool input always serializes"),
            state: if *is_error { "error" } else { "ok" },
            result_json: Some(serde_json::to_string(block).expect("ContentBlock always serializes")),
        });
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::agent::AgentConfigInput;
    use shared::llm::message::Usage;
    use shared::llm::model::ModelRef;
    use shared::llm::message::StopReason;
    use shared::llm::tool::Thinking;

    async fn seed_conversation(db: &Db) -> ConversationId {
        let agent = super::super::agents::create(
            db,
            &AgentConfigInput {
                name: "assistant".to_string(),
                description: None,
                model: ModelRef::new("anthropic", "claude-opus-5"),
                system: vec![],
                max_tokens: 1024,
                tools: vec![],
                tool_choice: None,
                thinking: Thinking::default(),
                stop_sequences: vec![],
                max_steps: 8,
            },
        )
        .await
        .unwrap();
        super::super::conversations::create(db, &agent, None, None).await.unwrap().id
    }

    fn user_text(text: &str) -> Message {
        Message::user(vec![ContentBlock::Text { text: text.to_string() }])
    }

    fn assistant_text(text: &str) -> Message {
        Message::assistant(vec![ContentBlock::Text { text: text.to_string() }])
    }

    #[tokio::test]
    async fn begin_writes_the_user_message_and_a_running_row() {
        let db = Db::in_memory().await.unwrap();
        let conv = seed_conversation(&db).await;
        let msg = user_text("hi");
        let turn_id = begin(&db, conv, Some(&msg)).await.unwrap();

        let messages = super::super::conversations::load_messages(&db, conv).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, Role::User);

        let state: String = sqlx::query_scalar("SELECT state FROM turns WHERE id = ?")
            .bind(turn_id.get())
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(state, "running");
    }

    #[tokio::test]
    async fn a_second_begin_on_the_same_conversation_is_conversation_busy() {
        let db = Db::in_memory().await.unwrap();
        let conv = seed_conversation(&db).await;
        begin(&db, conv, Some(&user_text("first"))).await.unwrap();
        let err = begin(&db, conv, Some(&user_text("second"))).await.unwrap_err();
        assert!(matches!(err, DbError::ConversationBusy { .. }));
    }

    #[tokio::test]
    async fn settle_done_appends_added_messages_and_clears_turn_json() {
        let db = Db::in_memory().await.unwrap();
        let conv = seed_conversation(&db).await;
        let turn_id = begin(&db, conv, Some(&user_text("hi"))).await.unwrap();

        let conversation = super::super::conversations::load_conversation(&db, conv).await.unwrap();
        let mut full = conversation.clone();
        let reply = assistant_text("hello!");
        full.messages.push(reply.clone());

        let turn = AgentTurn {
            conversation: full,
            added: vec![reply],
            usage: Usage { input_tokens: 5, output_tokens: 3, ..Default::default() },
            steps: 1,
            stop: TurnStop::Done { stop_reason: StopReason::EndTurn },
            decisions: vec![],
        };
        settle(&db, turn_id, &turn).await.unwrap();

        let messages = super::super::conversations::load_messages(&db, conv).await.unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].role, Role::Assistant);

        let (state, turn_json): (String, Option<String>) =
            sqlx::query_as("SELECT state, turn_json FROM turns WHERE id = ?")
                .bind(turn_id.get())
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(state, "done");
        assert!(turn_json.is_none());
    }

    #[tokio::test]
    async fn settle_awaiting_approval_writes_zero_messages() {
        let db = Db::in_memory().await.unwrap();
        let conv = seed_conversation(&db).await;
        let turn_id = begin(&db, conv, Some(&user_text("run it"))).await.unwrap();

        let conversation = super::super::conversations::load_conversation(&db, conv).await.unwrap();
        let mut full = conversation.clone();
        let tool_call = assistant_text_with_tool_use("call_1_0", "deploy", serde_json::json!({}));
        full.messages.push(tool_call.clone());

        let turn = AgentTurn {
            conversation: full,
            added: vec![tool_call],
            usage: Usage::default(),
            steps: 1,
            stop: TurnStop::AwaitingApproval {
                pending: vec![ToolApprovalRequest {
                    tool_use_id: "call_1_0".to_string(),
                    name: "deploy".to_string(),
                    input: serde_json::json!({}),
                }],
                completed: vec![],
            },
            decisions: vec![],
        };
        settle(&db, turn_id, &turn).await.unwrap();

        let before = super::super::conversations::load_messages(&db, conv).await.unwrap().len();
        assert_eq!(before, 1, "only the user message should be persisted");

        let pending_count: i64 = sqlx::query_scalar("SELECT count(*) FROM tool_calls WHERE state = 'pending'")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(pending_count, 1);
    }

    fn assistant_text_with_tool_use(id: &str, name: &str, input: serde_json::Value) -> Message {
        Message::assistant(vec![ContentBlock::ToolUse {
            id: id.to_string(),
            name: name.to_string(),
            input,
        }])
    }

    #[tokio::test]
    async fn suspend_then_resume_round_trips_through_the_real_agent_without_conversation_mismatch() {
        use crate::agent::tool::{tool, ToolOutput};
        use crate::llm::router::Router;
        use crate::llm::testing::ScriptedProvider;
        use shared::llm::message::{CompletedMessage, StopReason as SR};
        use shared::llm::model::ModelRef as MR;

        #[derive(serde::Deserialize, schemars::JsonSchema)]
        struct DeployArgs {}

        let db = Db::in_memory().await.unwrap();
        let conv = seed_conversation(&db).await;

        let scripted = ScriptedProvider::new(vec![
            Ok(CompletedMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "toolu_1".to_string(),
                    name: "deploy".to_string(),
                    input: serde_json::json!({}),
                }],
                stop_reason: SR::ToolUse,
                usage: Default::default(),
                model: Some("scripted-model".to_string()),
            }),
            // The model's second call, made once `resume` sends the approved
            // tool's result back — the turn ends here.
            Ok(CompletedMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::Text {
                    text: "Deployed.".to_string(),
                }],
                stop_reason: SR::EndTurn,
                usage: Default::default(),
                model: Some("scripted-model".to_string()),
            }),
        ]);
        let mut router = Router::new();
        router.register(
            "scripted",
            crate::llm::router::ProviderEntry {
                chat: Some(std::sync::Arc::new(scripted)),
                ..Default::default()
            },
        );
        let deploy_tool = tool("deploy", "Deploy to prod", |_args: DeployArgs| async move {
            Ok::<_, anyhow::Error>(ToolOutput::from("deployed"))
        })
        .requiring_approval();
        let agent = crate::agent::Agent::builder(MR::new("scripted", "test-model"), 1024)
            .tool(deploy_tool)
            .build(&router)
            .unwrap();

        let user_message = user_text("deploy please");
        let turn_id = begin(&db, conv, Some(&user_message)).await.unwrap();
        let conversation = super::super::conversations::load_conversation(&db, conv).await.unwrap();
        let turn = agent.next_turn(conversation).await.unwrap();
        assert!(matches!(turn.stop, TurnStop::AwaitingApproval { .. }));
        settle(&db, turn_id, &turn).await.unwrap();

        let (loaded_turn_id, loaded_turn) = load_pending(&db, conv).await.unwrap().unwrap();
        assert_eq!(loaded_turn_id, turn_id);
        assert_eq!(
            serde_json::to_value(&loaded_turn).unwrap(),
            serde_json::to_value(&turn).unwrap()
        );

        // `rewrite_tool_use_ids` replaces the provider's raw id ("toolu_1")
        // with `call_{step}_{ordinal}` before it's ever exposed — `pending`
        // carries that rewritten id, which is what must be used everywhere
        // from here on (the decision, the resume, and the `tool_calls` row).
        let rewritten_tool_use_id = match &loaded_turn.stop {
            TurnStop::AwaitingApproval { pending, .. } => pending[0].tool_use_id.clone(),
            _ => panic!("expected AwaitingApproval"),
        };
        let decisions = vec![ToolDecision {
            tool_use_id: rewritten_tool_use_id.clone(),
            decision: Decision::Approve,
        }];
        let (reopened_id, reopened_turn) = reopen(&db, conv, &decisions).await.unwrap();
        assert_eq!(reopened_id, turn_id);

        // The single assertion this whole test exists for: resuming a
        // round-tripped turn must not raise ConversationMismatch.
        let final_turn = agent.resume(reopened_turn, &decisions).await.unwrap();
        assert!(matches!(final_turn.stop, TurnStop::Done { .. }));
        settle(&db, turn_id, &final_turn).await.unwrap();

        let messages = super::super::conversations::load_messages(&db, conv).await.unwrap();
        // user + assistant(tool_use) + user(tool_result) + assistant(final text)
        // == 4, written exactly once (not double-written across the suspend).
        assert_eq!(messages.len(), 4, "messages: {messages:?}");

        let decision_recorded: Option<String> = sqlx::query_scalar(
            "SELECT decision FROM tool_calls WHERE turn_id = ? AND tool_use_id = ?",
        )
        .bind(turn_id.get())
        .bind(rewritten_tool_use_id)
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(decision_recorded.as_deref(), Some("approve"));
    }

    #[tokio::test]
    async fn abandon_leaves_the_conversation_exactly_as_it_was() {
        let db = Db::in_memory().await.unwrap();
        let conv = seed_conversation(&db).await;
        let turn_id = begin(&db, conv, Some(&user_text("hi"))).await.unwrap();
        let conversation = super::super::conversations::load_conversation(&db, conv).await.unwrap();
        let mut full = conversation.clone();
        let tool_call = assistant_text_with_tool_use("call_1_0", "deploy", serde_json::json!({}));
        full.messages.push(tool_call.clone());
        let turn = AgentTurn {
            conversation: full,
            added: vec![tool_call],
            usage: Usage::default(),
            steps: 1,
            stop: TurnStop::AwaitingApproval {
                pending: vec![ToolApprovalRequest {
                    tool_use_id: "call_1_0".to_string(),
                    name: "deploy".to_string(),
                    input: serde_json::json!({}),
                }],
                completed: vec![],
            },
            decisions: vec![],
        };
        settle(&db, turn_id, &turn).await.unwrap();

        abandon(&db, conv).await.unwrap();

        assert!(load_pending(&db, conv).await.unwrap().is_none());
        let messages = super::super::conversations::load_messages(&db, conv).await.unwrap();
        assert_eq!(messages.len(), 1, "only the original user message remains");
        let tool_calls: i64 = sqlx::query_scalar("SELECT count(*) FROM tool_calls")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(tool_calls, 0);

        // The conversation is open again.
        begin(&db, conv, Some(&user_text("try again"))).await.unwrap();
    }

    #[tokio::test]
    async fn fail_writes_no_messages_and_leaves_the_conversation_sendable() {
        let db = Db::in_memory().await.unwrap();
        let conv = seed_conversation(&db).await;
        let turn_id = begin(&db, conv, Some(&user_text("hi"))).await.unwrap();

        fail(
            &db,
            turn_id,
            &ErrorReport {
                kind: shared::error::ErrorKind::Network,
                provider: Some("scripted".to_string()),
                message: "boom".to_string(),
                retryable: true,
            },
        )
        .await
        .unwrap();

        let messages = super::super::conversations::load_messages(&db, conv).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, Role::User);

        let state: String = sqlx::query_scalar("SELECT state FROM turns WHERE id = ?")
            .bind(turn_id.get())
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(state, "failed");

        // The conversation is open again — begin succeeds.
        begin(&db, conv, Some(&user_text("retry"))).await.unwrap();
    }

    #[tokio::test]
    async fn two_turns_producing_the_same_tool_use_id_do_not_collide() {
        let db = Db::in_memory().await.unwrap();
        let conv = seed_conversation(&db).await;

        for i in 0..2 {
            let turn_id = begin(&db, conv, Some(&user_text(&format!("go {i}")))).await.unwrap();
            let conversation = super::super::conversations::load_conversation(&db, conv).await.unwrap();
            let mut full = conversation.clone();
            let tool_call = assistant_text_with_tool_use("call_1_0", "deploy", serde_json::json!({}));
            full.messages.push(tool_call.clone());
            let result_msg = Message::user(vec![ContentBlock::ToolResult {
                tool_use_id: "call_1_0".to_string(),
                content: vec![],
                is_error: false,
            }]);
            full.messages.push(result_msg.clone());
            let turn = AgentTurn {
                conversation: full,
                added: vec![tool_call, result_msg],
                usage: Usage::default(),
                steps: 1,
                stop: TurnStop::Done { stop_reason: StopReason::EndTurn },
                decisions: vec![],
            };
            settle(&db, turn_id, &turn).await.unwrap();
        }

        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM tool_calls WHERE tool_use_id = 'call_1_0'")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(count, 2, "each turn's call_1_0 must be its own row");
    }

    #[tokio::test]
    async fn rebuild_tool_calls_is_idempotent_and_preserves_decision() {
        let db = Db::in_memory().await.unwrap();
        let conv = seed_conversation(&db).await;
        let turn_id = begin(&db, conv, Some(&user_text("hi"))).await.unwrap();
        let conversation = super::super::conversations::load_conversation(&db, conv).await.unwrap();
        let mut full = conversation.clone();
        let tool_call = assistant_text_with_tool_use("call_1_0", "deploy", serde_json::json!({}));
        full.messages.push(tool_call.clone());
        let turn = AgentTurn {
            conversation: full,
            added: vec![tool_call],
            usage: Usage::default(),
            steps: 1,
            stop: TurnStop::AwaitingApproval {
                pending: vec![ToolApprovalRequest {
                    tool_use_id: "call_1_0".to_string(),
                    name: "deploy".to_string(),
                    input: serde_json::json!({}),
                }],
                completed: vec![],
            },
            decisions: vec![],
        };
        settle(&db, turn_id, &turn).await.unwrap();

        reopen(
            &db,
            conv,
            &[ToolDecision {
                tool_use_id: "call_1_0".to_string(),
                decision: Decision::Approve,
            }],
        )
        .await
        .unwrap();

        let before: (String, Option<String>) =
            sqlx::query_as("SELECT decision, created_at FROM tool_calls WHERE turn_id = ?")
                .bind(turn_id.get())
                .fetch_one(db.pool())
                .await
                .unwrap();

        // Re-settle with the same AwaitingApproval turn again (simulating a
        // second suspend on the same pending set) and confirm the rebuild
        // doesn't clobber `decision` or `created_at`.
        settle(&db, turn_id, &turn).await.unwrap();
        let after: (String, Option<String>) =
            sqlx::query_as("SELECT decision, created_at FROM tool_calls WHERE turn_id = ?")
                .bind(turn_id.get())
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn settle_writes_provenance_for_a_policy_decision_and_a_rebuild_does_not_clobber_it() {
        let db = Db::in_memory().await.unwrap();
        let conv = seed_conversation(&db).await;
        let turn_id = begin(&db, conv, Some(&user_text("hi"))).await.unwrap();
        let conversation = super::super::conversations::load_conversation(&db, conv).await.unwrap();
        let mut full = conversation.clone();
        let tool_call = assistant_text_with_tool_use("call_1_0", "deploy", serde_json::json!({}));
        full.messages.push(tool_call.clone());
        let result_msg = Message::user(vec![ContentBlock::ToolResult {
            tool_use_id: "call_1_0".to_string(),
            content: vec![],
            is_error: false,
        }]);
        full.messages.push(result_msg.clone());

        let turn = AgentTurn {
            conversation: full,
            added: vec![tool_call, result_msg],
            usage: Usage::default(),
            steps: 1,
            stop: TurnStop::Done { stop_reason: StopReason::EndTurn },
            decisions: vec![ToolDecisionRecord {
                tool_use_id: "call_1_0".to_string(),
                decision: Decision::Approve,
                decided_by: DecidedBy::Policy { reason: "matched auto-approve rule".to_string() },
                decided_at: "2026-08-29T00:00:00.000Z".to_string(),
            }],
        };
        settle(&db, turn_id, &turn).await.unwrap();

        let row: (Option<String>, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT decided_by, decision_reason, decided_at FROM tool_calls WHERE turn_id = ? AND tool_use_id = 'call_1_0'",
        )
        .bind(turn_id.get())
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(row.0.as_deref(), Some("policy"));
        assert_eq!(row.1.as_deref(), Some("matched auto-approve rule"));
        assert_eq!(row.2.as_deref(), Some("2026-08-29T00:00:00.000Z"));

        // Re-settling (a rebuild) must not clobber the provenance columns —
        // exactly the same guarantee `decision` already has.
        settle(&db, turn_id, &turn).await.unwrap();
        let row_again: (Option<String>, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT decided_by, decision_reason, decided_at FROM tool_calls WHERE turn_id = ? AND tool_use_id = 'call_1_0'",
        )
        .bind(turn_id.get())
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(row, row_again);
    }

    #[tokio::test]
    async fn settle_writes_provenance_for_a_user_decision_after_reopen() {
        let db = Db::in_memory().await.unwrap();
        let conv = seed_conversation(&db).await;
        let turn_id = begin(&db, conv, Some(&user_text("hi"))).await.unwrap();
        let conversation = super::super::conversations::load_conversation(&db, conv).await.unwrap();
        let mut full = conversation.clone();
        let tool_call = assistant_text_with_tool_use("call_1_0", "deploy", serde_json::json!({}));
        full.messages.push(tool_call.clone());
        let turn = AgentTurn {
            conversation: full,
            added: vec![tool_call],
            usage: Usage::default(),
            steps: 1,
            stop: TurnStop::AwaitingApproval {
                pending: vec![ToolApprovalRequest {
                    tool_use_id: "call_1_0".to_string(),
                    name: "deploy".to_string(),
                    input: serde_json::json!({}),
                }],
                completed: vec![],
            },
            decisions: vec![],
        };
        settle(&db, turn_id, &turn).await.unwrap();

        // `reopen` writes only `decision` — the provenance columns stay NULL
        // until the turn is driven forward again and settles.
        let (_, resumed_turn) = reopen(
            &db,
            conv,
            &[ToolDecision {
                tool_use_id: "call_1_0".to_string(),
                decision: Decision::Approve,
            }],
        )
        .await
        .unwrap();

        let mid: (String, Option<String>) =
            sqlx::query_as("SELECT decision, decided_by FROM tool_calls WHERE turn_id = ?")
                .bind(turn_id.get())
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(mid.0, "approve");
        assert!(mid.1.is_none(), "decided_by must stay NULL until settle writes it");

        let mut done_conversation = resumed_turn.conversation.clone();
        let result_msg = Message::user(vec![ContentBlock::ToolResult {
            tool_use_id: "call_1_0".to_string(),
            content: vec![],
            is_error: false,
        }]);
        done_conversation.messages.push(result_msg.clone());
        let done_turn = AgentTurn {
            conversation: done_conversation,
            added: vec![result_msg],
            usage: Usage::default(),
            steps: 1,
            stop: TurnStop::Done { stop_reason: StopReason::EndTurn },
            decisions: vec![ToolDecisionRecord {
                tool_use_id: "call_1_0".to_string(),
                decision: Decision::Approve,
                decided_by: DecidedBy::User,
                decided_at: "2026-08-29T00:00:01.000Z".to_string(),
            }],
        };
        settle(&db, turn_id, &done_turn).await.unwrap();

        let after: (String, Option<String>) =
            sqlx::query_as("SELECT decision, decided_by FROM tool_calls WHERE turn_id = ?")
                .bind(turn_id.get())
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(after.0, "approve");
        assert_eq!(after.1.as_deref(), Some("user"));
    }

    #[tokio::test]
    async fn decisions_for_conversation_reports_a_settled_policy_decision_and_omits_never_gated_calls() {
        let db = Db::in_memory().await.unwrap();
        let conv = seed_conversation(&db).await;
        let turn_id = begin(&db, conv, Some(&user_text("hi"))).await.unwrap();
        let conversation = super::super::conversations::load_conversation(&db, conv).await.unwrap();
        let mut full = conversation.clone();
        // A gated call the policy approved, and an ordinary automatic call
        // beside it (`get_weather`) that was never gated at all.
        let gated = assistant_text_with_tool_use("call_1_0", "deploy", serde_json::json!({}));
        full.messages.push(gated.clone());
        let result_msg = Message::user(vec![ContentBlock::ToolResult {
            tool_use_id: "call_1_0".to_string(),
            content: vec![],
            is_error: false,
        }]);
        full.messages.push(result_msg.clone());

        let turn = AgentTurn {
            conversation: full,
            added: vec![gated, result_msg],
            usage: Usage::default(),
            steps: 1,
            stop: TurnStop::Done { stop_reason: StopReason::EndTurn },
            decisions: vec![ToolDecisionRecord {
                tool_use_id: "call_1_0".to_string(),
                decision: Decision::Approve,
                decided_by: DecidedBy::Policy { reason: "matched auto-approve rule".to_string() },
                decided_at: "2026-08-29T00:00:00.000Z".to_string(),
            }],
        };
        settle(&db, turn_id, &turn).await.unwrap();

        let decisions = decisions_for_conversation(&db, conv).await.unwrap();
        assert_eq!(decisions.len(), 1, "a never-gated call must not appear");
        assert_eq!(decisions[0].turn_id, turn_id);
        assert_eq!(decisions[0].tool_use_id, "call_1_0");
        assert_eq!(decisions[0].decision, Decision::Approve);
        assert_eq!(
            decisions[0].decided_by,
            Some(DecidedBy::Policy { reason: "matched auto-approve rule".to_string() })
        );
        assert_eq!(decisions[0].decided_at.as_deref(), Some("2026-08-29T00:00:00.000Z"));
    }

    #[tokio::test]
    async fn decisions_for_conversation_reports_none_pending_provenance_after_reopen_but_before_settle() {
        let db = Db::in_memory().await.unwrap();
        let conv = seed_conversation(&db).await;
        let turn_id = begin(&db, conv, Some(&user_text("hi"))).await.unwrap();
        let conversation = super::super::conversations::load_conversation(&db, conv).await.unwrap();
        let mut full = conversation.clone();
        let tool_call = assistant_text_with_tool_use("call_1_0", "deploy", serde_json::json!({}));
        full.messages.push(tool_call.clone());
        let turn = AgentTurn {
            conversation: full,
            added: vec![tool_call],
            usage: Usage::default(),
            steps: 1,
            stop: TurnStop::AwaitingApproval {
                pending: vec![ToolApprovalRequest {
                    tool_use_id: "call_1_0".to_string(),
                    name: "deploy".to_string(),
                    input: serde_json::json!({}),
                }],
                completed: vec![],
            },
            decisions: vec![],
        };
        settle(&db, turn_id, &turn).await.unwrap();
        reopen(
            &db,
            conv,
            &[ToolDecision {
                tool_use_id: "call_1_0".to_string(),
                decision: Decision::Approve,
            }],
        )
        .await
        .unwrap();

        let decisions = decisions_for_conversation(&db, conv).await.unwrap();
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].decision, Decision::Approve);
        assert!(
            decisions[0].decided_by.is_none(),
            "provenance isn't recorded until the turn settles again"
        );
    }
}
