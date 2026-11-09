use crate::rock::{Rock, RockLayer};

#[derive(Clone, Debug)]
pub struct TileGeology {
    pub bottom: f32,
    pub layers: Vec<RockLayer>,
    pub mountainosity: f32,
    pub subduction: f32,
}

impl TileGeology {
    pub const MAX_HEIGHT: f32 = 10.0;
    pub const MIN_THICKNESS: f32 = 4.0;
    pub const MAX_THICKNESS: f32 = 75.0;

    /// Continental column initialized with a base crust of 9 km + surface relief `h`.
    pub fn empty_land(rock: Rock, h: f32) -> Self {
        Self {
            bottom: -9.0,
            layers: vec![RockLayer {
                rock,
                thickness: 9.0 + h,
            }],
            mountainosity: 0.0,
            subduction: 0.0,
        }
    }

    /// Oceanic column initialized with 5 km basaltic crust at abyssal depth (`elevation = -4.0 km`).
    pub fn empty_ocean(rock: Rock) -> Self {
        Self::empty_ocean_with_depth(rock, -4.0)
    }

    /// Oceanic column initialized with 5 km basaltic crust at the specified submarine elevation (`<= 0.0 km`).
    pub fn empty_ocean_with_depth(rock: Rock, elevation_km: f32) -> Self {
        let clamped_elev = elevation_km.min(-0.05);
        Self {
            bottom: clamped_elev - 5.0,
            layers: vec![RockLayer {
                rock,
                thickness: 5.0,
            }],
            mountainosity: 0.0,
            subduction: 0.0,
        }
    }

    #[inline]
    pub fn thickness(&self) -> f32 {
        self.layers.iter().map(|l| l.thickness).sum()
    }

    /// Signed elevation in km (`> 0` for land, `<= 0` for ocean floor).
    #[inline]
    pub fn elevation(&self) -> f32 {
        self.bottom + self.thickness()
    }

    #[inline]
    pub fn top_felsity(&self) -> f32 {
        self.layers.last().map(|l| l.rock.felsity).unwrap_or(0.5)
    }

    /// Merges multiple rock layer columns by voting from top to bottom (`Tile.mergelayers` in `tile.py`).
    pub fn merge_layers<R: Clone + PartialEq>(
        sources: &[(f32, f32, Vec<(R, f32)>)],
    ) -> (f32, f32, Vec<(R, f32)>) {
        let bottom = sources
            .iter()
            .map(|s| s.0)
            .fold(f32::INFINITY, f32::min);
        let max_elev = sources
            .iter()
            .map(|s| s.0 + s.2.iter().map(|l| l.1).sum::<f32>())
            .fold(f32::NEG_INFINITY, f32::max);

        let mut ss: Vec<Vec<(Option<R>, f32)>> = Vec::with_capacity(sources.len());
        for (b, _, layers) in sources {
            let mut padded = Vec::new();
            let db = *b - bottom;
            if db > 0.0 {
                padded.push((None, db));
            }
            for (r, t) in layers {
                padded.push((Some(r.clone()), *t));
            }
            let top = *b + layers.iter().map(|l| l.1).sum::<f32>();
            let de = max_elev - top;
            if de > 0.0 {
                padded.push((None, de));
            }
            ss.push(padded);
        }

        let mut output: Vec<(R, f32)> = Vec::new();
        let mut last_r: Option<R> = None;

        while ss.iter().any(|s| !s.is_empty()) {
            let min_t = ss
                .iter()
                .filter_map(|s| s.last().map(|l| l.1))
                .fold(f32::INFINITY, f32::min);

            let mut votes: Vec<(R, usize)> = Vec::new();
            for s in &mut ss {
                if let Some((rock_opt, thick)) = s.pop() {
                    if thick > min_t {
                        s.push((rock_opt.clone(), thick - min_t));
                    }
                    if let Some(r) = rock_opt {
                        if let Some(entry) = votes.iter_mut().find(|(vr, _)| *vr == r) {
                            entry.1 += 1;
                        } else {
                            votes.push((r, 1));
                        }
                    }
                }
            }

            if votes.is_empty() {
                continue;
            }

            let max_vote = votes.iter().map(|(_, c)| *c).max().unwrap();
            let mut winners: Vec<R> = votes
                .into_iter()
                .filter(|(_, c)| *c == max_vote)
                .map(|(r, _)| r)
                .collect();

            if winners.len() > 1 {
                if let Some(ref prev) = last_r {
                    if let Some(pos) = winners.iter().position(|w| w == prev) {
                        winners.remove(pos);
                    }
                }
            }

            if let Some(chosen) = winners.into_iter().next() {
                last_r = Some(chosen.clone());
                output.push((chosen, min_t));
            }
        }

        // Compact adjacent identical layers and drop zero-thickness layers
        let mut i = 0;
        while i + 1 < output.len() {
            if output[i].1 == 0.0 {
                output.remove(i);
            } else if output[i].0 == output[i + 1].0 {
                let next_t = output.remove(i + 1).1;
                output[i].1 += next_t;
            } else {
                i += 1;
            }
        }

        output.reverse();
        let mountainosity =
            sources.iter().map(|s| s.1).sum::<f32>() / (sources.len() as f32);

        (bottom, mountainosity, output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_layers() {
        let sources = vec![
            (-15.0, 0.0, vec![('A', 10.0), ('B', 18.0), ('C', 10.0)]),
            (
                -35.0,
                0.0,
                vec![('A', 9.0), ('D', 27.0), ('B', 7.0), ('E', 10.0), ('C', 12.0)],
            ),
            (
                -18.0,
                0.0,
                vec![('A', 7.0), ('E', 4.0), ('B', 10.0), ('C', 17.0)],
            ),
        ];

        let (bottom, _, layers) = TileGeology::merge_layers(&sources);
        assert_eq!(bottom, -35.0);
        assert_eq!(
            layers,
            vec![
                ('A', 9.0),
                ('D', 11.0),
                ('A', 4.0),
                ('D', 4.0),
                ('A', 2.0),
                ('B', 18.0),
                ('C', 17.0)
            ]
        );
    }
}
