use crate::game::Game;
use crate::hex::Axial;
use crate::resource::ResourceState;

#[derive(Debug, Clone, PartialEq)]
pub struct SummaryRow {
    pub icon_svg: String,
    pub text: String,
    pub generate_fill: f64,
    pub overfill_fill: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub title: String,
    pub rows: Vec<SummaryRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceDetails {
    pub icon_svg: String,
    pub lines: Vec<(&'static str, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Details {
    pub title: String,
    pub resources: Vec<ResourceDetails>,
}

fn title(pos: Axial) -> String {
    format!("Hex ({}, {})", pos.q, pos.r)
}

fn short(s: &ResourceState) -> String {
    format!(
        "{:.1} / {:.1} / {:.1}",
        s.current,
        s.max_generate,
        s.capacity()
    )
}

fn lines(s: &ResourceState) -> Vec<(&'static str, String)> {
    vec![
        ("Current", format!("{:.1}", s.current)),
        ("Max generate", format!("{:.1}", s.max_generate)),
        ("Max overfill", format!("{:.1}", s.max_overfill)),
        ("Per second", format!("{:.1}", s.per_second)),
    ]
}

pub fn summary(game: &Game, pos: Axial) -> Option<Summary> {
    let cell = game.cell(pos)?;
    Some(Summary {
        title: title(pos),
        rows: cell
            .resources()
            .map(|(_, state, icon)| SummaryRow {
                icon_svg: icon.to_svg(),
                text: short(state),
                generate_fill: state.generate_fill(),
                overfill_fill: state.overfill_fill(),
            })
            .collect(),
    })
}

pub fn details(game: &Game, pos: Axial) -> Option<Details> {
    let cell = game.cell(pos)?;
    Some(Details {
        title: title(pos),
        resources: cell
            .resources()
            .map(|(_, state, icon)| ResourceDetails {
                icon_svg: icon.to_svg(),
                lines: lines(state),
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_hex_has_info() {
        let game = Game::new(0);
        let s = summary(&game, Axial::ORIGIN).unwrap();
        assert_eq!(s.title, "Hex (0, 0)");
        assert!(!s.rows.is_empty());
        let d = details(&game, Axial::ORIGIN).unwrap();
        assert_eq!(d.resources.len(), s.rows.len());
        assert_eq!(d.resources[0].lines.len(), 4);
    }

    #[test]
    fn summary_uses_shorthand() {
        let game = Game::new(0);
        let s = summary(&game, Axial::ORIGIN).unwrap();
        assert_eq!(s.rows[0].text, "0.0 / 10.0 / 100.0");
    }

    #[test]
    fn summary_reports_fills() {
        let mut game = Game::new(0);
        game.gather(Axial::ORIGIN, 5.0).unwrap();
        let s = summary(&game, Axial::ORIGIN).unwrap();
        assert_eq!(s.rows[0].generate_fill, 0.5);
        assert_eq!(s.rows[0].overfill_fill, 0.0);
    }

    #[test]
    fn info_tracks_gathering() {
        let mut game = Game::new(0);
        game.gather(Axial::ORIGIN, 2.0).unwrap();
        let d = details(&game, Axial::ORIGIN).unwrap();
        assert_eq!(d.resources[0].lines[0], ("Current", "2.0".to_string()));
    }

    #[test]
    fn empty_hex_has_no_info() {
        let game = Game::new(0);
        assert_eq!(summary(&game, Axial::new(3, 3)), None);
        assert_eq!(details(&game, Axial::new(3, 3)), None);
    }
}
