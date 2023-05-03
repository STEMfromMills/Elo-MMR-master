//! Topcoder system details: https://www.topcoder.com/community/competitive-programming/how-to-compete/ratings
//! Further analysis: https://web.archive.org/web/20120417104152/http://brucemerry.org.za:80/tc-rating/rating_submit1.pdf

use super::{Player, Rating, RatingSystem};
use crate::data_processing::ContestRatingParams;
use crate::numerical::{standard_normal_cdf, standard_normal_cdf_inv};
use rayon::prelude::*;

#[derive(Debug)]
pub struct TopcoderSys {
    pub weight_noob: f64,  // must be positive
    pub weight_limit: f64, // must be positive
}

impl Default for TopcoderSys {
    fn default() -> Self {
        Self {
            weight_noob: 0.6,
            weight_limit: 0.18,
        }
    }
}

impl TopcoderSys {
    fn win_probability(&self, sqrt_weight: f64, player: &Rating, foe: &Rating) -> f64 {
        let z = sqrt_weight * (player.mu - foe.mu) / player.sig.hypot(foe.sig);
        standard_normal_cdf(z)
    }
}

impl RatingSystem for TopcoderSys {
    fn round_update(
        &self,
        params: ContestRatingParams,
        standings: Vec<(&mut Player, usize, usize)>,
    ) {
        let num_coders = standings.len() as f64;
        let ave_rating = standings
            .iter()
            .map(|&(ref player, _, _)| player.approx_posterior.mu)
            .sum::<f64>()
            / num_coders;

        let c_factor = {
            let mut mean_vol_sq = standings
                .iter()
                .map(|&(ref player, _, _)| player.approx_posterior.sig.powi(2))
                .sum::<f64>()
                / num_coders;
            if num_coders > 1. {
                mean_vol_sq += standings
                    .iter()
                    .map(|&(ref player, _, _)| (player.approx_posterior.mu - ave_rating).powi(2))
                    .sum::<f64>()
                    / (num_coders - 1.);
            }
            mean_vol_sq.sqrt()
        };

        let sqrt_contest_weight = params.weight.sqrt();
        let weight_extra = self.weight_noob - self.weight_limit;
        let new_ratings: Vec<(Rating, f64)> = standings
            .par_iter()
            .map(|(player, lo, hi)| {
                let old_rating = player.approx_posterior.mu;
                let vol_sq = player.approx_posterior.sig.powi(2);

                let ex_rank = standings
                    .iter()
                    .map(|&(ref foe, _, _)| {
                        self.win_probability(
                            sqrt_contest_weight,
                            &foe.approx_posterior,
                            &player.approx_posterior,
                        )
                    })
                    .sum::<f64>();
                let ac_rank = 0.5 * (1 + lo + hi) as f64;

                // cdf(-perf) = rank / num_coders
                //   => perf  = -inverse_cdf(rank / num_coders)
                let ex_perf = -standard_normal_cdf_inv(ex_rank / num_coders);
                let ac_perf = -standard_normal_cdf_inv(ac_rank / num_coders);
                let perf_as = old_rating + c_factor * (ac_perf - ex_perf);
                let perf_as = perf_as.min(params.perf_ceiling);

                let num_contests = player.times_played() as f64;
                let mut weight = self.weight_limit + weight_extra / num_contests;
                let mut cap = 150. + 1500. / (num_contests + 1.);
                cap *= sqrt_contest_weight * weight / (0.18 + 0.42 / num_contests);

                weight *= params.weight / (1. - weight);
                if old_rating >= 2500. {
                    weight *= 0.8;
                } else if old_rating >= 2000. {
                    weight *= 0.9;
                }

                let try_rating = (old_rating + weight * perf_as) / (1. + weight);
                let new_rating = try_rating.clamp(old_rating - cap, old_rating + cap);
                let new_vol =
                    ((try_rating - old_rating).powi(2) / weight + vol_sq / (1. + weight)).sqrt();

                (
                    Rating {
                        mu: new_rating,
                        sig: new_vol,
                    },
                    perf_as,
                )
            })
            .collect();

        standings.into_par_iter().zip(new_ratings).for_each(
            |((player, _, _), (new_rating, new_perf))| {
                player.update_rating(new_rating, new_perf);
            },
        );
    }
}
