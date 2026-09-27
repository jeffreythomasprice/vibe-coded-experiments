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
    changes: Vec<String>,
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
                                        <ul class="changelog-changes">
                                            {release.changes.into_iter()
                                                .map(|change| view! { <li>{change}</li> })
                                                .collect_view()}
                                        </ul>
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
    use time::format_description::well_known::Iso8601;
    use time::Date;

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
                assert!(!change.trim().is_empty(), "{:?} has a blank change entry", release.date);
            }
        }
    }
}
