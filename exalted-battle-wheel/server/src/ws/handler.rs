//! Pure dispatch for one `ClientMessage`: checks the token, checks permission, loads and mutates
//! a room through `RoomStore`, and reports every message that needs to go out as a result. Knows
//! nothing about websockets, sockets, or connection lifecycles — see `ws::socket` for that; this
//! is unit-tested directly against the memory stores.

use crate::access_codes::AccessCodeStore;
use crate::rooms::{Room, RoomMember, RoomStore, RoomStoreError};
use shared::battle::BattleLog;
use shared::protocol::{
    apply_command, room_key as validate_room_key, sanitize_name, BattleCommand, BattleRequest, ClientMessage, ConnectionId,
    LeaveReason, Member, ProtocolError, ServerMessage,
};

/// Every message an action results in sending, to every connection it reaches. The caller matches
/// its own `ConnectionId` against this list to know which entry (if any) is the direct reply to
/// tag with the request's `RequestId`; every other entry goes out with no `reply_to`.
pub struct Handled {
    pub sends: Vec<(ConnectionId, ServerMessage)>,
    pub room_transition: Option<RoomTransition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomTransition {
    Entered(String),
    Left,
}

fn just_reply(connection_id: &ConnectionId, message: ServerMessage) -> Handled {
    Handled { sends: vec![(connection_id.clone(), message)], room_transition: None }
}

fn members_of(room: &Room) -> Vec<Member> {
    room.members
        .iter()
        .map(|member| Member { id: member.connection_id.clone(), name: member.name.clone(), can_write: member.can_write, is_host: room.is_host(&member.connection_id) })
        .collect()
}

fn joined_message(room: &Room, you: &ConnectionId, can_write: bool) -> ServerMessage {
    ServerMessage::Joined {
        room: room.display_name.clone(),
        you: you.clone(),
        can_write,
        everyone_writes: room.everyone_writes,
        members: members_of(room),
        version: room.version,
        log: room.log.clone(),
    }
}

fn members_broadcast(room: &Room) -> Vec<(ConnectionId, ServerMessage)> {
    let message = ServerMessage::Members { members: members_of(room), everyone_writes: room.everyone_writes };
    room.members.iter().map(|member| (member.connection_id.clone(), message.clone())).collect()
}

fn state_broadcast(room: &Room) -> Vec<(ConnectionId, ServerMessage)> {
    let message = ServerMessage::State { room: room.display_name.clone(), version: room.version, log: room.log.clone() };
    room.members.iter().map(|member| (member.connection_id.clone(), message.clone())).collect()
}

/// Every caller reaches here through the same path regardless of which `ClientMessage` it's
/// answering — see `handle`'s own doc comment on why token validity is checked once, up front,
/// for every message kind rather than per-branch.
async fn require_valid_token<A: AccessCodeStore>(access_codes: &A, token: &str) -> Result<(), ProtocolError> {
    let code = access_codes.get(token).await.map_err(|_| ProtocolError::Internal)?;
    code.map(|_| ()).ok_or(ProtocolError::Unauthorized)
}

async fn load_current_room<R: RoomStore>(rooms: &R, current_room: Option<&str>) -> Result<Room, ProtocolError> {
    let room_key = current_room.ok_or(ProtocolError::NotInRoom)?;
    rooms.get(room_key).await.map_err(store_error)?.ok_or(ProtocolError::NoSuchRoom)
}

fn store_error(error: RoomStoreError) -> ProtocolError {
    match error {
        RoomStoreError::TooLarge { .. } => ProtocolError::TooLarge,
        RoomStoreError::AlreadyExists => ProtocolError::RoomExists,
        // A version conflict can only mean another write landed on this exact room between this
        // handler's own `get` and `save` -- shouldn't happen given the caller holds the process-
        // wide room lock for the whole handler call (see `ws::hub`'s doc comment), so there is
        // nothing more specific to tell the client than "something went wrong, try again."
        RoomStoreError::VersionConflict | RoomStoreError::Item(_) | RoomStoreError::GetItem(_) | RoomStoreError::PutItem(_) | RoomStoreError::Scan(_) => {
            ProtocolError::Internal
        }
    }
}

/// Dispatches one already-deserialized `ClientMessage`. `current_room` is whatever room key the
/// caller's own connection last entered (`None` if it isn't in one) — this function never looks
/// that up itself, since nothing in `RoomStore` indexes rooms by member, only by name.
pub async fn handle<A, R>(
    access_codes: &A,
    rooms: &R,
    connection_id: &ConnectionId,
    current_room: Option<&str>,
    token: &str,
    message: ClientMessage,
) -> Result<Handled, ProtocolError>
where
    A: AccessCodeStore,
    R: RoomStore,
{
    // Checked once, here, rather than inside every branch below: `ClientMessage`'s own doc
    // comment says every variant carries a token, and every variant needs it checked the same
    // way regardless of what it goes on to do -- `is_admin` never enters into it, since access-
    // code administration and room permissions are unrelated (see `shared::access`'s doc comment).
    require_valid_token(access_codes, token).await?;

    match message {
        ClientMessage::Create { room, name, everyone_writes, log } => {
            handle_create(rooms, connection_id, current_room, room, name, everyone_writes, log).await
        }
        ClientMessage::Join { room, name } => handle_join(rooms, connection_id, current_room, room, name).await,
        ClientMessage::Leave => handle_leave(rooms, connection_id, current_room).await,
        ClientMessage::Rename { name } => handle_rename(rooms, connection_id, current_room, name).await,
        ClientMessage::SetWritable { member, can_write } => handle_set_writable(rooms, connection_id, current_room, member, can_write).await,
        ClientMessage::SetEveryoneWrites { everyone_writes } => handle_set_everyone_writes(rooms, connection_id, current_room, everyone_writes).await,
        ClientMessage::Kick { member } => handle_kick(rooms, connection_id, current_room, member).await,
        ClientMessage::Request(request) => handle_request(rooms, connection_id, current_room, request).await,
        ClientMessage::Resync => handle_resync(rooms, connection_id, current_room).await,
    }
}

async fn handle_create<R: RoomStore>(
    rooms: &R,
    connection_id: &ConnectionId,
    current_room: Option<&str>,
    room_name: String,
    name: String,
    everyone_writes: bool,
    log: BattleLog,
) -> Result<Handled, ProtocolError> {
    if current_room.is_some() {
        return Err(ProtocolError::AlreadyInRoom);
    }
    let key = validate_room_key(&room_name).map_err(|error| ProtocolError::BadRoomName(error.to_string()))?;
    let host_name = sanitize_name(&name);

    let new_room = crate::rooms::NewRoom {
        room_key: key.clone(),
        display_name: room_name.trim().to_string(),
        everyone_writes,
        host: connection_id.clone(),
        host_name,
        log,
    };
    let room = rooms.create(new_room).await.map_err(store_error)?;

    let message = joined_message(&room, connection_id, true);
    Ok(Handled { sends: vec![(connection_id.clone(), message)], room_transition: Some(RoomTransition::Entered(key)) })
}

async fn handle_join<R: RoomStore>(
    rooms: &R,
    connection_id: &ConnectionId,
    current_room: Option<&str>,
    room_name: String,
    name: String,
) -> Result<Handled, ProtocolError> {
    if current_room.is_some() {
        return Err(ProtocolError::AlreadyInRoom);
    }
    let key = validate_room_key(&room_name).map_err(|error| ProtocolError::BadRoomName(error.to_string()))?;
    let mut room = rooms.get(&key).await.map_err(store_error)?.ok_or(ProtocolError::NoSuchRoom)?;

    let can_write = room.everyone_writes;
    room.members.push(RoomMember { connection_id: connection_id.clone(), name: sanitize_name(&name), can_write });
    let room = rooms.save(room).await.map_err(store_error)?;

    let mut sends = members_broadcast(&room);
    // Overwrite this connection's own entry: everyone else gets `Members`, but the joiner needs
    // the fuller `Joined` (its own id, permission, and the current battle) instead.
    sends.retain(|(id, _)| id != connection_id);
    sends.push((connection_id.clone(), joined_message(&room, connection_id, can_write)));

    Ok(Handled { sends, room_transition: Some(RoomTransition::Entered(key)) })
}

/// Removes `connection_id` from `room_key`, without requiring a token — used only by the socket's
/// own teardown when a connection drops without ever sending `Leave` itself, so the rest of the
/// room finds out immediately rather than waiting out the TTL. Unlike every other path here, this
/// proceeds regardless of whether this connection's last-known access token would still validate:
/// a revoked code should still get to clean up after its own disconnect. `None` if the room is
/// already gone — nothing to broadcast to.
pub async fn disconnect<R: RoomStore>(rooms: &R, connection_id: &ConnectionId, room_key: &str) -> Option<Handled> {
    handle_leave(rooms, connection_id, Some(room_key)).await.ok()
}

async fn handle_leave<R: RoomStore>(rooms: &R, connection_id: &ConnectionId, current_room: Option<&str>) -> Result<Handled, ProtocolError> {
    let mut room = load_current_room(rooms, current_room).await?;
    room.members.retain(|member| &member.connection_id != connection_id);
    if room.is_host(connection_id) {
        room.host = None;
    }
    let room = rooms.save(room).await.map_err(store_error)?;

    let mut sends = members_broadcast(&room);
    sends.push((connection_id.clone(), ServerMessage::Left { reason: LeaveReason::Requested }));
    Ok(Handled { sends, room_transition: Some(RoomTransition::Left) })
}

async fn handle_rename<R: RoomStore>(rooms: &R, connection_id: &ConnectionId, current_room: Option<&str>, name: String) -> Result<Handled, ProtocolError> {
    let mut room = load_current_room(rooms, current_room).await?;
    let name = sanitize_name(&name);
    let Some(member) = room.members.iter_mut().find(|member| &member.connection_id == connection_id) else {
        return Err(ProtocolError::NotInRoom);
    };
    if member.name == name {
        return Ok(Handled { sends: Vec::new(), room_transition: None });
    }
    member.name = name;
    let room = rooms.save(room).await.map_err(store_error)?;
    Ok(Handled { sends: members_broadcast(&room), room_transition: None })
}

/// Shared by `SetWritable` and `Kick`: the actor must currently have write access, and neither
/// action may ever target the actor themselves or the room's host.
fn require_can_administer(room: &Room, actor: &ConnectionId, target: &ConnectionId) -> Result<(), ProtocolError> {
    let actor_can_write = room.member(actor).is_some_and(|member| member.can_write);
    if !actor_can_write {
        return Err(ProtocolError::ReadOnly);
    }
    if target == actor || room.is_host(target) {
        return Err(ProtocolError::NotAllowed);
    }
    if room.member(target).is_none() {
        return Err(ProtocolError::NotAllowed);
    }
    Ok(())
}

async fn handle_set_writable<R: RoomStore>(
    rooms: &R,
    connection_id: &ConnectionId,
    current_room: Option<&str>,
    target: ConnectionId,
    can_write: bool,
) -> Result<Handled, ProtocolError> {
    let mut room = load_current_room(rooms, current_room).await?;
    require_can_administer(&room, connection_id, &target)?;
    let member = room.members.iter_mut().find(|member| member.connection_id == target).expect("checked above");
    member.can_write = can_write;
    let room = rooms.save(room).await.map_err(store_error)?;
    Ok(Handled { sends: members_broadcast(&room), room_transition: None })
}

async fn handle_set_everyone_writes<R: RoomStore>(
    rooms: &R,
    connection_id: &ConnectionId,
    current_room: Option<&str>,
    everyone_writes: bool,
) -> Result<Handled, ProtocolError> {
    let mut room = load_current_room(rooms, current_room).await?;
    if !room.member(connection_id).is_some_and(|member| member.can_write) {
        return Err(ProtocolError::ReadOnly);
    }
    room.everyone_writes = everyone_writes;
    let room = rooms.save(room).await.map_err(store_error)?;
    Ok(Handled { sends: members_broadcast(&room), room_transition: None })
}

async fn handle_kick<R: RoomStore>(rooms: &R, connection_id: &ConnectionId, current_room: Option<&str>, target: ConnectionId) -> Result<Handled, ProtocolError> {
    let mut room = load_current_room(rooms, current_room).await?;
    require_can_administer(&room, connection_id, &target)?;
    room.members.retain(|member| member.connection_id != target);
    let room = rooms.save(room).await.map_err(store_error)?;

    let mut sends = members_broadcast(&room);
    sends.push((target, ServerMessage::Left { reason: LeaveReason::Kicked }));
    Ok(Handled { sends, room_transition: None })
}

async fn handle_request<R: RoomStore>(rooms: &R, connection_id: &ConnectionId, current_room: Option<&str>, request: BattleRequest) -> Result<Handled, ProtocolError> {
    let mut room = load_current_room(rooms, current_room).await?;
    if !room.member(connection_id).is_some_and(|member| member.can_write) {
        return Err(ProtocolError::ReadOnly);
    }

    // `PushMinting` carries placeholder ids; the server is the room's sole authority, so it (and
    // only it) turns them into real ones by restamping from its own log. See
    // `shared::protocol::BattleRequest`'s doc comment.
    let command = match request {
        BattleRequest::Push(event) => BattleCommand::Push(event),
        BattleRequest::PushMinting(event) => BattleCommand::Push(room.log.restamp(event)),
        BattleRequest::Undo => BattleCommand::Undo,
        BattleRequest::Redo => BattleCommand::Redo,
        BattleRequest::Seek(cursor) => BattleCommand::Seek(cursor),
        BattleRequest::Reset => BattleCommand::Reset,
    };
    apply_command(&mut room.log, &command).map_err(|error| ProtocolError::IllegalMove(error.to_string()))?;

    let room = rooms.save(room).await.map_err(store_error)?;
    Ok(Handled { sends: state_broadcast(&room), room_transition: None })
}

/// Available to a readonly member too — it changes nothing, it only asks. Answered with the same
/// `Joined` shape a fresh `Join` gets, since that already carries everything a resync needs
/// (membership, permission, and the current battle) with no second message type to invent.
async fn handle_resync<R: RoomStore>(rooms: &R, connection_id: &ConnectionId, current_room: Option<&str>) -> Result<Handled, ProtocolError> {
    let room = load_current_room(rooms, current_room).await?;
    let can_write = room.member(connection_id).is_some_and(|member| member.can_write);
    Ok(just_reply(connection_id, joined_message(&room, connection_id, can_write)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access_codes::MemoryAccessCodeStore;
    use crate::rooms::MemoryRoomStore;

    fn access() -> MemoryAccessCodeStore {
        let store = MemoryAccessCodeStore::default();
        store.seed("token", false);
        store
    }

    fn conn(id: &str) -> ConnectionId {
        ConnectionId(id.to_string())
    }

    async fn create(
        access: &MemoryAccessCodeStore,
        rooms: &MemoryRoomStore,
        host: &ConnectionId,
        room: &str,
        everyone_writes: bool,
    ) -> Handled {
        handle(
            access,
            rooms,
            host,
            None,
            "token",
            ClientMessage::Create { room: room.to_string(), name: "Host".to_string(), everyone_writes, log: BattleLog::new() },
        )
        .await
        .unwrap()
    }

    async fn join(access: &MemoryAccessCodeStore, rooms: &MemoryRoomStore, member: &ConnectionId, room: &str) -> Handled {
        handle(access, rooms, member, None, "token", ClientMessage::Join { room: room.to_string(), name: "Member".to_string() })
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn an_unknown_token_is_unauthorized() {
        let access = access();
        let rooms = MemoryRoomStore::default();
        let result = handle(&access, &rooms, &conn("host"), None, "not-a-real-token", ClientMessage::Leave).await;
        assert_eq!(result.err(), Some(ProtocolError::Unauthorized));
    }

    #[tokio::test]
    async fn creating_a_room_twice_is_a_conflict_even_with_different_casing() {
        let access = access();
        let rooms = MemoryRoomStore::default();
        create(&access, &rooms, &conn("host"), "Test Room", true).await;

        let result = handle(
            &access,
            &rooms,
            &conn("other"),
            None,
            "token",
            ClientMessage::Create { room: "test room".to_string(), name: "Other".to_string(), everyone_writes: true, log: BattleLog::new() },
        )
        .await;
        assert_eq!(result.err(), Some(ProtocolError::RoomExists));
    }

    #[tokio::test]
    async fn creating_or_joining_while_already_in_a_room_is_refused() {
        let access = access();
        let rooms = MemoryRoomStore::default();
        let host = conn("host");
        create(&access, &rooms, &host, "room", true).await;

        let result = handle(
            &access,
            &rooms,
            &host,
            Some("room"),
            "token",
            ClientMessage::Create { room: "other".to_string(), name: "Host".to_string(), everyone_writes: true, log: BattleLog::new() },
        )
        .await;
        assert_eq!(result.err(), Some(ProtocolError::AlreadyInRoom));
    }

    #[tokio::test]
    async fn joining_a_missing_room_is_no_such_room() {
        let access = access();
        let rooms = MemoryRoomStore::default();
        let result = handle(&access, &rooms, &conn("joiner"), None, "token", ClientMessage::Join { room: "nope".to_string(), name: "Joiner".to_string() }).await;
        assert_eq!(result.err(), Some(ProtocolError::NoSuchRoom));
    }

    #[tokio::test]
    async fn a_readonly_member_cannot_make_a_move_but_can_rename() {
        let access = access();
        let rooms = MemoryRoomStore::default();
        create(&access, &rooms, &conn("host"), "room", false).await;
        let joiner = conn("joiner");
        join(&access, &rooms, &joiner, "room").await;

        let move_result = handle(&access, &rooms, &joiner, Some("room"), "token", ClientMessage::Request(BattleRequest::Undo)).await;
        assert_eq!(move_result.err(), Some(ProtocolError::ReadOnly));

        let rename_result = handle(&access, &rooms, &joiner, Some("room"), "token", ClientMessage::Rename { name: "New Name".to_string() }).await;
        assert!(rename_result.is_ok());
    }

    #[tokio::test]
    async fn set_writable_refuses_self_and_host_and_requires_write_access() {
        let access = access();
        let rooms = MemoryRoomStore::default();
        let host = conn("host");
        create(&access, &rooms, &host, "room", false).await;
        let joiner = conn("joiner");
        join(&access, &rooms, &joiner, "room").await;

        let host_self_demote =
            handle(&access, &rooms, &host, Some("room"), "token", ClientMessage::SetWritable { member: host.clone(), can_write: false }).await;
        assert_eq!(host_self_demote.err(), Some(ProtocolError::NotAllowed));

        let readonly_self_promote =
            handle(&access, &rooms, &joiner, Some("room"), "token", ClientMessage::SetWritable { member: joiner.clone(), can_write: true }).await;
        assert_eq!(readonly_self_promote.err(), Some(ProtocolError::ReadOnly));

        handle(&access, &rooms, &host, Some("room"), "token", ClientMessage::SetWritable { member: joiner.clone(), can_write: true }).await.unwrap();

        let demote_host =
            handle(&access, &rooms, &joiner, Some("room"), "token", ClientMessage::SetWritable { member: host.clone(), can_write: false }).await;
        assert_eq!(demote_host.err(), Some(ProtocolError::NotAllowed));
    }

    #[tokio::test]
    async fn kicking_removes_the_member_and_notifies_them() {
        let access = access();
        let rooms = MemoryRoomStore::default();
        let host = conn("host");
        create(&access, &rooms, &host, "room", true).await;
        let joiner = conn("joiner");
        join(&access, &rooms, &joiner, "room").await;

        let handled = handle(&access, &rooms, &host, Some("room"), "token", ClientMessage::Kick { member: joiner.clone() }).await.unwrap();
        assert!(handled.sends.iter().any(|(id, message)| id == &joiner && matches!(message, ServerMessage::Left { reason: LeaveReason::Kicked })));

        let room = rooms.get("room").await.unwrap().unwrap();
        assert!(room.member(&joiner).is_none());
    }

    #[tokio::test]
    async fn an_illegal_move_does_not_change_the_stored_version() {
        let access = access();
        let rooms = MemoryRoomStore::default();
        let host = conn("host");
        create(&access, &rooms, &host, "room", true).await;
        let before = rooms.get("room").await.unwrap().unwrap().version;

        let result = handle(&access, &rooms, &host, Some("room"), "token", ClientMessage::Request(BattleRequest::Undo)).await;
        assert!(matches!(result, Err(ProtocolError::IllegalMove(_))));

        let after = rooms.get("room").await.unwrap().unwrap().version;
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn joining_after_the_host_left_grants_permission_from_everyone_writes_alone() {
        let access = access();
        let rooms = MemoryRoomStore::default();
        let host = conn("host");
        create(&access, &rooms, &host, "room", false).await;
        handle(&access, &rooms, &host, Some("room"), "token", ClientMessage::Leave).await.unwrap();

        let room = rooms.get("room").await.unwrap().unwrap();
        assert!(room.host.is_none());

        let joiner = conn("joiner");
        let joined = join(&access, &rooms, &joiner, "room").await;
        match &joined.sends[..] {
            [(_, ServerMessage::Joined { can_write, .. })] => assert!(!can_write),
            other => panic!("expected a single Joined reply, got {other:?}"),
        }

        // Nobody in the room can write, so nobody can promote anyone either.
        let promote =
            handle(&access, &rooms, &joiner, Some("room"), "token", ClientMessage::SetWritable { member: joiner.clone(), can_write: true }).await;
        assert_eq!(promote.err(), Some(ProtocolError::ReadOnly));
    }
}
