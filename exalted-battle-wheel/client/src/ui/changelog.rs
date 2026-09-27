//! The Change Log dialog. Content is `client/changelog.toml`, baked into the bundle with
//! `include_str!` -- rustc records it in the crate's dep-info, so cargo rebuilds when it changes
//! without any build-script involvement. Parsed on open rather than at startup: the file is a few
//! hundred bytes and the dialog is only mounted while it's actually open.

use crate::ui::Modal;
use leptos::prelude::*;

const SOURCE: &str = include_str!("../../changelog.toml");

#[derive(Debug, serde::Deserialize)]
struct Changelog {
    release: Vec<Release>,
}

#[derive(Debug, serde::Deserialize)]
struct Release {
    date: String,
    changes: Vec<Change>,
}

/// A change entry: either a plain bullet, or a labelled group with its own nested bullets.
/// Untagged so a plain TOML string still deserializes as `Item` -- flat releases need no changes.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
enum Change {
    Item(String),
    Group { text: String, changes: Vec<Change> },
}

#[derive(Debug, thiserror::Error)]
enum ChangelogError {
    #[error("changelog.toml is not valid TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("changelog.toml defines no releases -- it needs at least one [[release]]")]
    Empty,
}

/// Releases newest first, which is display order. `changelog.toml` is authored oldest first.
fn releases() -> Result<Vec<Release>, ChangelogError> {
    let changelog: Changelog = toml::from_str(SOURCE)?;
    if changelog.release.is_empty() {
        return Err(ChangelogError::Empty);
    }
    let mut releases = changelog.release;
    releases.reverse();
    Ok(releases)
}

fn change_list(changes: Vec<Change>) -> impl IntoView {
    view! {
        <ul class="changelog-changes">
            {changes.into_iter().map(change_item).collect_view()}
        </ul>
    }
}

fn change_item(change: Change) -> impl IntoView {
    match change {
        Change::Item(text) => view! { <li>{text}</li> }.into_any(),
        Change::Group { text, changes } => view! {
            <li>
                <span class="changelog-group">{text}</span>
                {change_list(changes)}
            </li>
        }
        .into_any(),
    }
}

#[component]
pub fn ChangelogModal(open: RwSignal<bool>) -> impl IntoView {
    view! {
        {move || {
            open.get().then(|| view! {
                <Modal title="Change Log" on_close=move || open.set(false)>
                    {match releases() {
                        Ok(releases) => view! {
                            <ul class="changelog-list">
                                {releases.into_iter().map(|release| view! {
                                    <li class="changelog-release">
                                        <div class="changelog-date">{release.date}</div>
                                        {change_list(release.changes)}
                                    </li>
                                }).collect_view()}
                            </ul>
                        }.into_any(),
                        Err(error) => {
                            tracing::error!(%error, "could not read the baked-in change log");
                            view! { <p class="changelog-error">"Change log unavailable."</p> }.into_any()
                        }
                    }}
                </Modal>
            })
        }}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Date;
    use time::format_description::well_known::Iso8601;

    #[test]
    fn parses_and_is_nonempty() {
        let releases = releases().expect("changelog.toml should parse");
        assert!(!releases.is_empty(), "changelog.toml defines no releases");
    }

    #[test]
    fn every_date_is_a_real_iso8601_calendar_date() {
        for release in releases().expect("changelog.toml should parse") {
            assert_eq!(release.date.len(), 10, "{:?} is not YYYY-MM-DD shaped", release.date);
            Date::parse(&release.date, &Iso8601::DATE)
                .unwrap_or_else(|error| panic!("{:?} is not a real calendar date: {error}", release.date));
        }
    }

    #[test]
    fn dates_are_strictly_descending() {
        let releases = releases().expect("changelog.toml should parse");
        for pair in releases.windows(2) {
            let [newer, older] = pair else { unreachable!() };
            assert!(
                newer.date > older.date,
                "{:?} should be strictly newer than {:?} -- changelog.toml is authored oldest first",
                newer.date,
                older.date
            );
        }
    }

    #[test]
    fn every_release_has_at_least_one_nonblank_change() {
        for release in releases().expect("changelog.toml should parse") {
            assert!(!release.changes.is_empty(), "{:?} lists no changes", release.date);
            for change in &release.changes {
                assert_change_nonblank(change, &release.date);
            }
        }
    }

    fn assert_change_nonblank(change: &Change, date: &str) {
        match change {
            Change::Item(text) => {
                assert!(!text.trim().is_empty(), "{date:?} has a blank change entry");
            }
            Change::Group { text, changes } => {
                assert!(!text.trim().is_empty(), "{date:?} has a blank group label");
                assert!(!changes.is_empty(), "{date:?} has a group with no changes");
                for change in changes {
                    assert_change_nonblank(change, date);
                }
            }
        }
    }
}
