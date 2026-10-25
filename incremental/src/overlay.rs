use wasm_bindgen::JsCast;
use web_sys::{Document, HtmlElement, Node};

use crate::error::AppError;
use crate::game::Game;
use crate::hex::{Axial, Point};
use crate::info::{self, Details, Summary, SummaryRow};
use crate::theme::Theme;

const TOOLTIP_OFFSET: f64 = 14.0;

pub struct Overlay {
    tooltip: HtmlElement,
    sidebar: HtmlElement,
    summary: Option<Summary>,
    details: Option<Details>,
}

impl Overlay {
    pub fn new(theme: &Theme) -> Result<Self, AppError> {
        let tooltip = element("tooltip")?;
        let sidebar = element("sidebar")?;
        for el in [&tooltip, &sidebar] {
            let style = el.style();
            style.set_property("background", theme.panel_background)?;
            style.set_property("border-color", theme.panel_border)?;
            style.set_property("color", theme.panel_text)?;
        }
        let style = tooltip.style();
        style.set_property("--bar-background", theme.bar_background)?;
        style.set_property("--bar-generate", theme.bar_generate)?;
        style.set_property("--bar-overfill", theme.bar_overfill)?;
        Ok(Self {
            tooltip,
            sidebar,
            summary: None,
            details: None,
        })
    }

    pub fn set_hover(
        &mut self,
        hover: Option<(Axial, Point)>,
        game: &Game,
        viewport: Point,
    ) -> Result<(), AppError> {
        let summary = hover.and_then(|(pos, _)| info::summary(game, pos));
        let (Some(summary), Some((_, cursor))) = (summary, hover) else {
            self.summary = None;
            self.tooltip.set_hidden(true);
            return Ok(());
        };
        if self.summary.as_ref() != Some(&summary) {
            let document = self.tooltip.owner_document().ok_or(AppError::NoDocument)?;
            self.tooltip.set_text_content(None);
            self.tooltip
                .append_child(&text_element(&document, "div", "title", &summary.title)?)?;
            for row in &summary.rows {
                let el = document.create_element("div")?;
                el.set_class_name("row");
                el.append_child(&icon_element(&document, &row.icon_svg)?)?;
                el.append_child(&bar_element(&document, row)?)?;
                self.tooltip.append_child(&el)?;
            }
            self.summary = Some(summary);
        }
        self.tooltip.set_hidden(false);
        let size = Point::new(
            f64::from(self.tooltip.offset_width()),
            f64::from(self.tooltip.offset_height()),
        );
        let at = tooltip_position(cursor, size, viewport);
        let style = self.tooltip.style();
        style.set_property("left", &format!("{}px", at.x))?;
        style.set_property("top", &format!("{}px", at.y))?;
        Ok(())
    }

    pub fn set_selected(&mut self, selected: Option<Axial>, game: &Game) -> Result<(), AppError> {
        let details = selected.and_then(|pos| info::details(game, pos));
        if self.details == details {
            return Ok(());
        }
        let Some(d) = &details else {
            self.sidebar.set_hidden(true);
            self.details = None;
            return Ok(());
        };
        let document = self.sidebar.owner_document().ok_or(AppError::NoDocument)?;
        self.sidebar.set_text_content(None);
        self.sidebar
            .append_child(&text_element(&document, "h2", "", &d.title)?)?;
        for resource in &d.resources {
            let section = document.create_element("div")?;
            section.set_class_name("resource");
            section.append_child(&icon_element(&document, &resource.icon_svg)?)?;
            for (label, value) in &resource.lines {
                let p = document.create_element("p")?;
                p.append_child(&text_element(&document, "span", "label", label)?)?;
                p.append_child(&text_element(&document, "span", "", value)?)?;
                section.append_child(&p)?;
            }
            self.sidebar.append_child(&section)?;
        }
        self.sidebar.set_hidden(false);
        self.details = details;
        Ok(())
    }
}

fn text_element(document: &Document, tag: &str, class: &str, text: &str) -> Result<Node, AppError> {
    let el = document.create_element(tag)?;
    el.set_class_name(class);
    el.set_text_content(Some(text));
    Ok(el.into())
}

fn bar_element(document: &Document, row: &SummaryRow) -> Result<Node, AppError> {
    let bar = document.create_element("div")?;
    bar.set_class_name("bar");
    for (class, fill) in [
        ("fill generate", row.generate_fill),
        ("fill overfill", row.overfill_fill),
    ] {
        let el = document.create_element("div")?;
        el.set_class_name(class);
        el.set_attribute("style", &format!("width: {}%", fill * 100.0))?;
        bar.append_child(&el)?;
    }
    bar.append_child(&text_element(document, "span", "bar-text", &row.text)?)?;
    Ok(bar.into())
}

fn icon_element(document: &Document, svg: &str) -> Result<Node, AppError> {
    let el = document.create_element("span")?;
    el.set_class_name("icon");
    el.set_inner_html(svg);
    Ok(el.into())
}

fn element(id: &'static str) -> Result<HtmlElement, AppError> {
    web_sys::window()
        .ok_or(AppError::NoWindow)?
        .document()
        .ok_or(AppError::NoDocument)?
        .get_element_by_id(id)
        .ok_or(AppError::ElementNotFound(id))?
        .dyn_into::<HtmlElement>()
        .map_err(|_| AppError::NotAnHtmlElement(id))
}

fn tooltip_position(cursor: Point, size: Point, viewport: Point) -> Point {
    let place = |c: f64, s: f64, v: f64| {
        let after = c + TOOLTIP_OFFSET;
        if after + s <= v {
            after
        } else {
            (c - TOOLTIP_OFFSET - s).max(0.0)
        }
    };
    Point::new(
        place(cursor.x, size.x, viewport.x),
        place(cursor.y, size.y, viewport.y),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEWPORT: Point = Point::new(800.0, 600.0);
    const SIZE: Point = Point::new(100.0, 50.0);

    #[test]
    fn offsets_from_cursor() {
        let p = tooltip_position(Point::new(100.0, 100.0), SIZE, VIEWPORT);
        assert_eq!(p, Point::new(114.0, 114.0));
    }

    #[test]
    fn flips_at_right_edge() {
        let p = tooltip_position(Point::new(750.0, 100.0), SIZE, VIEWPORT);
        assert_eq!(p, Point::new(636.0, 114.0));
    }

    #[test]
    fn flips_at_bottom_edge() {
        let p = tooltip_position(Point::new(100.0, 580.0), SIZE, VIEWPORT);
        assert_eq!(p, Point::new(114.0, 516.0));
    }
}
