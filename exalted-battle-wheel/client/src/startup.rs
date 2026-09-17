//! What happens in the first moments of a page load: an invite link's `auth_code`/`join_room`
//! query parameters are read off the address bar (and scrubbed from it -- see `crate::link`),
//! this browser's access code is checked against the server exactly once (`Access::start`), and
//! only once that's settled is a room entered. Ordering matters here more than anywhere else in
//! the app -- see `run`.

use crate::access::Access;
use crate::battle_net::Battles;
use crate::prefs::Prefs;
use shared::protocol::room_key;

/// What startup should do about rooms, and what (if anything) is worth telling the user about the
/// link that asked for it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RoomPlan {
    action: RoomAction,
    complaint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RoomAction {
    Stay,
    Resume,
    Join(String),
}

fn stay_or_resume(stored_room: Option<&str>) -> RoomAction {
    if stored_room.is_some() { RoomAction::Resume } else { RoomAction::Stay }
}

/// Decides what to do with an invite link's `join_room` against whatever room session this
/// browser already had stored, without touching a signal or making a request -- so the one
/// decision with any real policy in it ("which room wins") is a function with tests, not a branch
/// buried in `run`'s continuation.
///
/// A link naming the room this browser is already remembering wins by *resuming* that stored
/// session rather than joining fresh: only `resume()` presents the stored host token, so a host
/// following their own invite link lands back as host instead of rejoining as an ordinary member.
/// A link that fails `room_key` (blank, or over `MAX_ROOM_NAME_LEN`) never costs a stored session
/// its resume -- it's downgraded to a complaint instead.
fn room_plan(stored_room: Option<&str>, join_room: Option<&str>) -> RoomPlan {
    let Some(link_room) = join_room else {
        return RoomPlan { action: stay_or_resume(stored_room), complaint: None };
    };

    match room_key(link_room) {
        Err(error) => RoomPlan { action: stay_or_resume(stored_room), complaint: Some(error.to_string()) },
        Ok(link_key) => {
            let stored_key = stored_room.and_then(|room| room_key(room).ok());
            let action = if stored_key.as_deref() == Some(link_key.as_str()) {
                RoomAction::Resume
            } else {
                RoomAction::Join(link_room.trim().to_string())
            };
            RoomPlan { action, complaint: None }
        }
    }
}

/// Runs once, from `app.rs`, in place of the `battles.restore()` call it replaces.
pub fn run(access: Access, battles: Battles, prefs: Prefs) {
    let params = crate::link::take_startup_params();
    let plan = room_plan(battles.stored_room().as_deref(), params.join_room.as_deref());

    let hint = match &plan.action {
        RoomAction::Stay => None,
        RoomAction::Resume => battles.stored_room(),
        RoomAction::Join(room) => Some(room.clone()),
    };
    if !matches!(plan.action, RoomAction::Stay) {
        battles.hold_connecting(hint);
    }

    access.start(params.auth_code, move |signed_in| {
        if let Some(complaint) = plan.complaint {
            crate::ui::toast::error(format!("That invite link's room name isn't usable: {complaint}"));
        }

        if !signed_in {
            battles.release_connecting();
            if matches!(plan.action, RoomAction::Join(_)) {
                crate::ui::toast::error("That invite link needs an access code this browser doesn't have.".to_string());
            }
            return;
        }

        match plan.action {
            RoomAction::Stay => {}
            RoomAction::Resume => battles.resume(),
            RoomAction::Join(room) => battles.join_room(room, prefs.ensure_player_name()),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_stored_and_no_link_stays_in_solo() {
        assert_eq!(room_plan(None, None), RoomPlan { action: RoomAction::Stay, complaint: None });
    }

    #[test]
    fn a_stored_session_with_no_link_resumes() {
        assert_eq!(room_plan(Some("my-game"), None), RoomPlan { action: RoomAction::Resume, complaint: None });
    }

    #[test]
    fn a_link_naming_the_stored_room_resumes_case_and_whitespace_insensitively() {
        assert_eq!(
            room_plan(Some("My Game"), Some("  my game  ")),
            RoomPlan { action: RoomAction::Resume, complaint: None }
        );
    }

    #[test]
    fn a_link_naming_a_different_room_joins_it_as_spelled() {
        assert_eq!(
            room_plan(Some("my-game"), Some(" Tuesday's Game ")),
            RoomPlan { action: RoomAction::Join("Tuesday's Game".to_string()), complaint: None }
        );
    }

    #[test]
    fn a_link_with_no_stored_session_joins() {
        assert_eq!(
            room_plan(None, Some("tuesday-game")),
            RoomPlan { action: RoomAction::Join("tuesday-game".to_string()), complaint: None }
        );
    }

    #[test]
    fn a_blank_join_room_complains_but_still_resumes_a_stored_session() {
        let plan = room_plan(Some("my-game"), Some("   "));
        assert_eq!(plan.action, RoomAction::Resume);
        assert!(plan.complaint.is_some());
    }

    #[test]
    fn an_over_long_join_room_complains_but_never_panics() {
        let long = "a".repeat(41);
        let plan = room_plan(None, Some(&long));
        assert_eq!(plan.action, RoomAction::Stay);
        assert!(plan.complaint.is_some());
    }

    #[test]
    fn a_stored_room_that_itself_fails_room_key_still_lets_a_valid_link_join() {
        let plan = room_plan(Some(""), Some("tuesday-game"));
        assert_eq!(plan.action, RoomAction::Join("tuesday-game".to_string()));
        assert!(plan.complaint.is_none());
    }
}
