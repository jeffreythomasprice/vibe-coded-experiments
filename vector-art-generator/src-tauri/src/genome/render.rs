use super::geometry::evaluate_csg;
use super::types::*;

const VIEWBOX_SIZE: f64 = 400.0;

pub fn render_genome(genome: &Genome) -> String {
    let result = evaluate_csg(&genome.tree);

    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {vb} {vb}" width="{vb}" height="{vb}">"#,
        vb = VIEWBOX_SIZE
    );

    svg.push_str(&format!(
        r#"<rect width="{vb}" height="{vb}" fill="white"/>"#,
        vb = VIEWBOX_SIZE
    ));

    for polygon in result.0.iter() {
        let mut d = String::new();

        // Exterior ring
        let exterior = polygon.exterior();
        if exterior.0.len() >= 3 {
            for (i, coord) in exterior.0.iter().enumerate() {
                if i == 0 {
                    d.push_str(&format!("M{:.2},{:.2}", coord.x, coord.y));
                } else {
                    d.push_str(&format!(" L{:.2},{:.2}", coord.x, coord.y));
                }
            }
            d.push('Z');
        }

        // Interior rings (holes from subtraction)
        for interior in polygon.interiors() {
            if interior.0.len() >= 3 {
                for (i, coord) in interior.0.iter().enumerate() {
                    if i == 0 {
                        d.push_str(&format!(" M{:.2},{:.2}", coord.x, coord.y));
                    } else {
                        d.push_str(&format!(" L{:.2},{:.2}", coord.x, coord.y));
                    }
                }
                d.push('Z');
            }
        }

        if !d.is_empty() {
            svg.push_str(&format!(
                r#"<path d="{d}" fill="none" stroke="black" stroke-width="2" fill-rule="evenodd"/>"#,
                d = d
            ));
        }
    }

    svg.push_str("</svg>");
    svg
}
