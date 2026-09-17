//! Pure dispatch for one `ClientMessage`: checks the token, checks permission, loads and mutates
//! a room through `RoomStore`, and reports every message that needs to go out as a result. Knows
//! nothing about websockets, sockets, or connection lifecycles — see `ws::socket` for that; this
//! is unit-tested directly against the memory stores.

use crate::access_codes::AccessCodeStore;
use crate::rooms::{Room, RoomMember, RoomStore, RoomStoreError};
use crate::sessions::Sessions;
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
        .map(|member| Member {
            id: member.connection_id.clone(),
            name: member.name.clone().try_into().expect("valid by construction: sanitize_name enforces the bound"),
            can_write: member.can_write,
            is_host: member.is_host,
        })
        .collect()
}

/// Builds the `Joined` reply for `Create`, `Join`, and `Resync` alike — `can_write` and the
/// session's `host` claim both come from `room.member(you)` rather than being passed in, so this
/// can never report something that isn't what was actually stored.
fn joined_message(room: &Room, you: &ConnectionId, sessions: &Sessions) -> Result<ServerMessage, ProtocolError> {
    let member = room.member(you).expect("the caller just inserted or found this member");
    let session = sessions.issue(&room.id, member.is_host).map_err(|error| {
        tracing::error!(%error, "could not sign a room session");
        ProtocolError::Internal
    })?;
    Ok(ServerMessage::Joined {
        room: room.display_name.clone().try_into().expect("valid by construction: see ws/handler.rs's room_key validation"),
        you: you.clone(),
        can_write: member.can_write,
        everyone_writes: room.everyone_writes,
        members: members_of(room),
        version: room.version,
        log: room.log.clone(),
        session,
    })
}

fn members_broadcast(room: &Room) -> Vec<(ConnectionId, ServerMessage)> {
    let message = ServerMessage::Members { members: members_of(room), everyone_writes: room.everyone_writes };
    room.members.iter().map(|member| (member.connection_id.clone(), message.clone())).collect()
}

fn state_broadcast(room: &Room) -> Vec<(ConnectionId, ServerMessage)> {
    let message = ServerMessage::State {
        room: room.display_name.clone().try_into().expect("valid by construction: see ws/handler.rs's room_key validation"),
        version: room.version,
        log: room.log.clone(),
    };
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
        RoomStoreError::VersionConflict
        | RoomStoreError::Item(_)
        | RoomStoreError::GetItem(_)
        | RoomStoreError::PutItem(_)
        | RoomStoreError::Scan(_)
        | RoomStoreError::DeleteItem(_) => {
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
    sessions: &Sessions,
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
            let create = CreateRoom { room_name: room.to_string(), name: name.to_string(), everyone_writes, log };
            handle_create(rooms, sessions, connection_id, current_room, create).await
        }
        ClientMessage::Join { room, name, session } => {
            handle_join(rooms, sessions, connection_id, current_room, room.to_string(), name.to_string(), session).await
        }
        ClientMessage::Leave => handle_leave(rooms, connection_id, current_room).await,
        ClientMessage::Rename { name } => handle_rename(rooms, connection_id, current_room, name.to_string()).await,
        ClientMessage::SetWritable { member, can_write } => handle_set_writable(rooms, connection_id, current_room, member, can_write).await,
        ClientMessage::SetEveryoneWrites { everyone_writes } => handle_set_everyone_writes(rooms, connection_id, current_room, everyone_writes).await,
        ClientMessage::Kick { member } => handle_kick(rooms, connection_id, current_room, member).await,
        ClientMessage::Request(request) => handle_request(rooms, connection_id, current_room, request).await,
        ClientMessage::Resync => handle_resync(rooms, sessions, connection_id, current_room).await,
    }
}

/// `ClientMessage::Create`'s payload, bundled so `handle_create` doesn't grow an unreadable
/// parameter list on top of the caller-context arguments every handler already takes.
struct CreateRoom {
    room_name: String,
    name: String,
    everyone_writes: bool,
    log: BattleLog,
}

async fn handle_create<R: RoomStore>(
    rooms: &R,
    sessions: &Sessions,
    connection_id: &ConnectionId,
    current_room: Option<&str>,
    create: CreateRoom,
) -> Result<Handled, ProtocolError> {
    if current_room.is_some() {
        return Err(ProtocolError::AlreadyInRoom);
    }
    let key = validate_room_key(&create.room_name).map_err(|error| ProtocolError::BadRoomName(error.to_string()))?;
    let host_name = sanitize_name(&create.name).to_string();

    let new_room = crate::rooms::NewRoom {
        room_key: key.clone(),
        display_name: create.room_name.trim().to_string(),
        everyone_writes: create.everyone_writes,
        host: connection_id.clone(),
        host_name,
        log: create.log,
    };
    let room = rooms.create(new_room).await.map_err(store_error)?;

    let message = joined_message(&room, connection_id, sessions)?;
    Ok(Handled { sends: vec![(connection_id.clone(), message)], room_transition: Some(RoomTransition::Entered(key)) })
}

async fn handle_join<R: RoomStore>(
    rooms: &R,
    sessions: &Sessions,
    connection_id: &ConnectionId,
    current_room: Option<&str>,
    room_name: String,
    name: String,
    session: Option<String>,
) -> Result<Handled, ProtocolError> {
    if current_room.is_some() {
        return Err(ProtocolError::AlreadyInRoom);
    }
    let key = validate_room_key(&room_name).map_err(|error| ProtocolError::BadRoomName(error.to_string()))?;
    let mut room = rooms.get(&key).await.map_err(store_error)?.ok_or(ProtocolError::NoSuchRoom)?;

    // A missing `session` is an ordinary join, not an error -- the token only ever *adds* host
    // status, checked after confirming the room itself still exists so a room that expired while
    // the browser was closed reports `NoSuchRoom` rather than a confusing session rejection.
    let is_host = match session {
        None => false,
        Some(token) => sessions.verify(&token, &room.id).map_err(ProtocolError::InvalidSession)?.host,
    };
    let can_write = is_host || room.everyone_writes;
    room.members.push(RoomMember { connection_id: connection_id.clone(), name: sanitize_name(&name).to_string(), can_write, is_host });
    let room = rooms.save(room).await.map_err(store_error)?;

    let mut sends = members_broadcast(&room);
    // Overwrite this connection's own entry: everyone else gets `Members`, but the joiner needs
    // the fuller `Joined` (its own id, permission, and the current battle) instead.
    sends.retain(|(id, _)| id != connection_id);
    sends.push((connection_id.clone(), joined_message(&room, connection_id, sessions)?));

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
    // Removing the member removes whatever `is_host` it carried too -- host status now lives on
    // the member (see `RoomMember`'s doc comment), not the room, so there's nothing else to clear.
    room.members.retain(|member| &member.connection_id != connection_id);
    let room = rooms.save(room).await.map_err(store_error)?;

    let mut sends = members_broadcast(&room);
    sends.push((connection_id.clone(), ServerMessage::Left { reason: LeaveReason::Requested }));
    Ok(Handled { sends, room_transition: Some(RoomTransition::Left) })
}

async fn handle_rename<R: RoomStore>(rooms: &R, connection_id: &ConnectionId, current_room: Option<&str>, name: String) -> Result<Handled, ProtocolError> {
    let mut room = load_current_room(rooms, current_room).await?;
    let name = sanitize_name(&name).to_string();
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
/// (membership, permission, a fresh session token, and the current battle) with no second message
/// type to invent.
async fn handle_resync<R: RoomStore>(rooms: &R, sessions: &Sessions, connection_id: &ConnectionId, current_room: Option<&str>) -> Result<Handled, ProtocolError> {
    let room = load_current_room(rooms, current_room).await?;
    Ok(just_reply(connection_id, joined_message(&room, connection_id, sessions)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access_codes::MemoryAccessCodeStore;
    use crate::rooms::MemoryRoomStore;
    use shared::protocol::SessionRejection;

    fn access() -> MemoryAccessCodeStore {
        let store = MemoryAccessCodeStore::default();
        store.seed("token", false);
        store
    }

    fn sessions() -> Sessions {
        Sessions::new("test-secret")
    }

    fn conn(id: &str) -> ConnectionId {
        ConnectionId(id.to_string())
    }

    async fn create(
        access: &MemoryAccessCodeStore,
        rooms: &MemoryRoomStore,
        sessions: &Sessions,
        host: &ConnectionId,
        room: &str,
        everyone_writes: bool,
    ) -> Handled {
        handle(
            access,
            rooms,
            sessions,
            host,
            None,
            "token",
            ClientMessage::Create { room: room.try_into().unwrap(), name: "Host".try_into().unwrap(), everyone_writes, log: BattleLog::new() },
        )
        .await
        .unwrap()
    }

    async fn join(access: &MemoryAccessCodeStore, rooms: &MemoryRoomStore, sessions: &Sessions, member: &ConnectionId, room: &str) -> Handled {
        join_with_session(access, rooms, sessions, member, room, None).await.unwrap()
    }

    async fn join_with_session(
        access: &MemoryAccessCodeStore,
        rooms: &MemoryRoomStore,
        sessions: &Sessions,
        member: &ConnectionId,
        room: &str,
        session: Option<String>,
    ) -> Result<Handled, ProtocolError> {
        handle(
            access,
            rooms,
            sessions,
            member,
            None,
            "token",
            ClientMessage::Join { room: room.try_into().unwrap(), name: "Member".try_into().unwrap(), session },
        )
        .await
    }

    /// Pulls the one `Joined` reply out of a `Handled`'s sends -- there's always exactly one
    /// (everyone else in the room gets a `Members` broadcast instead), but a room with other
    /// members already in it means `Joined` isn't necessarily the only or the first entry.
    fn joined_of(handled: &Handled) -> &ServerMessage {
        handled
            .sends
            .iter()
            .map(|(_, message)| message)
            .find(|message| matches!(message, ServerMessage::Joined { .. }))
            .expect("expected a Joined reply")
    }

    /// Pulls the `session` token out of a `Joined` reply -- every test that exercises a rejoin
    /// needs one to present later.
    fn session_of(handled: &Handled) -> String {
        match joined_of(handled) {
            ServerMessage::Joined { session, .. } => session.clone(),
            _ => unreachable!(),
        }
    }

    #[tokio::test]
    async fn an_unknown_token_is_unauthorized() {
        let access = access();
        let rooms = MemoryRoomStore::default();
        let sessions = sessions();
        let result = handle(&access, &rooms, &sessions, &conn("host"), None, "not-a-real-token", ClientMessage::Leave).await;
        assert_eq!(result.err(), Some(ProtocolError::Unauthorized));
    }

    #[tokio::test]
    async fn creating_a_room_twice_is_a_conflict_even_with_different_casing() {
        let access = access();
        let rooms = MemoryRoomStore::default();
        let sessions = sessions();
        create(&access, &rooms, &sessions, &conn("host"), "Test Room", true).await;

        let result = handle(
            &access,
            &rooms,
            &sessions,
            &conn("other"),
            None,
            "token",
            ClientMessage::Create { room: "test room".try_into().unwrap(), name: "Other".try_into().unwrap(), everyone_writes: true, log: BattleLog::new() },
        )
        .await;
        assert_eq!(result.err(), Some(ProtocolError::RoomExists));
    }

    #[tokio::test]
    async fn creating_or_joining_while_already_in_a_room_is_refused() {
        let access = access();
        let rooms = MemoryRoomStore::default();
        let sessions = sessions();
        let host = conn("host");
        create(&access, &rooms, &sessions, &host, "room", true).await;

        let result = handle(
            &access,
            &rooms,
            &sessions,
            &host,
            Some("room"),
            "token",
            ClientMessage::Create { room: "other".try_into().unwrap(), name: "Host".try_into().unwrap(), everyone_writes: true, log: BattleLog::new() },
        )
        .await;
        assert_eq!(result.err(), Some(ProtocolError::AlreadyInRoom));
    }

    #[tokio::test]
    async fn joining_a_missing_room_is_no_such_room() {
        let access = access();
        let rooms = MemoryRoomStore::default();
        let sessions = sessions();
        let result = join_with_session(&access, &rooms, &sessions, &conn("joiner"), "nope", None).await;
        assert_eq!(result.err(), Some(ProtocolError::NoSuchRoom));
    }

    #[tokio::test]
    async fn a_readonly_member_cannot_make_a_move_but_can_rename() {
        let access = access();
        let rooms = MemoryRoomStore::default();
        let sessions = sessions();
        create(&access, &rooms, &sessions, &conn("host"), "room", false).await;
        let joiner = conn("joiner");
        join(&access, &rooms, &sessions, &joiner, "room").await;

        let move_result = handle(&access, &rooms, &sessions, &joiner, Some("room"), "token", ClientMessage::Request(BattleRequest::Undo)).await;
        assert_eq!(move_result.err(), Some(ProtocolError::ReadOnly));

        let rename_result =
            handle(&access, &rooms, &sessions, &joiner, Some("room"), "token", ClientMessage::Rename { name: "New Name".try_into().unwrap() }).await;
        assert!(rename_result.is_ok());
    }

    #[tokio::test]
    async fn set_writable_refuses_self_and_host_and_requires_write_access() {
        let access = access();
        let rooms = MemoryRoomStore::default();
        let sessions = sessions();
        let host = conn("host");
        create(&access, &rooms, &sessions, &host, "room", false).await;
        let joiner = conn("joiner");
        join(&access, &rooms, &sessions, &joiner, "room").await;

        let host_self_demote =
            handle(&access, &rooms, &sessions, &host, Some("room"), "token", ClientMessage::SetWritable { member: host.clone(), can_write: false })
                .await;
        assert_eq!(host_self_demote.err(), Some(ProtocolError::NotAllowed));

        let readonly_self_promote = handle(
            &access,
            &rooms,
            &sessions,
            &joiner,
            Some("room"),
            "token",
            ClientMessage::SetWritable { member: joiner.clone(), can_write: true },
        )
        .await;
        assert_eq!(readonly_self_promote.err(), Some(ProtocolError::ReadOnly));

        handle(&access, &rooms, &sessions, &host, Some("room"), "token", ClientMessage::SetWritable { member: joiner.clone(), can_write: true })
            .await
            .unwrap();

        let demote_host =
            handle(&access, &rooms, &sessions, &joiner, Some("room"), "token", ClientMessage::SetWritable { member: host.clone(), can_write: false })
                .await;
        assert_eq!(demote_host.err(), Some(ProtocolError::NotAllowed));
    }

    #[tokio::test]
    async fn kicking_removes_the_member_and_notifies_them() {
        let access = access();
        let rooms = MemoryRoomStore::default();
        let sessions = sessions();
        let host = conn("host");
        create(&access, &rooms, &sessions, &host, "room", true).await;
        let joiner = conn("joiner");
        join(&access, &rooms, &sessions, &joiner, "room").await;

        let handled = handle(&access, &rooms, &sessions, &host, Some("room"), "token", ClientMessage::Kick { member: joiner.clone() }).await.unwrap();
        assert!(handled.sends.iter().any(|(id, message)| id == &joiner && matches!(message, ServerMessage::Left { reason: LeaveReason::Kicked })));

        let room = rooms.get("room").await.unwrap().unwrap();
        assert!(room.member(&joiner).is_none());
    }

    #[tokio::test]
    async fn an_illegal_move_does_not_change_the_stored_version() {
        let access = access();
        let rooms = MemoryRoomStore::default();
        let sessions = sessions();
        let host = conn("host");
        create(&access, &rooms, &sessions, &host, "room", true).await;
        let before = rooms.get("room").await.unwrap().unwrap().version;

        let result = handle(&access, &rooms, &sessions, &host, Some("room"), "token", ClientMessage::Request(BattleRequest::Undo)).await;
        assert!(matches!(result, Err(ProtocolError::IllegalMove(_))));

        let after = rooms.get("room").await.unwrap().unwrap().version;
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn joining_after_the_host_left_grants_permission_from_everyone_writes_alone() {
        let access = access();
        let rooms = MemoryRoomStore::default();
        let sessions = sessions();
        let host = conn("host");
        create(&access, &rooms, &sessions, &host, "room", false).await;
        handle(&access, &rooms, &sessions, &host, Some("room"), "token", ClientMessage::Leave).await.unwrap();

        let room = rooms.get("room").await.unwrap().unwrap();
        assert!(!room.members.iter().any(|member| member.is_host));

        let joiner = conn("joiner");
        let joined = join(&access, &rooms, &sessions, &joiner, "room").await;
        match joined_of(&joined) {
            ServerMessage::Joined { can_write, .. } => assert!(!can_write),
            other => panic!("expected a Joined reply, got {other:?}"),
        }

        // Nobody in the room can write, so nobody can promote anyone either.
        let promote = handle(
            &access,
            &rooms,
            &sessions,
            &joiner,
            Some("room"),
            "token",
            ClientMessage::SetWritable { member: joiner.clone(), can_write: true },
        )
        .await;
        assert_eq!(promote.err(), Some(ProtocolError::ReadOnly));
    }

    #[tokio::test]
    async fn a_hosts_session_reclaims_host_status_after_their_connection_left() {
        let access = access();
        let rooms = MemoryRoomStore::default();
        let sessions = sessions();
        let host = conn("host");
        let created = create(&access, &rooms, &sessions, &host, "room", false).await;
        let host_session = session_of(&created);

        handle(&access, &rooms, &sessions, &host, Some("room"), "token", ClientMessage::Leave).await.unwrap();

        let rejoined = conn("host-again");
        let joined = join_with_session(&access, &rooms, &sessions, &rejoined, "room", Some(host_session)).await.unwrap();
        match joined_of(&joined) {
            ServerMessage::Joined { can_write, .. } => assert!(can_write),
            other => panic!("expected a Joined reply, got {other:?}"),
        }

        let room = rooms.get("room").await.unwrap().unwrap();
        assert!(room.is_host(&rejoined));

        // The reclaimed host can administer another member, same as an original host could.
        let joiner = conn("joiner");
        join(&access, &rooms, &sessions, &joiner, "room").await;
        let promote =
            handle(&access, &rooms, &sessions, &rejoined, Some("room"), "token", ClientMessage::SetWritable { member: joiner, can_write: true })
                .await;
        assert!(promote.is_ok());
    }

    #[tokio::test]
    async fn a_hosts_session_from_a_different_room_is_refused() {
        let access = access();
        let rooms = MemoryRoomStore::default();
        let sessions = sessions();
        let host = conn("host");
        let created = create(&access, &rooms, &sessions, &host, "room-one", false).await;
        let session = session_of(&created);
        create(&access, &rooms, &sessions, &conn("other-host"), "room-two", false).await;

        let result = join_with_session(&access, &rooms, &sessions, &conn("someone"), "room-two", Some(session)).await;
        assert_eq!(result.err(), Some(ProtocolError::InvalidSession(SessionRejection::WrongRoom)));
    }

    #[tokio::test]
    async fn a_non_hosts_session_grants_write_access_from_everyone_writes_alone() {
        let access = access();
        let rooms = MemoryRoomStore::default();
        let sessions = sessions();
        let host = conn("host");
        create(&access, &rooms, &sessions, &host, "room", false).await;
        let joiner = conn("joiner");
        let joined = join(&access, &rooms, &sessions, &joiner, "room").await;
        let joiner_session = session_of(&joined);

        handle(&access, &rooms, &sessions, &joiner, Some("room"), "token", ClientMessage::Leave).await.unwrap();

        // `everyone_writes` is still off, so a non-host rejoin comes back read-only even though
        // it presents a valid session for this exact room.
        let rejoined = conn("joiner-again");
        let joined = join_with_session(&access, &rooms, &sessions, &rejoined, "room", Some(joiner_session)).await.unwrap();
        match joined_of(&joined) {
            ServerMessage::Joined { can_write, .. } => assert!(!can_write),
            other => panic!("expected a Joined reply, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn two_connections_can_both_hold_host_at_once() {
        let access = access();
        let rooms = MemoryRoomStore::default();
        let sessions = sessions();
        let host = conn("host");
        let created = create(&access, &rooms, &sessions, &host, "room", false).await;
        let host_session = session_of(&created);

        // The original host's connection never leaves -- a second one rejoins presenting the same
        // host session anyway (the scenario a duplicate browser tab, or a reconnect racing the old
        // socket's own teardown, would produce).
        let second = conn("host-second");
        join_with_session(&access, &rooms, &sessions, &second, "room", Some(host_session)).await.unwrap();

        let room = rooms.get("room").await.unwrap().unwrap();
        assert!(room.is_host(&host));
        assert!(room.is_host(&second));
    }

    #[tokio::test]
    async fn resync_reports_this_connections_own_host_status() {
        let access = access();
        let rooms = MemoryRoomStore::default();
        let sessions = sessions();
        let host = conn("host");
        create(&access, &rooms, &sessions, &host, "room", true).await;

        let resynced =
            handle(&access, &rooms, &sessions, &host, Some("room"), "token", ClientMessage::Resync).await.unwrap();
        match joined_of(&resynced) {
            ServerMessage::Joined { members, you, .. } => {
                let member = members.iter().find(|member| &member.id == you).unwrap();
                assert!(member.is_host);
            }
            other => panic!("expected a Joined reply, got {other:?}"),
        }
    }
}
