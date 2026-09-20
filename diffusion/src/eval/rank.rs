use serde::Serialize;

use crate::eval::{ImageEval, Scored, TitResult, VqaResult};

/// Tournament Borda points per metric, and the resulting rank across metrics.
/// `docs/evaluation.md` calls for aggregating ranks rather than averaging raw
/// scores, since VQAScore and TIT-Score are uncalibrated against each other:
/// VQAScore saturates near 1.0 while TIT-Score is quantized to `k/n` claims, so
/// a plain mean lets whichever metric has more spread on a given prompt decide
/// the winner.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Borda {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vqa: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tit: Option<f32>,
    pub total: f32,
    pub rank: usize,
}

pub(crate) fn vqa_score(scored: &Option<Scored<VqaResult>>) -> Option<f32> {
    match scored {
        Some(Scored::Score(result)) => Some(result.score),
        _ => None,
    }
}

pub(crate) fn tit_score(scored: &Option<Scored<TitResult>>) -> Option<f32> {
    match scored {
        Some(Scored::Score(result)) => Some(result.score),
        _ => None,
    }
}

/// Points for one metric across all images: each image scores
/// `(# images it strictly beats) + 0.5 * (# images it ties)`. An image with no
/// score for this metric (unrequested or `Scored::Failed`) gets 0 and never
/// out-ranks a scored image. `None` when nothing has a score for this metric,
/// so it does not participate.
fn metric_points(scores: &[Option<f32>]) -> Option<Vec<f32>> {
    if scores.iter().all(Option::is_none) {
        return None;
    }
    let points = scores
        .iter()
        .map(|score| {
            let Some(score) = score else {
                return 0.0;
            };
            scores
                .iter()
                .filter_map(|other| *other)
                .map(|other| match score.total_cmp(&other) {
                    std::cmp::Ordering::Greater => 1.0,
                    std::cmp::Ordering::Equal => 0.5,
                    std::cmp::Ordering::Less => 0.0,
                })
                .sum::<f32>()
                - 0.5 // exclude the image's own tie with itself
        })
        .collect();
    Some(points)
}

/// One [`Borda`] per image, or `None` if neither metric was requested (or every
/// request failed). Competition ranking: ties share a rank and the next rank
/// skips ahead (`1, 2, 2, 4`).
pub fn aggregate(evals: &[ImageEval]) -> Option<Vec<Borda>> {
    let vqa_scores: Vec<Option<f32>> = evals.iter().map(|e| vqa_score(&e.vqa)).collect();
    let tit_scores: Vec<Option<f32>> = evals.iter().map(|e| tit_score(&e.tit)).collect();

    let vqa_points = metric_points(&vqa_scores);
    let tit_points = metric_points(&tit_scores);
    if vqa_points.is_none() && tit_points.is_none() {
        return None;
    }

    let totals: Vec<f32> = (0..evals.len())
        .map(|i| {
            vqa_points.as_ref().map(|p| p[i]).unwrap_or(0.0)
                + tit_points.as_ref().map(|p| p[i]).unwrap_or(0.0)
        })
        .collect();
    let ranks = competition_rank(&totals);

    Some(
        (0..evals.len())
            .map(|i| Borda {
                vqa: vqa_points.as_ref().map(|p| p[i]),
                tit: tit_points.as_ref().map(|p| p[i]),
                total: totals[i],
                rank: ranks[i],
            })
            .collect(),
    )
}

/// 1-based competition ranking (`1, 2, 2, 4`), descending by value.
fn competition_rank(totals: &[f32]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..totals.len()).collect();
    order.sort_by(|&a, &b| totals[b].total_cmp(&totals[a]));

    let mut ranks = vec![0; totals.len()];
    for (position, &index) in order.iter().enumerate() {
        ranks[index] = match position {
            0 => 1,
            _ => {
                let prev = order[position - 1];
                if totals[index] == totals[prev] {
                    ranks[prev]
                } else {
                    position + 1
                }
            }
        };
    }
    ranks
}

/// Indices into `evals`, ascending by total Borda points so the last index is
/// the best-scoring image. `None` when [`aggregate`] would be `None`, so
/// callers fall back to generation order. Stable, so ties keep their original
/// relative order.
pub fn display_order(evals: &[ImageEval]) -> Option<Vec<usize>> {
    let scores = aggregate(evals)?;
    let mut order: Vec<usize> = (0..evals.len()).collect();
    order.sort_by(|&a, &b| scores[a].total.total_cmp(&scores[b].total));
    Some(order)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vqa(score: f32) -> Option<Scored<VqaResult>> {
        Some(Scored::Score(VqaResult { score }))
    }

    fn tit(score: f32) -> Option<Scored<TitResult>> {
        Some(Scored::Score(TitResult {
            score,
            claims: Vec::new(),
        }))
    }

    fn eval(vqa_score: Option<f32>, tit_score: Option<f32>) -> ImageEval {
        ImageEval {
            vqa: vqa_score.and_then(vqa),
            tit: tit_score.and_then(tit),
        }
    }

    #[test]
    fn single_metric_orders_by_that_score_ascending() {
        let evals = vec![
            eval(Some(0.5), None),
            eval(Some(0.9), None),
            eval(Some(0.1), None),
        ];
        assert_eq!(display_order(&evals), Some(vec![2, 0, 1]));
    }

    #[test]
    fn both_metrics_use_borda_points_not_a_mean() {
        // A mean would pick copy 1 too here, but by a margin that hides how
        // little the vqa spread (0.91-0.97) actually says versus tit
        // (0.40-1.00). Borda should still land on 1 as the clear winner.
        let evals = vec![
            eval(Some(0.97), Some(0.40)),
            eval(Some(0.94), Some(1.00)),
            eval(Some(0.91), Some(0.60)),
        ];
        let scores = aggregate(&evals).unwrap();
        assert_eq!(scores[0].total, 2.0);
        assert_eq!(scores[1].total, 3.0);
        assert_eq!(scores[2].total, 1.0);
        assert_eq!(scores[1].rank, 1);
        assert_eq!(display_order(&evals), Some(vec![2, 0, 1]));
    }

    #[test]
    fn tied_scores_split_points_and_sum_to_combinations() {
        let evals = vec![
            eval(Some(0.5), None),
            eval(Some(0.5), None),
            eval(Some(0.9), None),
        ];
        let scores = aggregate(&evals).unwrap();
        assert_eq!(scores[0].vqa, Some(0.5));
        assert_eq!(scores[1].vqa, Some(0.5));
        assert_eq!(scores[2].vqa, Some(2.0));
        // n=3 images -> C(3,2) = 3 total points distributed across all of them
        let total: f32 = scores.iter().map(|s| s.vqa.unwrap()).sum();
        assert_eq!(total, 3.0);
        assert_eq!(scores[0].rank, scores[1].rank);
        assert!(scores[2].rank < scores[0].rank);
    }

    #[test]
    fn failed_metric_scores_zero_points_but_other_metric_still_ranks() {
        let evals = vec![
            ImageEval {
                vqa: Some(Scored::Failed {
                    error: "boom".into(),
                }),
                tit: tit(0.2),
            },
            eval(None, Some(0.8)),
        ];
        let scores = aggregate(&evals).unwrap();
        assert_eq!(scores[0].vqa, None);
        assert_eq!(scores[0].tit, Some(0.0));
        assert_eq!(scores[1].tit, Some(1.0));
        assert_eq!(scores[1].rank, 1);
    }

    #[test]
    fn no_scores_at_all_returns_none() {
        let evals = vec![eval(None, None), eval(None, None)];
        assert_eq!(aggregate(&evals), None);
        assert_eq!(display_order(&evals), None);
    }

    #[test]
    fn equal_totals_are_stable_in_generation_order() {
        let evals = vec![
            eval(Some(0.5), None),
            eval(Some(0.5), None),
            eval(Some(0.5), None),
        ];
        assert_eq!(display_order(&evals), Some(vec![0, 1, 2]));
    }
}
